//! Keeping one endless chat within the model's reach: when a conversation
//! gets close to the context limit, older messages are summarized (like
//! Claude Code's compaction) and the summary is sent instead of them. The
//! messages themselves stay in the database and in the app.

use iris_core::{Agent, Message, MessageRole, MessageStatus};
use iris_providers::{ChatMessage, ChatOptions, OpenAiCompatible, Reasoning, Role};
use tracing::info;

use crate::{api::AppState, config, events::Event, tools};

/// Messages kept word for word after a compaction.
const KEEP_RECENT: usize = 6;
/// The choices for when chats compact (Settings → Models). Every step an agent
/// takes re-sends the whole chat, so a bigger context costs more per step and is
/// slower; Iris, who coordinates and is talked to most, keeps more by default.
pub const STEPS: &[u64] = &[32_000, 64_000, 128_000, 256_000, 512_000, 1_000_000];
pub const DEFAULT_OTHER: u64 = 64_000;
pub const DEFAULT_MAIN: u64 = 256_000;
/// For models whose context window isn't known.
const UNKNOWN_WINDOW: u64 = 32_000;
/// How much transcript one compaction call reads at most (characters).
const MAX_TRANSCRIPT_CHARS: usize = 240_000;

/// When a conversation using `model` should be compacted (tokens): the chosen
/// point, but always with room left in the model's window for the reply and its
/// steps (15% of a big window, 40% of a small one).
pub fn threshold(model: &str, override_window: Option<u64>, chosen: u64) -> u64 {
    let window = config::known_context_window(model).or(override_window).unwrap_or(UNKNOWN_WINDOW);
    let room = if window >= 256_000 { window * 85 / 100 } else { window * 6 / 10 };
    chosen.min(room)
}

/// The compaction point chosen for this agent (Iris has her own).
pub fn chosen_for(st: &AppState, agent: &Agent) -> u64 {
    let d = st.defaults.read().unwrap();
    if agent.role == "main" { d.compact_at_main.unwrap_or(DEFAULT_MAIN) } else { d.compact_at.unwrap_or(DEFAULT_OTHER) }
}

/// How full the conversation was at its latest reply.
fn last_context(messages: &[Message]) -> i64 {
    messages.iter().rev().find_map(|m| m.context_tokens).unwrap_or(0)
}

/// Compacts the agent's chat if it has grown past the threshold (or always,
/// with `force`). Returns whether it did.
pub async fn compact(
    st: &AppState,
    agent: &Agent,
    thread_id: &str,
    client: &OpenAiCompatible,
    model: &str,
    force: bool,
) -> Result<bool, String> {
    let thread = st.store.get_thread(thread_id).map_err(|e| e.to_string())?.ok_or("the chat is gone")?;
    let all = st.store.list_messages(thread_id).map_err(|e| e.to_string())?;
    if !force {
        let limit = threshold(model, st.defaults.read().unwrap().context_window, chosen_for(st, agent));
        if (last_context(&all) as u64) < limit {
            return Ok(false);
        }
    }
    let live: Vec<&Message> = all
        .iter()
        .filter(|m| m.created_at > thread.summary_until && m.status != MessageStatus::Streaming)
        .collect();
    if live.len() <= KEEP_RECENT {
        return Ok(false);
    }
    // What stays verbatim starts at a user message, so no reply loses its question.
    let mut cut = live.len() - KEEP_RECENT;
    while cut > 0 && live[cut].role != MessageRole::User {
        cut -= 1;
    }
    if cut == 0 {
        return Ok(false);
    }
    let old = &live[..cut];

    let mut transcript = String::new();
    for m in old {
        let who = match m.role {
            MessageRole::User => "User".to_owned(),
            MessageRole::Assistant => agent.name.clone(),
            MessageRole::System => continue,
        };
        let steps: Vec<String> = m.rounds.iter().flat_map(|r| r.tools.iter().map(|t| format!("  [{}]", t.summary))).collect();
        transcript.push_str(&format!("{who}: {}\n", tools::truncate(m.content.trim(), 3000)));
        if !steps.is_empty() {
            transcript.push_str(&steps.join("\n"));
            transcript.push('\n');
        }
        transcript.push('\n');
    }
    if transcript.chars().count() > MAX_TRANSCRIPT_CHARS {
        let skip = transcript.chars().count() - MAX_TRANSCRIPT_CHARS;
        transcript = format!("[… earliest part omitted]\n{}", transcript.chars().skip(skip).collect::<String>());
    }
    let previous = if thread.summary.trim().is_empty() {
        String::new()
    } else {
        format!("Summary of the conversation before these messages:\n{}\n\n", thread.summary.trim())
    };
    let prompt = format!(
        "You are compacting a long conversation between the user and {name}, an AI agent, so it can continue \
         within its context limit. Write one updated summary that replaces everything below.\n\n\
         Keep: what the user wants and their preferences; decisions made; important facts, names, numbers, \
         links, file names and commands; work done and its results; anything still open or promised. Drop \
         small talk and repetition. Use short bullet points under a few headings, at most about 700 words. \
         Write it so {name} can carry on as if it remembered everything.\n\n{previous}Messages to summarize:\n\n{transcript}",
        name = agent.name,
    );
    st.set_activity(thread_id, "compacting", "");
    let progress = |active: bool, summarized: usize| {
        st.emit(Event::Compacting { agent_id: agent.id.clone(), thread_id: thread_id.to_owned(), active, summarized })
    };
    progress(true, 0);
    let messages = [ChatMessage::new(Role::User, prompt)];
    let opts = ChatOptions { max_tokens: Some(2500), reasoning: Reasoning::Off, ..Default::default() };
    let done = match client.stream_chat(model, &messages, &opts, |_| {}).await {
        Ok(done) => done,
        Err(e) => {
            progress(false, 0);
            return Err(e.to_string());
        }
    };
    let tokens = done.usage.as_ref().map_or(0, |u| i64::from(u.prompt_tokens) + i64::from(u.completion_tokens));
    if let Some(u) = &done.usage {
        let cached = u.prompt_cache_hit_tokens.map_or(-1, i64::from);
        let _ = st.store.add_usage_split(i64::from(u.prompt_tokens), cached, i64::from(u.completion_tokens), model);
    }
    let summary = done.content.trim();
    if summary.is_empty() {
        progress(false, 0);
        return Err("the model returned an empty summary".into());
    }
    let until = old.last().map(|m| m.created_at).unwrap_or(thread.summary_until);
    if let Ok(Some(thread)) = st.store.set_summary(thread_id, summary, until) {
        st.emit(Event::ThreadUpdated { thread });
    }
    progress(false, old.len());
    info!("compacted {}'s chat: {} messages into a summary ({tokens} tokens)", agent.name, old.len());
    Ok(true)
}

/// The client and model an agent uses, for compacting outside a reply.
pub fn client_for(st: &AppState, agent: &Agent) -> Option<(OpenAiCompatible, String)> {
    let (provider, model) = st.resolve_model(agent)?;
    let client = st.providers.read().unwrap().get(&provider).map(|p| p.client.clone())?;
    Some((client, model))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds() {
        // DeepSeek Flash has a 1M window: the chosen point applies, up to 85% of it.
        assert_eq!(threshold("deepseek-flash", None, DEFAULT_OTHER), 64_000);
        assert_eq!(threshold("deepseek-flash", None, DEFAULT_MAIN), 256_000);
        assert_eq!(threshold("deepseek-flash", None, 1_000_000), 850_000);
        // Small or unknown windows keep 40% free.
        assert_eq!(threshold("some-local-model", None, DEFAULT_OTHER), UNKNOWN_WINDOW * 6 / 10);
        assert_eq!(threshold("some-local-model", Some(8_000), DEFAULT_MAIN), 4_800);
        assert!(STEPS.contains(&DEFAULT_OTHER) && STEPS.contains(&DEFAULT_MAIN));
    }

    // ---- end to end, against a fake model server ----

    use std::sync::{Arc, Mutex, RwLock, atomic::AtomicBool};

    use iris_core::{MessageResult, NewAgent, Store};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn state() -> AppState {
        let dir = std::env::temp_dir().join(format!("iris-compact-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        AppState {
            store: Arc::new(Store::open_in_memory().unwrap()),
            providers: Arc::new(RwLock::new(crate::providers::Providers::from_config(&[]))),
            data_dir: Arc::new(dir),
            events: tokio::sync::broadcast::channel(64).0,
            runs: Arc::default(),
            approvals: Arc::new(Mutex::new(Default::default())),
            followups: Arc::default(),
            http: reqwest::Client::new(),
            search: Arc::default(),
            defaults: Arc::default(),
            limits: Arc::default(),
            style: Arc::default(),
            local_device_id: Arc::new(None),
            started: std::time::Instant::now(),
            browser: Arc::default(),
            online: Arc::new(AtomicBool::new(true)),
            link: Arc::new(crate::link::Link::new(
                crate::link::Identity::from_secret(p256::SecretKey::random(&mut rand_core::OsRng)),
                crate::link::LinkConfig::default(),
            )),
            queues: Arc::default(),
            mcp: Arc::default(),
        }
    }

    /// A chat of `turns` question/answer pairs; the last answer says how full the context was.
    fn chat(st: &AppState, turns: usize, context: i64) -> (Agent, String) {
        let agent = st.store.create_agent(serde_json::from_value::<NewAgent>(serde_json::json!({ "name": "Tester" })).unwrap()).unwrap();
        let thread = st.store.create_thread(&agent.id, "chat").unwrap();
        for i in 0..turns {
            st.store.insert_message(&thread.id, MessageRole::User, &format!("question {i}"), MessageStatus::Complete, None).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
            let m = st.store.insert_message(&thread.id, MessageRole::Assistant, "", MessageStatus::Streaming, None).unwrap();
            st.store
                .finish_message(&m.id, &MessageResult {
                    content: format!("answer {i}"),
                    reasoning: String::new(),
                    rounds: vec![],
                    status: MessageStatus::Complete,
                    error: None,
                    model: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    context_tokens: (i + 1 == turns).then_some(context),
                })
                .unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        (agent, thread.id)
    }

    /// Answers one chat request with a streamed summary; returns what was asked.
    async fn fake_model() -> (OpenAiCompatible, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut req = Vec::new();
            let mut buf = [0u8; 8192];
            loop {
                let n = sock.read(&mut buf).await.unwrap();
                req.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&req).to_string();
                if let Some(head_end) = text.find("\r\n\r\n") {
                    let len = text[..head_end]
                        .lines()
                        .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    if req.len() >= head_end + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"- The user asked ten questions.\"}}]}\n\n\
                        data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":120,\"completion_tokens\":8}}\n\n\
                        data: [DONE]\n\n";
            let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{body}");
            sock.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&req).into_owned()
        });
        (OpenAiCompatible::new(format!("http://127.0.0.1:{port}/v1"), Some("k".to_owned())), server)
    }

    #[tokio::test]
    async fn compacting_on_demand_summarizes_the_older_messages() {
        let st = state();
        let (agent, thread_id) = chat(&st, 10, 1_000);
        let (client, server) = fake_model().await;
        let mut events = st.events.subscribe();
        assert!(compact(&st, &agent, &thread_id, &client, "deepseek-flash", true).await.unwrap());

        let asked = server.await.unwrap();
        assert!(asked.contains("question 0") && asked.contains("answer 0"), "the oldest messages are summarized");
        assert!(!asked.contains("question 9"), "the latest messages stay word for word");
        let thread = st.store.get_thread(&thread_id).unwrap().unwrap();
        assert_eq!(thread.summary, "- The user asked ten questions.");
        let all = st.store.list_messages(&thread_id).unwrap();
        assert_eq!(all.len(), 20, "the messages themselves stay");
        let kept = all.iter().filter(|m| m.created_at > thread.summary_until).count();
        assert!((KEEP_RECENT..=KEEP_RECENT + 1).contains(&kept), "kept {kept}");
        assert_eq!(all.iter().find(|m| m.created_at > thread.summary_until).unwrap().role, MessageRole::User);
        // The app hears about it: "compacting" while it works, then the updated chat.
        // (No reply is running here, as with the menu item or /compact: it's still announced.)
        let mut seen = vec![];
        while let Ok(e) = events.try_recv() {
            match e {
                Event::Compacting { thread_id: t, active, summarized, .. } if t == thread_id => seen.push(format!("compacting {active} {summarized}")),
                Event::ThreadUpdated { thread } if thread.id == thread_id => seen.push("updated".into()),
                _ => {}
            }
        }
        assert_eq!(seen.first().map(String::as_str), Some("compacting true 0"), "{seen:?}");
        assert!(seen.contains(&"updated".to_string()), "{seen:?}");
        assert!(seen.last().unwrap().starts_with("compacting false ") && !seen.last().unwrap().ends_with(" 0"), "{seen:?}");
    }

    #[tokio::test]
    async fn compacts_by_itself_at_the_ceiling_and_not_before() {
        let st = state();
        let ceiling = threshold("deepseek-flash", None, DEFAULT_OTHER) as i64;
        let (agent, quiet) = chat(&st, 10, ceiling - 1);
        let unused = OpenAiCompatible::new("http://127.0.0.1:9/v1", None);
        assert!(!compact(&st, &agent, &quiet, &unused, "deepseek-flash", false).await.unwrap(), "below the ceiling: nothing to do");

        let (agent, full) = chat(&st, 10, ceiling);
        let (client, server) = fake_model().await;
        assert!(compact(&st, &agent, &full, &client, "deepseek-flash", false).await.unwrap(), "at the ceiling it compacts");
        server.await.unwrap();
        assert!(!st.store.get_thread(&full).unwrap().unwrap().summary.is_empty());
    }
}
