//! SQLite storage.
//!
//! One connection behind a mutex. Queries are small and local, so callers may
//! use the store directly from async code; never hold a lock across an await
//! (the API never hands out the guard, so this can't happen by accident).

use std::{path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::{
    Agent, AgentPatch, ChatPreview, Device, Message, MessageResult, MessageRole, MessageStatus, NewAgent,
    Problem, Thread, ToolLogEntry, new_id, now_ms,
};

pub type Result<T> = rusqlite::Result<T>;

/// Schema migrations, applied in order; `PRAGMA user_version` records progress.
const MIGRATIONS: &[&str] = &[r#"
    CREATE TABLE agents (
        id            TEXT PRIMARY KEY,
        name          TEXT NOT NULL,
        description   TEXT NOT NULL DEFAULT '',
        avatar        TEXT NOT NULL DEFAULT '',
        color         TEXT NOT NULL DEFAULT '',
        system_prompt TEXT NOT NULL DEFAULT '',
        provider      TEXT NOT NULL,
        model         TEXT NOT NULL,
        created_at    INTEGER NOT NULL,
        updated_at    INTEGER NOT NULL
    );
    CREATE TABLE threads (
        id         TEXT PRIMARY KEY,
        agent_id   TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
        title      TEXT NOT NULL DEFAULT '',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX threads_by_agent ON threads(agent_id, updated_at DESC);
    CREATE TABLE messages (
        id                TEXT PRIMARY KEY,
        thread_id         TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
        role              TEXT NOT NULL,
        content           TEXT NOT NULL DEFAULT '',
        reasoning         TEXT NOT NULL DEFAULT '',
        status            TEXT NOT NULL,
        error             TEXT,
        model             TEXT,
        prompt_tokens     INTEGER,
        completion_tokens INTEGER,
        created_at        INTEGER NOT NULL
    );
    CREATE INDEX messages_by_thread ON messages(thread_id, id);
    CREATE TABLE devices (
        id           TEXT PRIMARY KEY,
        name         TEXT NOT NULL,
        token_hash   TEXT NOT NULL UNIQUE,
        created_at   INTEGER NOT NULL,
        last_seen_at INTEGER
    );
"#,
// v2 (Phase 3): tools per agent, reply rounds (tool steps), audit log.
r#"
    ALTER TABLE agents ADD COLUMN tools TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE messages ADD COLUMN rounds TEXT NOT NULL DEFAULT '[]';
    -- Existing agents get the safe, read-only tools.
    UPDATE agents SET tools = '["web_search","web_fetch","get_time"]';
    CREATE TABLE tool_log (
        id          TEXT PRIMARY KEY,
        agent_id    TEXT NOT NULL,
        thread_id   TEXT NOT NULL,
        message_id  TEXT NOT NULL,
        tool        TEXT NOT NULL,
        arguments   TEXT NOT NULL,
        status      TEXT NOT NULL,
        summary     TEXT NOT NULL DEFAULT '',
        started_at  INTEGER NOT NULL,
        duration_ms INTEGER NOT NULL DEFAULT 0
    );
    CREATE INDEX tool_log_by_time ON tool_log(started_at DESC);
"#,
// v3: one default model for every agent (empty = follow the default).
r#"
    UPDATE agents SET provider = '', model = '';
"#,
// v4: a main (boss) agent, and who created each agent.
r#"
    ALTER TABLE agents ADD COLUMN role TEXT NOT NULL DEFAULT '';
    ALTER TABLE agents ADD COLUMN created_by TEXT NOT NULL DEFAULT '';
"#,
// v5: how much of the model's context a reply filled (for the context ring).
r#"
    ALTER TABLE messages ADD COLUMN context_tokens INTEGER;
"#,
// v6: a usage ledger, so deleting conversations (or one-off agents) doesn't
// give tokens back to the daily / weekly limits.
r#"
    CREATE TABLE usage (
        message_id TEXT PRIMARY KEY,
        created_at INTEGER NOT NULL,
        tokens     INTEGER NOT NULL
    );
    CREATE INDEX usage_created ON usage(created_at);
    INSERT INTO usage (message_id, created_at, tokens)
        SELECT id, created_at, COALESCE(prompt_tokens, 0) + COALESCE(completion_tokens, 0)
        FROM messages WHERE prompt_tokens IS NOT NULL OR completion_tokens IS NOT NULL;
"#,
// v7: how hard each agent thinks (empty = follow the default in Settings).
r#"
    ALTER TABLE agents ADD COLUMN reasoning_effort TEXT NOT NULL DEFAULT '';
"#,
// v8: auto mode (a safety check approves safe commands instead of the user).
r#"
    ALTER TABLE agents ADD COLUMN auto_approve INTEGER NOT NULL DEFAULT 0;
"#,
// v9: one ongoing chat per agent, compacted when it grows long.
r#"
    ALTER TABLE threads ADD COLUMN summary TEXT NOT NULL DEFAULT '';
    ALTER TABLE threads ADD COLUMN summary_until INTEGER NOT NULL DEFAULT 0;
"#,
// v10: "Full auto" (no review; only the hard rules ask the user).
r#"
    ALTER TABLE agents ADD COLUMN auto_full INTEGER NOT NULL DEFAULT 0;
"#,
// v11: files attached to user messages.
r#"
    ALTER TABLE messages ADD COLUMN attachments TEXT NOT NULL DEFAULT '[]';
"#,
// v12: routines (scheduled tasks and commands).
r#"
    CREATE TABLE routines (
        id          TEXT PRIMARY KEY,
        agent_id    TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
        name        TEXT NOT NULL,
        schedule    TEXT NOT NULL,
        kind        TEXT NOT NULL,
        body        TEXT NOT NULL,
        notify      TEXT NOT NULL DEFAULT 'failure',
        enabled     INTEGER NOT NULL DEFAULT 1,
        next_run    INTEGER,
        last_run    INTEGER,
        last_result TEXT NOT NULL DEFAULT '',
        created_at  INTEGER NOT NULL
    );
    CREATE INDEX routines_due ON routines(enabled, next_run);
"#,
// v13: full-text search over messages (`search_chats`), kept in step by
// triggers. Content is written once per message (insert, then the finished
// reply), so this costs next to nothing.
r#"
    CREATE VIRTUAL TABLE messages_fts USING fts5(content, tokenize = 'unicode61 remove_diacritics 2');
    INSERT INTO messages_fts(rowid, content) SELECT rowid, content FROM messages WHERE content != '';
    CREATE TRIGGER messages_fts_insert AFTER INSERT ON messages WHEN new.content != '' BEGIN
        INSERT INTO messages_fts(rowid, content) VALUES (new.rowid, new.content);
    END;
    CREATE TRIGGER messages_fts_delete AFTER DELETE ON messages BEGIN
        DELETE FROM messages_fts WHERE rowid = old.rowid;
    END;
    CREATE TRIGGER messages_fts_update AFTER UPDATE OF content ON messages BEGIN
        DELETE FROM messages_fts WHERE rowid = old.rowid;
        INSERT INTO messages_fts(rowid, content) SELECT new.rowid, new.content WHERE new.content != '';
    END;
"#,
// v14: the problems inbox (failed replies and routines).
r#"
    CREATE TABLE problems (
        id          TEXT PRIMARY KEY,
        kind        TEXT NOT NULL,
        agent_id    TEXT NOT NULL,
        thread_id   TEXT NOT NULL DEFAULT '',
        message_id  TEXT NOT NULL DEFAULT '',
        routine_id  TEXT NOT NULL DEFAULT '',
        title       TEXT NOT NULL,
        detail      TEXT NOT NULL DEFAULT '',
        created_at  INTEGER NOT NULL,
        resolved_at INTEGER
    );
    CREATE INDEX problems_open ON problems(resolved_at, created_at);
"#];

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        Self::init(Connection::open(path)?)
    }

    /// Throwaway database, for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.pragma_update(None, "busy_timeout", 5000)?;
        // WAL keeps readers and the writer from blocking each other; in-memory
        // databases silently stay in "memory" mode, which is fine.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;

        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let conn = self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        f(&conn)
    }

    // ---- agents -----------------------------------------------------------

    pub fn list_agents(&self) -> Result<Vec<Agent>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!("SELECT {AGENT_COLS} FROM agents ORDER BY created_at"))?;
            let rows = stmt.query_map([], agent_from_row)?.collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_agent(&self, id: &str) -> Result<Option<Agent>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {AGENT_COLS} FROM agents WHERE id = ?1"), [id], agent_from_row)
                .optional()
        })
    }

    pub fn create_agent(&self, new: NewAgent) -> Result<Agent> {
        let now = now_ms();
        let agent = Agent {
            id: new_id(),
            name: new.name,
            description: new.description,
            avatar: new.avatar,
            color: new.color,
            system_prompt: new.system_prompt,
            provider: new.provider,
            model: new.model,
            reasoning_effort: new.reasoning_effort,
            tools: new.tools,
            role: new.role,
            created_by: new.created_by,
            auto_approve: false,
            auto_full: false,
            created_at: now,
            updated_at: now,
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO agents (id, name, description, avatar, color, system_prompt, provider, model, reasoning_effort, tools, role, created_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    agent.id, agent.name, agent.description, agent.avatar, agent.color,
                    agent.system_prompt, agent.provider, agent.model, agent.reasoning_effort,
                    to_json(&agent.tools), agent.role, agent.created_by, agent.created_at, agent.updated_at
                ],
            )
        })?;
        Ok(agent)
    }

    /// Makes `id` the main agent (and nobody else).
    pub fn set_main_agent(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE agents SET role = CASE WHEN id = ?1 THEN 'main' ELSE '' END", [id])
        })?;
        Ok(())
    }

    /// Changes an agent's role (e.g. a one-off agent the coordinator keeps).
    pub fn set_agent_role(&self, id: &str, role: &str) -> Result<bool> {
        let n = self.with(|c| c.execute("UPDATE agents SET role = ?2 WHERE id = ?1 AND role != 'main'", [id, role]))?;
        Ok(n > 0)
    }

    /// Records tokens spent outside a reply (e.g. auto mode's safety checks).
    pub fn add_usage(&self, tokens: i64) -> Result<()> {
        if tokens <= 0 {
            return Ok(());
        }
        self.with(|c| {
            c.execute(
                "INSERT INTO usage (message_id, created_at, tokens) VALUES (?1, ?2, ?3)",
                params![format!("extra-{}", new_id()), now_ms(), tokens],
            )
        })?;
        Ok(())
    }

    /// Tokens (prompt + completion) of replies started at or after `since_ms`.
    pub fn tokens_used_since(&self, since_ms: i64) -> Result<i64> {
        self.with(|c| {
            c.query_row(
                "SELECT COALESCE(SUM(tokens), 0) FROM usage WHERE created_at >= ?1",
                [since_ms],
                |r| r.get(0),
            )
        })
    }

    pub fn update_agent(&self, id: &str, patch: AgentPatch) -> Result<Option<Agent>> {
        let Some(mut a) = self.get_agent(id)? else { return Ok(None) };
        if let Some(v) = patch.name { a.name = v }
        if let Some(v) = patch.description { a.description = v }
        if let Some(v) = patch.avatar { a.avatar = v }
        if let Some(v) = patch.color { a.color = v }
        if let Some(v) = patch.system_prompt { a.system_prompt = v }
        if let Some(v) = patch.provider { a.provider = v }
        if let Some(v) = patch.model { a.model = v }
        if let Some(v) = patch.reasoning_effort { a.reasoning_effort = v }
        if let Some(v) = patch.tools { a.tools = v }
        if let Some(v) = patch.auto_approve { a.auto_approve = v }
        if let Some(v) = patch.auto_full { a.auto_full = v }
        a.updated_at = now_ms();
        self.with(|c| {
            c.execute(
                "UPDATE agents SET name = ?2, description = ?3, avatar = ?4, color = ?5,
                    system_prompt = ?6, provider = ?7, model = ?8, reasoning_effort = ?9, tools = ?10,
                    updated_at = ?11, auto_approve = ?12, auto_full = ?13 WHERE id = ?1",
                params![
                    a.id, a.name, a.description, a.avatar, a.color, a.system_prompt, a.provider,
                    a.model, a.reasoning_effort, to_json(&a.tools), a.updated_at, a.auto_approve, a.auto_full
                ],
            )
        })?;
        Ok(Some(a))
    }

    /// Deletes the agent with all of its threads and messages.
    pub fn delete_agent(&self, id: &str) -> Result<bool> {
        self.with(|c| Ok(c.execute("DELETE FROM agents WHERE id = ?1", [id])? > 0))
    }

    // ---- threads ----------------------------------------------------------

    /// Threads of one agent, most recently active first.
    pub fn list_threads(&self, agent_id: &str) -> Result<Vec<Thread>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {THREAD_COLS} FROM threads WHERE agent_id = ?1 ORDER BY updated_at DESC"
            ))?;
            let rows = stmt.query_map([agent_id], thread_from_row)?.collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_thread(&self, id: &str) -> Result<Option<Thread>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {THREAD_COLS} FROM threads WHERE id = ?1"), [id], thread_from_row)
                .optional()
        })
    }

    pub fn create_thread(&self, agent_id: &str, title: &str) -> Result<Thread> {
        let now = now_ms();
        let thread = Thread {
            id: new_id(),
            agent_id: agent_id.to_owned(),
            title: title.to_owned(),
            created_at: now,
            updated_at: now,
            summary: String::new(),
            summary_until: 0,
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO threads (id, agent_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![thread.id, thread.agent_id, thread.title, thread.created_at, thread.updated_at],
            )
        })?;
        Ok(thread)
    }

    /// The most recently active threads across all agents.
    pub fn list_recent_threads(&self, limit: u32) -> Result<Vec<Thread>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {THREAD_COLS} FROM threads ORDER BY updated_at DESC LIMIT ?1"
            ))?;
            let rows = stmt.query_map([limit], thread_from_row)?.collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn rename_thread(&self, id: &str, title: &str) -> Result<Option<Thread>> {
        self.with(|c| {
            c.execute("UPDATE threads SET title = ?2 WHERE id = ?1", params![id, title])
        })?;
        self.get_thread(id)
    }

    /// Marks the thread as active now, which moves it to the top of its list.
    pub fn touch_thread(&self, id: &str) -> Result<Option<Thread>> {
        self.with(|c| {
            c.execute("UPDATE threads SET updated_at = ?2 WHERE id = ?1", params![id, now_ms()])
        })?;
        self.get_thread(id)
    }

    /// An agent's one ongoing chat, created on first use.
    pub fn chat_thread(&self, agent_id: &str, title: &str) -> Result<Thread> {
        match self.list_threads(agent_id)?.into_iter().next() {
            Some(t) => Ok(t),
            None => self.create_thread(agent_id, title),
        }
    }

    /// Keeps only each agent's most recent thread (agents have one chat now).
    /// Returns how many threads were removed.
    pub fn keep_one_thread_per_agent(&self) -> Result<usize> {
        self.with(|c| {
            c.execute(
                "DELETE FROM threads WHERE id NOT IN (
                    SELECT id FROM threads t WHERE t.updated_at = (
                        SELECT MAX(updated_at) FROM threads u WHERE u.agent_id = t.agent_id)
                    GROUP BY agent_id)",
                [],
            )
        })
    }

    /// Empties a chat: every message and its summary go.
    pub fn clear_thread(&self, id: &str) -> Result<Option<Thread>> {
        self.with(|c| {
            c.execute("DELETE FROM messages WHERE thread_id = ?1", [id])?;
            c.execute("UPDATE threads SET summary = '', summary_until = 0, updated_at = ?2 WHERE id = ?1", params![id, now_ms()])
        })?;
        self.get_thread(id)
    }

    /// Saves a compaction: `summary` stands in for messages up to `until`.
    pub fn set_summary(&self, id: &str, summary: &str, until: i64) -> Result<Option<Thread>> {
        self.with(|c| {
            c.execute("UPDATE threads SET summary = ?2, summary_until = ?3 WHERE id = ?1", params![id, summary, until])
        })?;
        self.get_thread(id)
    }

    pub fn delete_thread(&self, id: &str) -> Result<bool> {
        self.with(|c| Ok(c.execute("DELETE FROM threads WHERE id = ?1", [id])? > 0))
    }

    // ---- messages ---------------------------------------------------------

    /// Each agent's latest chat line: what the user or the agent last said
    /// (engine notes like routine starts and team updates are skipped).
    pub fn chat_previews(&self) -> Result<Vec<ChatPreview>> {
        let rows = self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT t.agent_id, m.role, m.content, m.created_at FROM threads t
                 JOIN messages m ON m.id = (
                     SELECT id FROM messages WHERE thread_id = t.id AND role IN ('user', 'assistant')
                       AND content != '' AND content NOT LIKE '[I.R.I.S. %' AND content NOT LIKE '[Routine:%'
                     ORDER BY id DESC LIMIT 1)
                 ORDER BY m.created_at DESC",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(ChatPreview { agent_id: r.get(0)?, role: r.get(1)?, text: r.get(2)?, created_at: r.get(3)? })
                })?
                .collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })?;
        let mut seen = std::collections::HashSet::new();
        Ok(rows
            .into_iter()
            .filter(|p| seen.insert(p.agent_id.clone()))
            .map(|mut p| {
                p.text = p.text.chars().take(160).collect();
                p
            })
            .collect())
    }

    /// All messages of a thread, oldest first.
    pub fn list_messages(&self, thread_id: &str) -> Result<Vec<Message>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {MESSAGE_COLS} FROM messages WHERE thread_id = ?1 ORDER BY id"
            ))?;
            let rows = stmt.query_map([thread_id], message_from_row)?.collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_message(&self, id: &str) -> Result<Option<Message>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {MESSAGE_COLS} FROM messages WHERE id = ?1"), [id], message_from_row)
                .optional()
        })
    }

    /// The newest message of a thread, if any.
    pub fn last_message(&self, thread_id: &str) -> Result<Option<Message>> {
        self.with(|c| {
            c.query_row(
                &format!("SELECT {MESSAGE_COLS} FROM messages WHERE thread_id = ?1 ORDER BY id DESC LIMIT 1"),
                [thread_id],
                message_from_row,
            )
            .optional()
        })
    }

    // ---- problems ---------------------------------------------------------

    pub fn add_problem(&self, p: &Problem) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO problems (id, kind, agent_id, thread_id, message_id, routine_id, title, detail, created_at, resolved_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![p.id, p.kind, p.agent_id, p.thread_id, p.message_id, p.routine_id, p.title, p.detail, p.created_at, p.resolved_at],
            )
            .map(|_| ())
        })
    }

    /// Open problems, newest first.
    pub fn open_problems(&self) -> Result<Vec<Problem>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {PROBLEM_COLS} FROM problems WHERE resolved_at IS NULL ORDER BY created_at DESC LIMIT 200"
            ))?;
            let rows = stmt.query_map([], problem_from_row)?.collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_problem(&self, id: &str) -> Result<Option<Problem>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {PROBLEM_COLS} FROM problems WHERE id = ?1"), [id], problem_from_row).optional()
        })
    }

    /// Marks one problem (or, with `None`, every open one) as dealt with.
    /// Returns how many changed.
    pub fn resolve_problems(&self, id: Option<&str>) -> Result<usize> {
        self.with(|c| match id {
            Some(id) => c.execute("UPDATE problems SET resolved_at = ?2 WHERE id = ?1 AND resolved_at IS NULL", params![id, now_ms()]),
            None => c.execute("UPDATE problems SET resolved_at = ?1 WHERE resolved_at IS NULL", [now_ms()]),
        })
    }

    /// A chat answered fine again: its failed-reply problems are over.
    pub fn resolve_thread_problems(&self, thread_id: &str) -> Result<usize> {
        self.with(|c| {
            c.execute(
                "UPDATE problems SET resolved_at = ?2 WHERE kind = 'reply' AND thread_id = ?1 AND resolved_at IS NULL",
                params![thread_id, now_ms()],
            )
        })
    }

    pub fn delete_message(&self, id: &str) -> Result<bool> {
        self.with(|c| Ok(c.execute("DELETE FROM messages WHERE id = ?1", [id])? > 0))
    }

    pub fn insert_message(
        &self,
        thread_id: &str,
        role: MessageRole,
        content: &str,
        status: MessageStatus,
        model: Option<&str>,
    ) -> Result<Message> {
        let msg = Message {
            id: new_id(),
            thread_id: thread_id.to_owned(),
            role,
            content: content.to_owned(),
            reasoning: String::new(),
            rounds: Vec::new(),
            status,
            error: None,
            model: model.map(str::to_owned),
            prompt_tokens: None,
            completion_tokens: None,
            context_tokens: None,
            created_at: now_ms(),
            attachments: Vec::new(),
        };
        self.with(|c| {
            c.execute(
                "INSERT INTO messages (id, thread_id, role, content, status, model, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![msg.id, msg.thread_id, role.as_str(), msg.content, status.as_str(), msg.model, msg.created_at],
            )
        })?;
        Ok(msg)
    }

    /// Full-text search over every message still stored (compacted ones
    /// included), best matches first. `agent_id` limits it to one agent.
    pub fn search_messages(&self, query: &str, agent_id: Option<&str>, limit: u32) -> Result<Vec<crate::SearchHit>> {
        // Each word is quoted (so FTS syntax in the query can't break it) and
        // matched as a prefix; any word may match, more matches rank higher.
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 2)
            .take(12)
            .map(|w| format!("\"{w}\"*"))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let fts = terms.join(" OR ");
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT m.id, m.thread_id, t.agent_id, COALESCE(a.name, ''), m.role, m.created_at,
                        snippet(messages_fts, 0, '«', '»', '…', 16)
                 FROM messages_fts
                 JOIN messages m ON m.rowid = messages_fts.rowid
                 JOIN threads t ON t.id = m.thread_id
                 LEFT JOIN agents a ON a.id = t.agent_id
                 WHERE messages_fts MATCH ?1 AND (?2 IS NULL OR t.agent_id = ?2)
                 ORDER BY bm25(messages_fts), m.created_at DESC
                 LIMIT ?3",
            )?;
            let rows = stmt
                .query_map(params![fts, agent_id, limit.clamp(1, 50)], |r| {
                    Ok(crate::SearchHit {
                        message_id: r.get(0)?,
                        thread_id: r.get(1)?,
                        agent_id: r.get(2)?,
                        agent_name: r.get(3)?,
                        role: r.get(4)?,
                        created_at: r.get(5)?,
                        snippet: r.get(6)?,
                    })
                })?
                .collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Writes the final state of a streamed reply.
    pub fn finish_message(&self, id: &str, r: &MessageResult) -> Result<Option<Message>> {
        self.with(|c| {
            c.execute(
                "UPDATE messages SET content = ?2, reasoning = ?3, rounds = ?4, status = ?5, error = ?6,
                    model = COALESCE(?7, model), prompt_tokens = ?8, completion_tokens = ?9, context_tokens = ?10
                 WHERE id = ?1",
                params![
                    id, r.content, r.reasoning, to_json(&r.rounds), r.status.as_str(), r.error,
                    r.model, r.prompt_tokens, r.completion_tokens, r.context_tokens
                ],
            )?;
            if r.prompt_tokens.is_some() || r.completion_tokens.is_some() {
                c.execute(
                    "INSERT OR REPLACE INTO usage (message_id, created_at, tokens)
                     SELECT id, created_at, ?2 FROM messages WHERE id = ?1",
                    params![id, r.prompt_tokens.unwrap_or(0) + r.completion_tokens.unwrap_or(0)],
                )?;
            }
            Ok(())
        })?;
        self.get_message(id)
    }

    // ---- routines -----------------------------------------------------------

    pub fn list_routines(&self) -> Result<Vec<crate::Routine>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!("SELECT {ROUTINE_COLS} FROM routines ORDER BY created_at"))?;
            let rows = stmt.query_map([], routine_from_row)?.collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn get_routine(&self, id: &str) -> Result<Option<crate::Routine>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {ROUTINE_COLS} FROM routines WHERE id = ?1"), [id], routine_from_row).optional()
        })
    }

    /// Saves a routine (insert or full update).
    pub fn save_routine(&self, r: &crate::Routine) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO routines (id, agent_id, name, schedule, kind, body, notify, enabled, next_run, last_run, last_result, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    r.id, r.agent_id, r.name, r.schedule, r.kind, r.body, r.notify, r.enabled, r.next_run,
                    r.last_run, r.last_result, r.created_at
                ],
            )
        })?;
        Ok(())
    }

    pub fn delete_routine(&self, id: &str) -> Result<bool> {
        self.with(|c| Ok(c.execute("DELETE FROM routines WHERE id = ?1", [id])? > 0))
    }

    /// Records the files attached to a (user) message.
    pub fn set_attachments(&self, id: &str, attachments: &[crate::Attachment]) -> Result<()> {
        self.with(|c| c.execute("UPDATE messages SET attachments = ?2 WHERE id = ?1", params![id, to_json(attachments)]))?;
        Ok(())
    }

    /// Saves a reply's progress (its rounds so far) while it is still being
    /// generated, so a reload shows the tool steps already taken.
    pub fn save_rounds(&self, id: &str, rounds: &[crate::Round]) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE messages SET rounds = ?2 WHERE id = ?1", params![id, to_json(&rounds)])
        })?;
        Ok(())
    }

    // ---- tool audit log ---------------------------------------------------

    pub fn log_tool(&self, e: &ToolLogEntry) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO tool_log (id, agent_id, thread_id, message_id, tool, arguments, status, summary, started_at, duration_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    e.id, e.agent_id, e.thread_id, e.message_id, e.tool, e.arguments,
                    e.status.as_str(), e.summary, e.started_at, e.duration_ms
                ],
            )
        })?;
        Ok(())
    }

    /// Most recent tool calls first.
    pub fn list_tool_log(&self, limit: u32) -> Result<Vec<ToolLogEntry>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, agent_id, thread_id, message_id, tool, arguments, status, summary, started_at, duration_ms
                 FROM tool_log ORDER BY started_at DESC LIMIT ?1",
            )?;
            let rows = stmt
                .query_map([limit], |r| {
                    Ok(ToolLogEntry {
                        id: r.get(0)?,
                        agent_id: r.get(1)?,
                        thread_id: r.get(2)?,
                        message_id: r.get(3)?,
                        tool: r.get(4)?,
                        arguments: r.get(5)?,
                        status: parse_text(r, 6)?,
                        summary: r.get(7)?,
                        started_at: r.get(8)?,
                        duration_ms: r.get(9)?,
                    })
                })?
                .collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    /// Replies that were still streaming when the engine stopped can never
    /// finish; mark them as failed. Returns how many were fixed up.
    pub fn fail_interrupted_messages(&self) -> Result<usize> {
        self.with(|c| {
            c.execute(
                "UPDATE messages SET status = 'error', error = 'Interrupted: the engine restarted'
                 WHERE status = 'streaming'",
                [],
            )
        })
    }

    // ---- devices ----------------------------------------------------------

    pub fn count_devices(&self) -> Result<i64> {
        self.with(|c| c.query_row("SELECT COUNT(*) FROM devices", [], |r| r.get(0)))
    }

    pub fn list_devices(&self) -> Result<Vec<Device>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!("SELECT {DEVICE_COLS} FROM devices ORDER BY created_at"))?;
            let rows = stmt.query_map([], device_from_row)?.collect::<Result<Vec<_>>>()?;
            Ok(rows)
        })
    }

    pub fn create_device(&self, name: &str, token_hash: &str) -> Result<Device> {
        let device = Device { id: new_id(), name: name.to_owned(), created_at: now_ms(), last_seen_at: None };
        self.with(|c| {
            c.execute(
                "INSERT INTO devices (id, name, token_hash, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![device.id, device.name, token_hash, device.created_at],
            )
        })?;
        Ok(device)
    }

    /// Looks up the device owning a token hash and records that it was seen.
    pub fn authenticate_device(&self, token_hash: &str) -> Result<Option<Device>> {
        self.with(|c| {
            let now = now_ms();
            let changed = c.execute(
                "UPDATE devices SET last_seen_at = ?2 WHERE token_hash = ?1",
                params![token_hash, now],
            )?;
            if changed == 0 {
                return Ok(None);
            }
            c.query_row(
                &format!("SELECT {DEVICE_COLS} FROM devices WHERE token_hash = ?1"),
                [token_hash],
                device_from_row,
            )
            .optional()
        })
    }

    pub fn delete_device(&self, id: &str) -> Result<bool> {
        self.with(|c| Ok(c.execute("DELETE FROM devices WHERE id = ?1", [id])? > 0))
    }
}

const AGENT_COLS: &str = "id, name, description, avatar, color, system_prompt, provider, model, \
                          created_at, updated_at, tools, role, created_by, reasoning_effort, auto_approve, auto_full";
const THREAD_COLS: &str = "id, agent_id, title, created_at, updated_at, summary, summary_until";
const MESSAGE_COLS: &str = "id, thread_id, role, content, reasoning, status, error, model, \
                            prompt_tokens, completion_tokens, created_at, rounds, context_tokens, attachments";
const ROUTINE_COLS: &str = "id, agent_id, name, schedule, kind, body, notify, enabled, next_run, last_run, last_result, created_at";
const DEVICE_COLS: &str = "id, name, created_at, last_seen_at";

fn to_json<T: serde::Serialize + ?Sized>(value: &T) -> String {
    serde_json::to_string(value).expect("serializable")
}

/// Reads a JSON text column; bad data degrades to the default instead of
/// making the whole row unreadable.
fn from_json<T: serde::de::DeserializeOwned + Default>(r: &Row, idx: usize) -> Result<T> {
    let text: String = r.get(idx)?;
    Ok(serde_json::from_str(&text).unwrap_or_default())
}

fn agent_from_row(r: &Row) -> Result<Agent> {
    Ok(Agent {
        id: r.get(0)?,
        name: r.get(1)?,
        description: r.get(2)?,
        avatar: r.get(3)?,
        color: r.get(4)?,
        system_prompt: r.get(5)?,
        provider: r.get(6)?,
        model: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
        tools: from_json(r, 10)?,
        role: r.get(11)?,
        created_by: r.get(12)?,
        reasoning_effort: r.get(13)?,
        auto_approve: r.get(14)?,
        auto_full: r.get(15)?,
    })
}

fn thread_from_row(r: &Row) -> Result<Thread> {
    Ok(Thread {
        id: r.get(0)?,
        agent_id: r.get(1)?,
        title: r.get(2)?,
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
        summary: r.get(5)?,
        summary_until: r.get(6)?,
    })
}

const PROBLEM_COLS: &str =
    "id, kind, agent_id, thread_id, message_id, routine_id, title, detail, created_at, resolved_at";

fn problem_from_row(r: &Row) -> Result<Problem> {
    Ok(Problem {
        id: r.get(0)?,
        kind: r.get(1)?,
        agent_id: r.get(2)?,
        thread_id: r.get(3)?,
        message_id: r.get(4)?,
        routine_id: r.get(5)?,
        title: r.get(6)?,
        detail: r.get(7)?,
        created_at: r.get(8)?,
        resolved_at: r.get(9)?,
    })
}

fn message_from_row(r: &Row) -> Result<Message> {
    Ok(Message {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        role: parse_text(r, 2)?,
        content: r.get(3)?,
        reasoning: r.get(4)?,
        status: parse_text(r, 5)?,
        error: r.get(6)?,
        model: r.get(7)?,
        prompt_tokens: r.get(8)?,
        completion_tokens: r.get(9)?,
        created_at: r.get(10)?,
        rounds: from_json(r, 11)?,
        context_tokens: r.get(12)?,
        attachments: from_json(r, 13)?,
    })
}

fn routine_from_row(r: &Row) -> Result<crate::Routine> {
    Ok(crate::Routine {
        id: r.get(0)?,
        agent_id: r.get(1)?,
        name: r.get(2)?,
        schedule: r.get(3)?,
        kind: r.get(4)?,
        body: r.get(5)?,
        notify: r.get(6)?,
        enabled: r.get(7)?,
        next_run: r.get(8)?,
        last_run: r.get(9)?,
        last_result: r.get(10)?,
        created_at: r.get(11)?,
    })
}

fn device_from_row(r: &Row) -> Result<Device> {
    Ok(Device { id: r.get(0)?, name: r.get(1)?, created_at: r.get(2)?, last_seen_at: r.get(3)? })
}

fn parse_text<T: std::str::FromStr<Err = String>>(r: &Row, idx: usize) -> Result<T> {
    let s: String = r.get(idx)?;
    s.parse().map_err(|e: String| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, e.into())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(store: &Store) -> Agent {
        store
            .create_agent(NewAgent {
                name: "Iris".into(),
                description: String::new(),
                avatar: String::new(),
                color: String::new(),
                system_prompt: "Be helpful.".into(),
                provider: "deepseek".into(),
                model: "deepseek-chat".into(),
                reasoning_effort: String::new(),
                tools: vec!["web_search".into()],
                role: String::new(),
                created_by: String::new(),
            })
            .unwrap()
    }

    #[test]
    fn main_agent_and_token_usage() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = (agent(&s), agent(&s));
        s.set_main_agent(&b.id).unwrap();
        assert_eq!(s.get_agent(&b.id).unwrap().unwrap().role, "main");
        assert_eq!(s.get_agent(&a.id).unwrap().unwrap().role, "");

        let t = s.create_thread(&a.id, "").unwrap();
        let m = s.insert_message(&t.id, MessageRole::Assistant, "", MessageStatus::Streaming, None).unwrap();
        s.finish_message(
            &m.id,
            &MessageResult {
                content: "x".into(),
                reasoning: String::new(),
                rounds: Vec::new(),
                status: MessageStatus::Complete,
                error: None,
                model: None,
                prompt_tokens: Some(100),
                completion_tokens: Some(20),
                context_tokens: Some(120),
            },
        )
        .unwrap();
        assert_eq!(s.tokens_used_since(0).unwrap(), 120);
        assert_eq!(s.tokens_used_since(now_ms() + 1000).unwrap(), 0);
        // Spent tokens still count after the conversation is gone.
        s.delete_agent(&a.id).unwrap();
        assert_eq!(s.tokens_used_since(0).unwrap(), 120);
    }

    #[test]
    fn full_text_search_follows_messages() {
        let s = Store::open_in_memory().unwrap();
        let a = agent(&s);
        let t = s.create_thread(&a.id, "").unwrap();
        s.insert_message(&t.id, MessageRole::User, "Which router did we pick for the cabin?", MessageStatus::Complete, None)
            .unwrap();
        let reply = s.insert_message(&t.id, MessageRole::Assistant, "", MessageStatus::Streaming, None).unwrap();
        // Replies are written when they finish.
        s.finish_message(
            &reply.id,
            &MessageResult {
                content: "We chose the TP-Link Deco X20 mesh; the café wifi was too weak.".into(),
                reasoning: String::new(),
                rounds: Vec::new(),
                status: MessageStatus::Complete,
                error: None,
                model: None,
                prompt_tokens: None,
                completion_tokens: None,
                context_tokens: None,
            },
        )
        .unwrap();

        let hits = s.search_messages("deco mesh", None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].message_id, reply.id);
        assert_eq!(hits[0].agent_name, "Iris");
        assert!(hits[0].snippet.contains("«Deco»"), "{}", hits[0].snippet);
        // Prefixes and accents: "rout" finds "router", "cafe" finds "café".
        assert_eq!(s.search_messages("rout", None, 10).unwrap().len(), 1);
        assert_eq!(s.search_messages("cafe", None, 10).unwrap().len(), 1);
        // FTS syntax in the query is just text.
        assert!(s.search_messages("\"deco\" OR NEAR(", None, 10).is_ok());
        assert!(s.search_messages("deco", Some("someone-else"), 10).unwrap().is_empty());
        // Cleared chats aren't searchable any more.
        s.clear_thread(&t.id).unwrap();
        assert!(s.search_messages("deco", None, 10).unwrap().is_empty());
    }

    #[test]
    fn rounds_and_tools_roundtrip() {
        use crate::{Round, ToolStatus, ToolStep};
        let s = Store::open_in_memory().unwrap();
        let a = agent(&s);
        assert_eq!(s.get_agent(&a.id).unwrap().unwrap().tools, vec!["web_search".to_string()]);
        let patched = s
            .update_agent(&a.id, AgentPatch { tools: Some(vec![]), ..Default::default() })
            .unwrap()
            .unwrap();
        assert!(patched.tools.is_empty());

        let t = s.create_thread(&a.id, "").unwrap();
        let m = s.insert_message(&t.id, MessageRole::Assistant, "", MessageStatus::Streaming, None).unwrap();
        let rounds = vec![Round {
            reasoning: "need data".into(),
            content: String::new(),
            tools: vec![ToolStep {
                id: "c1".into(),
                name: "get_time".into(),
                arguments: "{}".into(),
                status: ToolStatus::Done,
                summary: "Checked the time".into(),
                output: "12:00".into(),
                approval_id: None,
                auto_approved: None,
                started_at: 1,
                finished_at: Some(2),
            }],
        }];
        s.save_rounds(&m.id, &rounds).unwrap();
        assert_eq!(s.get_message(&m.id).unwrap().unwrap().rounds, rounds);

        s.log_tool(&ToolLogEntry {
            id: "l1".into(),
            agent_id: a.id.clone(),
            thread_id: t.id.clone(),
            message_id: m.id.clone(),
            tool: "get_time".into(),
            arguments: "{}".into(),
            status: ToolStatus::Done,
            summary: "Checked the time".into(),
            started_at: 5,
            duration_ms: 3,
        })
        .unwrap();
        let log = s.list_tool_log(10).unwrap();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].status, ToolStatus::Done);
    }

    #[test]
    fn agent_crud() {
        let s = Store::open_in_memory().unwrap();
        let a = agent(&s);
        assert_eq!(s.list_agents().unwrap(), vec![a.clone()]);
        let patched = s
            .update_agent(&a.id, AgentPatch { name: Some("Chief".into()), ..Default::default() })
            .unwrap()
            .unwrap();
        assert_eq!(patched.name, "Chief");
        assert_eq!(patched.model, "deepseek-chat");
        assert!(s.delete_agent(&a.id).unwrap());
        assert!(s.get_agent(&a.id).unwrap().is_none());
    }

    #[test]
    fn messages_keep_order_and_finish() {
        let s = Store::open_in_memory().unwrap();
        let a = agent(&s);
        let t = s.create_thread(&a.id, "").unwrap();
        let u = s.insert_message(&t.id, MessageRole::User, "hi", MessageStatus::Complete, None).unwrap();
        let m = s
            .insert_message(&t.id, MessageRole::Assistant, "", MessageStatus::Streaming, Some("deepseek-chat"))
            .unwrap();
        let done = s
            .finish_message(
                &m.id,
                &MessageResult {
                    content: "hello".into(),
                    reasoning: String::new(),
                    rounds: Vec::new(),
                    status: MessageStatus::Complete,
                    error: None,
                    model: None,
                    prompt_tokens: Some(3),
                    completion_tokens: Some(1),
                    context_tokens: Some(4),
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(done.content, "hello");
        assert_eq!(done.model.as_deref(), Some("deepseek-chat"), "model kept when result has none");
        let ids: Vec<_> = s.list_messages(&t.id).unwrap().into_iter().map(|m| m.id).collect();
        assert_eq!(ids, vec![u.id.clone(), m.id.clone()]);
        assert_eq!(s.last_message(&t.id).unwrap().unwrap().id, m.id);
        assert!(s.delete_message(&m.id).unwrap());
        assert_eq!(s.last_message(&t.id).unwrap().unwrap().id, u.id);
    }

    #[test]
    fn recent_threads_across_agents() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = (agent(&s), agent(&s));
        let t1 = s.create_thread(&a.id, "one").unwrap();
        let t2 = s.create_thread(&b.id, "two").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        s.touch_thread(&t1.id).unwrap();
        let ids: Vec<_> = s.list_recent_threads(10).unwrap().into_iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![t1.id, t2.id]);
    }

    #[test]
    fn problems_open_and_resolve() {
        let s = Store::open_in_memory().unwrap();
        let p = |id: &str, thread: &str| Problem {
            id: id.into(),
            kind: "reply".into(),
            agent_id: "a".into(),
            thread_id: thread.into(),
            message_id: String::new(),
            routine_id: String::new(),
            title: "failed".into(),
            detail: String::new(),
            created_at: now_ms(),
            resolved_at: None,
        };
        s.add_problem(&p("p1", "t1")).unwrap();
        s.add_problem(&p("p2", "t2")).unwrap();
        assert_eq!(s.open_problems().unwrap().len(), 2);
        assert_eq!(s.resolve_thread_problems("t1").unwrap(), 1);
        assert_eq!(s.resolve_problems(Some("p2")).unwrap(), 1);
        assert!(s.open_problems().unwrap().is_empty());
        assert!(s.get_problem("p1").unwrap().unwrap().resolved_at.is_some());
    }

    #[test]
    fn previews_show_each_agents_last_line() {
        let s = Store::open_in_memory().unwrap();
        let a = agent(&s);
        let t = s.create_thread(&a.id, "chat").unwrap();
        s.insert_message(&t.id, MessageRole::User, "hello there", MessageStatus::Complete, None).unwrap();
        s.insert_message(&t.id, MessageRole::User, "[I.R.I.S. update] Helper has finished", MessageStatus::Complete, None).unwrap();
        let p = s.chat_previews().unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].agent_id.as_str(), p[0].text.as_str()), (a.id.as_str(), "hello there"));
    }

    #[test]
    fn one_chat_per_agent() {
        let s = Store::open_in_memory().unwrap();
        let (a, b) = (agent(&s), agent(&s));
        let old = s.create_thread(&a.id, "old").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let newer = s.create_thread(&a.id, "newer").unwrap();
        let only = s.create_thread(&b.id, "b").unwrap();
        assert_eq!(s.keep_one_thread_per_agent().unwrap(), 1);
        assert!(s.get_thread(&old.id).unwrap().is_none());
        assert_eq!(s.chat_thread(&a.id, "a").unwrap().id, newer.id);
        assert_eq!(s.chat_thread(&b.id, "b").unwrap().id, only.id);

        s.insert_message(&newer.id, MessageRole::User, "hi", MessageStatus::Complete, None).unwrap();
        s.set_summary(&newer.id, "earlier stuff", 123).unwrap();
        let cleared = s.clear_thread(&newer.id).unwrap().unwrap();
        assert!(s.list_messages(&newer.id).unwrap().is_empty());
        assert_eq!((cleared.summary.as_str(), cleared.summary_until), ("", 0));
    }

    #[test]
    fn deleting_agent_cascades() {
        let s = Store::open_in_memory().unwrap();
        let a = agent(&s);
        let t = s.create_thread(&a.id, "x").unwrap();
        s.insert_message(&t.id, MessageRole::User, "hi", MessageStatus::Complete, None).unwrap();
        s.delete_agent(&a.id).unwrap();
        assert!(s.get_thread(&t.id).unwrap().is_none());
        assert!(s.list_messages(&t.id).unwrap().is_empty());
    }

    #[test]
    fn interrupted_streams_are_failed() {
        let s = Store::open_in_memory().unwrap();
        let a = agent(&s);
        let t = s.create_thread(&a.id, "").unwrap();
        let m = s.insert_message(&t.id, MessageRole::Assistant, "", MessageStatus::Streaming, None).unwrap();
        assert_eq!(s.fail_interrupted_messages().unwrap(), 1);
        assert_eq!(s.get_message(&m.id).unwrap().unwrap().status, MessageStatus::Error);
    }

    #[test]
    fn device_auth() {
        let s = Store::open_in_memory().unwrap();
        let d = s.create_device("server", "hash-1").unwrap();
        let seen = s.authenticate_device("hash-1").unwrap().unwrap();
        assert_eq!(seen.id, d.id);
        assert!(seen.last_seen_at.is_some());
        assert!(s.authenticate_device("nope").unwrap().is_none());
    }
}
