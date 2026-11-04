//! An agent's markdown files, in `agents/<id>/` under the data dir:
//!
//! - `SOUL.md`: who the agent is and how it behaves (its instructions). Kept
//!   in step with the agent's `system_prompt`; edits to the file win.
//! - `MEMORY.md`: the agent's own long-term notes (facts, decisions, how the
//!   user likes things done), one `- entry` per line.
//!
//! Plus `memory/USER.md`, shared by every agent: what they know about the
//! user. Agents change their notes with the `memory` tool; both files go into
//! every system prompt, so they are kept small.

use std::{fs, path::PathBuf};

use iris_core::{Agent, AgentPatch, Store};
use iris_providers::ToolCall;
use serde_json::Value;
use tracing::warn;

use crate::{api::AppState, tools::ToolOutcome};

/// Size caps (characters), so memory never crowds out the conversation.
pub const MEMORY_LIMIT: usize = 3000;
pub const USER_LIMIT: usize = 2000;

pub fn agent_dir(data_dir: &std::path::Path, agent_id: &str) -> PathBuf {
    data_dir.join("agents").join(agent_id)
}

fn soul_path(data_dir: &std::path::Path, agent_id: &str) -> PathBuf {
    agent_dir(data_dir, agent_id).join("SOUL.md")
}

pub fn memory_path(data_dir: &std::path::Path, agent_id: &str) -> PathBuf {
    agent_dir(data_dir, agent_id).join("MEMORY.md")
}

pub fn user_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("memory").join("USER.md")
}

pub fn read(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap_or_default().trim().to_owned()
}

pub fn write(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, format!("{}\n", text.trim()))
}

/// Writes an agent's SOUL.md from its instructions.
pub fn save_soul(data_dir: &std::path::Path, agent: &Agent) {
    if let Err(e) = write(&soul_path(data_dir, &agent.id), &agent.system_prompt) {
        warn!("writing SOUL.md for {}: {e}", agent.name);
    }
}

/// On start: a SOUL.md edited by hand updates the agent; otherwise the file
/// is (re)written from the agent's instructions.
pub fn sync_souls(store: &Store, data_dir: &std::path::Path) -> anyhow::Result<()> {
    for agent in store.list_agents()? {
        let path = soul_path(data_dir, &agent.id);
        let file = read(&path);
        if path.exists() && file != agent.system_prompt.trim() {
            store.update_agent(&agent.id, AgentPatch { system_prompt: Some(file), ..Default::default() })?;
        } else if !path.exists() {
            save_soul(data_dir, &agent);
        }
    }
    Ok(())
}

/// Removes an agent's files along with the agent.
pub fn remove_agent_files(data_dir: &std::path::Path, agent_id: &str) {
    let _ = fs::remove_dir_all(agent_dir(data_dir, agent_id));
}

/// The memory part of an agent's system prompt.
pub fn prompt_section(data_dir: &std::path::Path, agent: &Agent) -> String {
    let memory = read(&memory_path(data_dir, &agent.id));
    let user = read(&user_path(data_dir));
    let mut out = String::from(
        "\n\nLong-term memory (persists across chats). Save what will matter later with the memory tool: \
         target \"user\" for facts about the user, \"memory\" for your own notes. Short entries; nothing temporary.",
    );
    out.push_str(&format!(
        "\n\nUSER.md (shared by all agents):\n{}",
        if user.is_empty() { "(empty)".into() } else { user }
    ));
    out.push_str(&format!(
        "\n\nMEMORY.md (yours):\n{}",
        if memory.is_empty() { "(empty)".into() } else { memory }
    ));
    out
}

/// The `memory` tool: add, replace or remove an entry in MEMORY.md or USER.md.
pub fn run(st: &AppState, agent: &Agent, call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
        .unwrap_or(Value::Null);
    let s = |k: &str| args[k].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    let target = s("target").unwrap_or_else(|| "memory".into());
    let (path, limit, label) = if target == "user" {
        (user_path(&st.data_dir), USER_LIMIT, "USER.md")
    } else {
        (memory_path(&st.data_dir, &agent.id), MEMORY_LIMIT, "MEMORY.md")
    };
    let mut entries: Vec<String> = read(&path)
        .lines()
        .map(|l| l.trim().trim_start_matches("- ").trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect();
    let err = |summary: &str, e: String| ToolOutcome { ok: false, output: format!("Error: {e}"), summary: summary.to_owned() };
    let find = |entries: &[String], needle: &str| {
        let n = needle.to_lowercase();
        entries.iter().position(|e| e.to_lowercase().contains(&n))
    };
    let (summary, done) = match s("action").as_deref().unwrap_or("add") {
        "add" => {
            let Some(text) = s("content") else { return err("Saving to memory failed", "`content` is required".into()) };
            let text = text.replace('\n', " ");
            if entries.iter().any(|e| e.eq_ignore_ascii_case(&text)) {
                return ToolOutcome { ok: true, output: "Already saved.".into(), summary: format!("Already in {label}") };
            }
            entries.push(text.clone());
            (format!("Saved to {label}: “{}”", crate::tools::truncate(&text, 60)), true)
        }
        "replace" => {
            let (Some(old), Some(text)) = (s("old"), s("content")) else {
                return err("Updating memory failed", "`old` (part of the entry) and `content` are required".into());
            };
            let Some(i) = find(&entries, &old) else { return err("Updating memory failed", format!("no entry contains “{old}”")) };
            entries[i] = text.replace('\n', " ");
            (format!("Updated {label}"), true)
        }
        "remove" => {
            let Some(old) = s("old") else { return err("Removing from memory failed", "`old` (part of the entry) is required".into()) };
            let Some(i) = find(&entries, &old) else { return err("Removing from memory failed", format!("no entry contains “{old}”")) };
            entries.remove(i);
            (format!("Removed from {label}"), true)
        }
        other => return err("Memory failed", format!("unknown action {other:?}; use add, replace or remove")),
    };
    let text: String = entries.iter().map(|e| format!("- {e}")).collect::<Vec<_>>().join("\n");
    if done && text.chars().count() > limit {
        return err(
            &format!("{label} is full"),
            format!(
                "{label} would be {} characters (limit {limit}). Remove or shorten entries that matter less first, \
                 then try again.",
                text.chars().count()
            ),
        );
    }
    if let Err(e) = write(&path, &text) {
        return err("Saving memory failed", e.to_string());
    }
    ToolOutcome {
        ok: true,
        output: format!("{label} now holds {} of {limit} characters.", text.chars().count()),
        summary,
    }
}
