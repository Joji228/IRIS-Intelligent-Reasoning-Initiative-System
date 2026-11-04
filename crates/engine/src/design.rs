//! Designing an agent from one sentence: the model picks a short name, a
//! one-line description, the tools it needs and writes its instructions, so
//! nobody has to fill in a form.

use iris_core::NewAgent;
use iris_providers::{ChatMessage, ChatOptions, Reasoning, Role};
use serde_json::Value;

use crate::{api::AppState, compact};

/// Tools a designed agent may get, or ask Iris for later (`team`, `models`
/// and `settings` stay with Iris).
pub const DESIGNABLE_TOOLS: &[&str] = &["files", "shell", "system"];

/// Asks the default model to design an agent for `request`. The result is
/// not saved; the caller creates it.
pub async fn design(st: &AppState, request: &str) -> Result<NewAgent, String> {
    let main = st.store.list_agents().map_err(|e| e.to_string())?.into_iter().find(|a| a.role == "main");
    let (client, model) = main
        .as_ref()
        .and_then(|a| compact::client_for(st, a))
        .ok_or("no model is set up yet; add a provider in Settings")?;
    let taken: Vec<String> = st.store.list_agents().map_err(|e| e.to_string())?.into_iter().map(|a| a.name).collect();
    let prompt = format!(
        "Design an AI agent for this request from the user of I.R.I.S., a personal multi-agent app running on \
         their home server (a modest Windows 10 PC).\n\nRequest: {request}\n\n\
         Every agent already has: web search, reading web pages, a real browser, date/time, routines, the \
         team (asking other agents and Iris), models and settings. Extra tools it may need: files (read/write \
         files in its own folder), shell (run PowerShell on the server; commands follow its auto mode), system \
         (this computer's health, app updates/installs/removal, Windows Update, restart). List only the extra \
         tools it really needs (often none).\n\
         Names already taken: {taken}.\n\n\
         Reply with JSON only:\n\
         {{\"name\": \"1-2 words, title case, e.g. Site Watcher\",\n\
          \"description\": \"one short sentence: what it does\",\n\
          \"tools\": [\"...\"],\n\
          \"instructions\": \"its system prompt, 60-120 words, second person: role, how to work, what to report, \
         and to keep answers short and to the point\"}}",
        taken = if taken.is_empty() { "none".into() } else { taken.join(", ") },
    );
    let opts = ChatOptions { temperature: Some(0.4), max_tokens: Some(700), reasoning: Reasoning::Off, ..Default::default() };
    let done = client
        .stream_chat(&model, &[ChatMessage::new(Role::User, prompt)], &opts, |_| {})
        .await
        .map_err(|e| e.to_string())?;
    let tokens = done.usage.as_ref().map_or(0, |u| i64::from(u.prompt_tokens) + i64::from(u.completion_tokens));
    let _ = st.store.add_usage(tokens);
    parse(&done.content, &taken).ok_or_else(|| "the model didn't return a usable design; try again".into())
}

/// Role of an orb summoned with "+" that hasn't been told its job yet.
pub const BLANK_ROLE: &str = "new";
/// Marks the note that says how a summoned orb was set up.
pub const SETUP_PREFIX: &str = "[I.R.I.S. setup]";
/// Toolsets Iris only grants when the specialty really needs them.
const POWERFUL: &[&str] = &["shell", "system", "browser", "files"];

/// Iris, as Chief of Staff, reviews the tools a new specialist asks for and
/// grants only what its job needs. Returns (granted, declined, her note).
pub async fn audit(st: &AppState, spec: &NewAgent) -> (Vec<String>, Vec<String>, String) {
    let requested = spec.tools.clone();
    if requested.is_empty() {
        return (requested, Vec::new(), String::new());
    }
    let prompt = format!(
        "You are Iris, Chief of Staff of the user's I.R.I.S. agent team on their home server. A new specialist \
         is joining.\nName: {}\nJob: {}\nIts instructions: {}\nIt asks for these tools: {}.\n\
         Grant only the tools this job really needs. shell (PowerShell), system (updates, installs, restart), \
         browser (a real browser) and files are powerful: grant them only if the job can't be done without them.\n\
         Reply with JSON only: {{\"granted\": [\"...\"], \"note\": \"one short sentence for the user: what you \
         granted and why, and what you declined\"}}",
        spec.name,
        spec.description,
        spec.system_prompt,
        requested.join(", "),
    );
    ask_iris(st, &prompt, &requested).await
}

/// An agent asks Iris for more tools while working (the `request_tools`
/// tool). The user asked for the work, so ordinary tools are granted readily;
/// powerful ones need a real reason. Returns (granted, declined, her note).
pub async fn review_request(
    st: &AppState,
    agent: &iris_core::Agent,
    requested: &[String],
    reason: &str,
) -> (Vec<String>, Vec<String>, String) {
    let prompt = format!(
        "You are Iris, Chief of Staff of the user's I.R.I.S. agent team on their home server. One of your \
         specialists asks for more tools.\nAgent: {} ({})\nIt has: {}\nIt asks for: {}\nWhy: {}\n\
         The user asked for this work, so grant tools that fit it. web_search, web_fetch, get_time, files and \
         routines are ordinary. shell (PowerShell; each command still follows its auto mode), system (updates, \
         installs, restart) and browser (a real browser) are powerful: grant them when the reason really needs \
         them, not for vague or unrelated wishes.\n\
         Reply with JSON only: {{\"granted\": [\"...\"], \"note\": \"one short sentence for the agent: what you \
         granted and why, and what you declined\"}}",
        agent.name,
        agent.description,
        if agent.tools.is_empty() { "no tools".to_owned() } else { agent.tools.join(", ") },
        requested.join(", "),
        reason,
    );
    ask_iris(st, &prompt, requested).await
}

/// Asks the main agent's model for a tool review; without a model (or a
/// readable answer) only the safe tools are granted.
async fn ask_iris(st: &AppState, prompt: &str, requested: &[String]) -> (Vec<String>, Vec<String>, String) {
    let safe_only = |why: &str| {
        let (ok, no): (Vec<_>, Vec<_>) = requested.iter().cloned().partition(|t| !POWERFUL.contains(&t.as_str()));
        (ok, no, why.to_owned())
    };
    let main = st.store.list_agents().ok().and_then(|a| a.into_iter().find(|a| a.role == "main"));
    let Some((client, model)) = main.as_ref().and_then(|a| compact::client_for(st, a)) else {
        return safe_only("kept the safe tools only");
    };
    let opts = ChatOptions { temperature: Some(0.2), max_tokens: Some(300), reasoning: Reasoning::Off, ..Default::default() };
    let reply = client.stream_chat(&model, &[ChatMessage::new(Role::User, prompt.to_owned())], &opts, |_| {}).await;
    let Ok(done) = reply else {
        return safe_only("kept the safe tools only (couldn't reach the model to review the rest)");
    };
    let tokens = done.usage.as_ref().map_or(0, |u| i64::from(u.prompt_tokens) + i64::from(u.completion_tokens));
    let _ = st.store.add_usage(tokens);
    parse_audit(&done.content, requested)
}

/// Reads Iris's review: granted tools must be among those requested.
fn parse_audit(text: &str, requested: &[String]) -> (Vec<String>, Vec<String>, String) {
    let parsed = text.find('{').zip(text.rfind('}')).and_then(|(a, b)| serde_json::from_str::<Value>(&text[a..=b]).ok());
    let Some(v) = parsed else {
        let (ok, no): (Vec<_>, Vec<_>) = requested.iter().cloned().partition(|t| !POWERFUL.contains(&t.as_str()));
        return (ok, no, "kept the safe tools only".into());
    };
    let wanted: Vec<&str> = v["granted"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).collect()).unwrap_or_default();
    let (granted, declined): (Vec<_>, Vec<_>) = requested.iter().cloned().partition(|t| wanted.contains(&t.as_str()));
    let note = v["note"].as_str().map(str::trim).unwrap_or_default().chars().take(240).collect();
    (granted, declined, note)
}

/// Turns a blank orb into a specialist from the user's first message: designs
/// it, has Iris review its tools, saves it and leaves a setup note in its
/// chat. Returns the agent as it is now.
pub async fn shape(st: &AppState, agent: &iris_core::Agent, thread_id: &str, request: &str) -> Result<iris_core::Agent, String> {
    let spec = design(st, request).await?;
    let (granted, declined, note) = audit(st, &spec).await;
    let patch = iris_core::AgentPatch {
        name: Some(spec.name.clone()),
        description: Some(spec.description.clone()),
        system_prompt: Some(spec.system_prompt.clone()),
        tools: Some(granted.clone()),
        ..Default::default()
    };
    let shaped = st.store.update_agent(&agent.id, patch).map_err(|e| e.to_string())?.ok_or("the agent is gone")?;
    st.store.set_agent_role(&agent.id, "").map_err(|e| e.to_string())?;
    let shaped = st.store.get_agent(&shaped.id).map_err(|e| e.to_string())?.unwrap_or(shaped);
    crate::memory::save_soul(&st.data_dir, &shaped);
    st.emit(crate::events::Event::AgentUpdated { agent: shaped.clone() });
    let tools = if granted.is_empty() { "no tools".to_owned() } else { granted.join(", ") };
    let mut text = format!("{SETUP_PREFIX} You are now {}: {} Iris gave you: {tools}.", shaped.name, shaped.description);
    if !declined.is_empty() {
        text.push_str(&format!(" Declined: {}.", declined.join(", ")));
    }
    if !note.is_empty() {
        text.push_str(&format!(" Iris: {note}"));
    }
    text.push_str(" Now do the user's request above.");
    if let Ok(m) = st.store.insert_message(thread_id, iris_core::MessageRole::User, &text, iris_core::MessageStatus::Complete, None) {
        st.emit(crate::events::Event::MessageCreated { message: m });
    }
    Ok(shaped)
}

/// Reads the model's JSON into a new agent, keeping only known tools and a
/// name that isn't taken.
fn parse(text: &str, taken: &[String]) -> Option<NewAgent> {
    let (a, b) = (text.find('{')?, text.rfind('}')?);
    let v: Value = serde_json::from_str(&text[a..=b]).ok()?;
    let s = |k: &str| v[k].as_str().map(str::trim).filter(|x| !x.is_empty()).map(str::to_owned);
    let mut name: String = s("name")?.chars().filter(|c| !c.is_control()).take(28).collect();
    let base = name.clone();
    for n in 2.. {
        if !taken.iter().any(|t| t.eq_ignore_ascii_case(&name)) {
            break;
        }
        name = format!("{base} {n}");
    }
    let mut tools: Vec<String> = v["tools"]
        .as_array()
        .map(|a| a.iter().filter_map(|t| t.as_str()).filter(|t| DESIGNABLE_TOOLS.contains(t)).map(str::to_owned).collect())
        .unwrap_or_default();
    tools.dedup();
    Some(NewAgent {
        name,
        description: s("description").unwrap_or_default(),
        avatar: String::new(),
        color: String::new(),
        system_prompt: s("instructions")?,
        provider: String::new(),
        model: String::new(),
        reasoning_effort: String::new(),
        tools,
        role: String::new(),
        created_by: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_design() {
        let text = "```json\n{\"name\": \"Site Watcher\", \"description\": \"Checks sites.\", \
                    \"tools\": [\"web_fetch\", \"team\", \"shell\"], \"instructions\": \"You watch sites.\"}\n```";
        let a = parse(text, &["Site Watcher".to_string()]).unwrap();
        assert_eq!(a.name, "Site Watcher 2", "a taken name gets a number");
        assert_eq!(a.tools, vec!["shell"], "built-in, unknown or reserved tools are dropped");
        assert_eq!(a.system_prompt, "You watch sites.");
        assert!(parse("no json here", &[]).is_none());
    }

    #[test]
    fn audit_grants_only_requested_tools() {
        let requested = vec!["web_fetch".to_string(), "shell".to_string(), "system".to_string()];
        let (granted, declined, note) =
            parse_audit(r#"{"granted": ["web_fetch", "system", "team"], "note": "No shell needed."}"#, &requested);
        assert_eq!(granted, vec!["web_fetch", "system"], "never more than it asked for");
        assert_eq!(declined, vec!["shell"]);
        assert_eq!(note, "No shell needed.");
        let (granted, declined, _) = parse_audit("garbled", &requested);
        assert_eq!(granted, vec!["web_fetch"], "without a review only the safe tools stay");
        assert_eq!(declined, vec!["shell", "system"]);
    }
}
