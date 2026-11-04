//! `search_chats`: full-text search over every agent's conversations,
//! including what compaction folded into a summary, so agents can look up
//! what was said or decided weeks ago.

use chrono::{Local, TimeZone};
use iris_providers::ToolCall;
use serde_json::Value;

use crate::{api::AppState, tools::ToolOutcome};

const DEFAULT_RESULTS: u32 = 8;

pub fn run(st: &AppState, call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
        .unwrap_or(Value::Null);
    let query = args["query"].as_str().unwrap_or_default().trim().to_owned();
    let summary = format!("Searched past chats for “{query}”");
    if query.is_empty() {
        return ToolOutcome::err(summary, "give some words to search for");
    }
    // Optional: only one agent's chat, by name.
    let agent_id = match args["agent"].as_str().map(str::trim).filter(|a| !a.is_empty()) {
        Some(name) => match st.store.list_agents().unwrap_or_default().into_iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
            Some(a) => Some(a.id),
            None => return ToolOutcome::err(summary, format!("no agent called {name:?}")),
        },
        None => None,
    };
    let limit = args["max_results"].as_u64().map_or(DEFAULT_RESULTS, |n| n.clamp(1, 20) as u32);
    let hits = match st.store.search_messages(&query, agent_id.as_deref(), limit) {
        Ok(h) => h,
        Err(e) => return ToolOutcome::err(summary, format!("search failed: {e}")),
    };
    if hits.is_empty() {
        return ToolOutcome::ok(format!("{summary} · nothing found"), "No messages match. Try other or fewer words.");
    }
    let lines: Vec<String> = hits
        .iter()
        .map(|h| {
            let when = Local.timestamp_millis_opt(h.created_at).single().map(|d| d.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_default();
            let who = if h.role == "user" { "user".to_owned() } else { h.agent_name.clone() };
            format!("- {when} · {}'s chat · {who}: {}", h.agent_name, h.snippet.replace('\n', " "))
        })
        .collect();
    ToolOutcome::ok(
        format!("{summary} · {} found", hits.len()),
        format!("Best matches first (matching words «marked»):\n{}", lines.join("\n")),
    )
}
