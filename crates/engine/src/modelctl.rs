//! Model tools: let an agent tell which providers and models are set up and
//! switch the model (for everyone, for one agent, or the fallback) when the
//! user asks for it in chat.

use std::time::Duration;

use iris_core::{Agent, AgentPatch};
use iris_providers::{OpenAiCompatible, ToolCall};
use serde_json::Value;

use crate::{
    api::AppState,
    events::Event,
    providers::KeySource,
    team,
    tools::ToolOutcome,
};

/// How long to wait for a provider's model list.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);
/// Model ids shown per provider at most (OpenRouter lists hundreds).
const MAX_LISTED: usize = 60;

pub async fn run(st: &AppState, me: &Agent, call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
        .unwrap_or(Value::Null);
    let s = |k: &str| args[k].as_str().map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    match call.name.as_str() {
        "list_models" => list(st, s("provider")).await,
        "switch_model" => match s("model") {
            Some(model) => switch(st, me, &model, s("provider"), s("target").unwrap_or_else(|| "all".into())).await,
            None => err("Switching the model failed", "`model` is required"),
        },
        other => err(format!("Unknown model tool {other}"), "no such tool"),
    }
}

fn err(summary: impl Into<String>, e: impl Into<String>) -> ToolOutcome {
    ToolOutcome { ok: false, output: format!("Error: {}", e.into()), summary: summary.into() }
}

fn ok(summary: impl Into<String>, output: impl Into<String>) -> ToolOutcome {
    ToolOutcome { ok: true, output: output.into(), summary: summary.into() }
}

/// A provider as the tools see it.
#[derive(Clone)]
struct Prov {
    id: String,
    name: String,
    usable: bool,
    client: OpenAiCompatible,
}

fn providers(st: &AppState) -> Vec<Prov> {
    st.providers
        .read()
        .unwrap()
        .all()
        .iter()
        .map(|p| Prov {
            id: p.id.clone(),
            name: p.name.clone(),
            usable: p.key_source != KeySource::None
                || p.base_url.contains("://localhost")
                || p.base_url.contains("://127.0.0.1"),
            client: p.client.clone(),
        })
        .collect()
}

async fn models_of(p: &Prov) -> Result<Vec<String>, String> {
    if !p.usable {
        return Err("no API key".into());
    }
    match tokio::time::timeout(LIST_TIMEOUT, p.client.list_models()).await {
        Ok(Ok(models)) => Ok(models),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("timed out".into()),
    }
}

fn label(st: &AppState, (provider, model): &(String, String)) -> String {
    format!("{model} ({})", st.provider_name(provider))
}

async fn list(st: &AppState, only: Option<String>) -> ToolOutcome {
    let all = providers(st);
    if all.is_empty() {
        return ok("No providers set up", "No AI providers are set up. The user can add one in Settings → Providers.");
    }
    let mut out = String::new();
    if let Some(d) = st.default_model() {
        out.push_str(&format!("Default model (every agent without its own): {}\n", label(st, &d)));
    }
    match st.fallback_model() {
        Some(f) => out.push_str(&format!("Fallback (used when the main provider fails): {}\n", label(st, &f))),
        None => out.push_str("Fallback: off\n"),
    }
    let own: Vec<String> = st
        .store
        .list_agents()
        .unwrap_or_default()
        .into_iter()
        .filter(|a| !a.provider.is_empty() && !a.model.is_empty())
        .map(|a| format!("{} → {}", a.name, label(st, &(a.provider.clone(), a.model.clone()))))
        .collect();
    out.push_str(&format!(
        "Agents with their own model: {}\n\nProviders:\n",
        if own.is_empty() { "none".to_owned() } else { own.join("; ") }
    ));

    let shown: Vec<&Prov> = all
        .iter()
        .filter(|p| only.as_ref().is_none_or(|o| p.id.eq_ignore_ascii_case(o) || p.name.eq_ignore_ascii_case(o)))
        .collect();
    // Ask all providers at once.
    let tasks: Vec<_> = shown
        .iter()
        .map(|p| {
            let p = (*p).clone();
            tokio::spawn(async move { models_of(&p).await })
        })
        .collect();
    for (p, task) in shown.iter().zip(tasks) {
        let models = match task.await.unwrap_or_else(|e| Err(e.to_string())) {
            Ok(mut m) => {
                m.sort();
                let more = m.len().saturating_sub(MAX_LISTED);
                let mut text = m.into_iter().take(MAX_LISTED).collect::<Vec<_>>().join(", ");
                if more > 0 {
                    text.push_str(&format!(" … and {more} more (ask with a narrower provider)"));
                }
                text
            }
            Err(e) => format!("(couldn't load the model list: {e})"),
        };
        out.push_str(&format!("- {} (id: {}): {}\n", p.name, p.id, models));
    }
    ok(format!("Checked the models · {} providers", all.len()), out)
}

/// Finds the provider offering `model`: the one named, else the current
/// default's, else any provider whose model list contains it.
async fn resolve_provider(st: &AppState, model: &str, provider: Option<String>) -> Result<String, String> {
    let all = providers(st);
    if let Some(wanted) = provider {
        let p = all
            .iter()
            .find(|p| p.id.eq_ignore_ascii_case(&wanted) || p.name.eq_ignore_ascii_case(&wanted))
            .ok_or_else(|| format!("there is no provider called {wanted}; use list_models to see them"))?;
        if !p.usable {
            return Err(format!("{} has no API key yet; the user can add it in Settings → Providers", p.name));
        }
        // Check the id when the provider can list its models.
        if let Ok(models) = models_of(p).await {
            if !models.iter().any(|m| m == model) {
                return Err(not_found(model, &p.name, &models));
            }
        }
        return Ok(p.id.clone());
    }
    let default = st.default_model().map(|(p, _)| p);
    let mut ordered: Vec<&Prov> = all.iter().filter(|p| p.usable).collect();
    ordered.sort_by_key(|p| Some(&p.id) != default.as_ref());
    let mut nearest: Vec<String> = Vec::new();
    let mut any_listed = false;
    for p in &ordered {
        if let Ok(models) = models_of(p).await {
            any_listed = true;
            if models.iter().any(|m| m == model) {
                return Ok(p.id.clone());
            }
            nearest.extend(close_matches(model, &models).into_iter().map(|m| format!("{m} ({})", p.name)));
        }
    }
    match (any_listed, ordered.first()) {
        // No provider could list its models: trust the user with the default one.
        (false, Some(p)) => Ok(p.id.clone()),
        (_, None) => Err("no provider with an API key is set up".into()),
        (true, Some(_)) => Err(format!(
            "no configured provider offers a model called {model}.{}",
            if nearest.is_empty() { String::new() } else { format!(" Close matches: {}", nearest.join(", ")) }
        )),
    }
}

fn close_matches(model: &str, models: &[String]) -> Vec<String> {
    let needle = model.to_lowercase();
    models
        .iter()
        .filter(|m| {
            let m = m.to_lowercase();
            m.contains(&needle) || needle.contains(&m) || needle.split(['-', '/', ' ', '.']).any(|w| w.len() > 2 && m.contains(w))
        })
        .take(8)
        .cloned()
        .collect()
}

fn not_found(model: &str, provider: &str, models: &[String]) -> String {
    let near = close_matches(model, models);
    format!(
        "{provider} has no model called {model}.{}",
        if near.is_empty() { format!(" It offers: {}", models.iter().take(20).cloned().collect::<Vec<_>>().join(", ")) } else { format!(" Did you mean: {}", near.join(", ")) }
    )
}

async fn switch(st: &AppState, me: &Agent, model: &str, provider: Option<String>, target: String) -> ToolOutcome {
    let target_l = target.to_lowercase();
    // Turning the fallback off needs no model.
    if matches!(target_l.as_str(), "fallback_off" | "no_fallback") {
        let (p, m) = st.fallback_model().unwrap_or_default();
        return match st.set_fallback(false, &p, &m) {
            Ok(()) => ok("Turned the fallback off", "The fallback model is now off."),
            Err(e) => err("Couldn't turn the fallback off", e),
        };
    }
    // An agent going back to the default model.
    let reset = matches!(model.to_lowercase().as_str(), "default" | "reset" | "none");
    let agent = match target_l.as_str() {
        "all" | "everyone" | "default" | "fallback" => None,
        "me" | "yourself" | "self" => Some(me.clone()),
        _ => match team::find(st, &target) {
            Some(a) => Some(a),
            None => return err(format!("No agent called “{target}”"), format!("there is no agent called {target}")),
        },
    };
    if reset {
        let Some(agent) = agent else {
            return err("Nothing to reset", "use model \"default\" only together with an agent as the target");
        };
        return match st.store.update_agent(&agent.id, AgentPatch { provider: Some(String::new()), model: Some(String::new()), ..Default::default() }) {
            Ok(Some(a)) => {
                st.emit(Event::AgentUpdated { agent: a.clone() });
                let d = st.default_model().map(|d| label(st, &d)).unwrap_or_default();
                ok(format!("{} now follows the default model", a.name), format!("{} now uses the default model, {d}.", a.name))
            }
            Ok(None) => err("Agent not found", "the agent no longer exists"),
            Err(e) => err("Couldn't reset the model", e.to_string()),
        };
    }

    let provider = match resolve_provider(st, model, provider).await {
        Ok(p) => p,
        Err(e) => return err(format!("Couldn't switch to {model}"), e),
    };
    let name = st.provider_name(&provider);
    if target_l == "fallback" {
        return match st.set_fallback(true, &provider, model) {
            Ok(()) => ok(
                format!("Fallback set to {model} ({name})"),
                format!("The fallback is now on: {model} from {name}. It takes over when the main provider fails."),
            ),
            Err(e) => err(format!("Couldn't set the fallback to {model}"), e),
        };
    }
    match agent {
        None => match st.set_default_model(&provider, model) {
            Ok(()) => ok(
                format!("Switched everyone to {model} ({name})"),
                format!(
                    "Done: every agent without its own model now uses {model} from {name}, starting with the next \
                     reply (this reply still finishes on the previous model)."
                ),
            ),
            Err(e) => err(format!("Couldn't switch to {model}"), e),
        },
        Some(agent) => {
            match st.store.update_agent(&agent.id, AgentPatch { provider: Some(provider.clone()), model: Some(model.to_owned()), ..Default::default() }) {
                Ok(Some(a)) => {
                    st.emit(Event::AgentUpdated { agent: a.clone() });
                    ok(
                        format!("{} now uses {model} ({name})", a.name),
                        format!("{} now uses {model} from {name}, starting with its next reply.", a.name),
                    )
                }
                Ok(None) => err("Agent not found", "the agent no longer exists"),
                Err(e) => err(format!("Couldn't switch {} to {model}", agent.name), e.to_string()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_model_matches() {
        let models: Vec<String> = ["deepseek-flash", "deepseek-v4-pro", "gpt-4o"].iter().map(|s| s.to_string()).collect();
        assert_eq!(close_matches("v4-pro", &models), vec!["deepseek-v4-pro".to_string()]);
        assert!(close_matches("deepseek", &models).len() == 2);
        assert!(close_matches("claude", &models).is_empty());
    }
}
