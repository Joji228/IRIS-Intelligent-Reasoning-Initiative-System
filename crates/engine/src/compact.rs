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
/// Never let a conversation grow past this many tokens before compacting,
/// even on 1M-token models: long contexts are slow and costly.
const MAX_BEFORE_COMPACT: u64 = 64_000;
/// For models whose context window isn't known.
const UNKNOWN_WINDOW: u64 = 32_000;
/// How much transcript one compaction call reads at most (characters).
const MAX_TRANSCRIPT_CHARS: usize = 240_000;

/// When a conversation using `model` should be compacted (tokens).
pub fn threshold(model: &str, override_window: Option<u64>) -> u64 {
    let window = config::known_context_window(model).or(override_window).unwrap_or(UNKNOWN_WINDOW);
    (window * 6 / 10).min(MAX_BEFORE_COMPACT)
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
        let limit = threshold(model, st.defaults.read().unwrap().context_window);
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
    let messages = [ChatMessage::new(Role::User, prompt)];
    let opts = ChatOptions { max_tokens: Some(2500), reasoning: Reasoning::Off, ..Default::default() };
    let done = client.stream_chat(model, &messages, &opts, |_| {}).await.map_err(|e| e.to_string())?;
    let tokens = done.usage.as_ref().map_or(0, |u| i64::from(u.prompt_tokens) + i64::from(u.completion_tokens));
    let _ = st.store.add_usage(tokens);
    let summary = done.content.trim();
    if summary.is_empty() {
        return Err("the model returned an empty summary".into());
    }
    let until = old.last().map(|m| m.created_at).unwrap_or(thread.summary_until);
    if let Ok(Some(thread)) = st.store.set_summary(thread_id, summary, until) {
        st.emit(Event::ThreadUpdated { thread });
    }
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
        assert_eq!(threshold("deepseek-flash", None), MAX_BEFORE_COMPACT);
        assert_eq!(threshold("some-local-model", None), UNKNOWN_WINDOW * 6 / 10);
        assert_eq!(threshold("some-local-model", Some(8_000)), 4_800);
    }
}
