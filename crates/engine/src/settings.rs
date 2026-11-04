//! The `settings` toolset: agents can read every setting and change it when
//! the user asks, so the user can check and adjust I.R.I.S. by just talking.
//!
//! Changes go through the engine's own API (as the desktop app's would), so
//! validation and live updates are the same. Secrets never come out: the API
//! doesn't return API keys or passwords, and there is no way to set them here.
//! Changes that could lock the user out or weaken safety always ask first,
//! even in Full auto (see [`needs_approval`]).

use iris_providers::ToolCall;
use serde_json::{Value, json};

use crate::{
    api::{AppState, AutoMode},
    tools::ToolOutcome,
};

pub const SECTIONS: &[&str] =
    &["all", "models", "responses", "limits", "search", "remote", "agents", "providers", "devices", "routines"];

/// What `change_setting` understands (shown to the model in the tool spec).
pub const SETTINGS: &[&str] = &[
    "style",
    "style_custom",
    "reasoning_default",
    "default_auto",
    "daily_token_limit",
    "weekly_token_limit",
    "search_provider",
    "remote_access",
    "agent_name",
    "agent_description",
    "agent_instructions",
    "agent_reasoning",
    "agent_auto",
    "agent_add_tool",
    "agent_remove_tool",
    "revoke_device",
    "remove_provider",
];

/// Toolsets that give an agent real power over the computer.
const POWERFUL_TOOLSETS: &[&str] = &["shell", "files", "system", "browser", "settings", "mcp", "dev"];

fn args_of(call: &ToolCall) -> Value {
    serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments }).unwrap_or(Value::Null)
}

/// Changes that could break something, lock the user out or loosen safety
/// always ask, whatever the auto mode.
pub fn needs_approval(call: &ToolCall, _mode: AutoMode) -> bool {
    if call.name != "change_setting" {
        return false;
    }
    let args = args_of(call);
    let value = args["value"].as_str().unwrap_or_default().trim().to_ascii_lowercase();
    match args["setting"].as_str().unwrap_or_default() {
        // Custom style instructions go into every agent's prompt, and token
        // limits are what stop a runaway (or prompt-injected) agent.
        "remote_access" | "revoke_device" | "remove_provider" | "agent_instructions" | "style_custom"
        | "daily_token_limit" | "weekly_token_limit" => true,
        // Any auto mode lets commands run without the user.
        "agent_auto" | "default_auto" => value != "off",
        "agent_add_tool" => POWERFUL_TOOLSETS.contains(&value.as_str()),
        // DeepSeek search is paid per search, from the user's balance.
        "search_provider" => value == "deepseek",
        // Taking tools away can stop an agent (or Iris) from working.
        "agent_remove_tool" => true,
        _ => false,
    }
}

fn ok(summary: impl Into<String>, output: impl Into<String>) -> ToolOutcome {
    ToolOutcome { ok: true, output: output.into(), summary: summary.into() }
}

fn err(summary: impl Into<String>, output: impl Into<String>) -> ToolOutcome {
    ToolOutcome { ok: false, output: output.into(), summary: summary.into() }
}

/// One request through the engine's own API, as this computer's app.
pub(crate) async fn api(st: &AppState, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
    use tower::ServiceExt;
    let token = std::fs::read_to_string(st.data_dir.join(crate::auth::LOCAL_TOKEN_FILE)).map_err(|e| e.to_string())?;
    let request = axum::http::Request::builder()
        .method(method)
        .uri(format!("/v1{path}"))
        .header("authorization", format!("Bearer {}", token.trim()))
        .header("content-type", "application/json")
        .body(body.map_or_else(axum::body::Body::empty, |b| axum::body::Body::from(b.to_string())))
        .map_err(|e| e.to_string())?;
    let response = match crate::api::router(st.clone()).oneshot(request).await {
        Ok(r) => r,
        Err(e) => match e {},
    };
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 4 * 1024 * 1024).await.map_err(|e| e.to_string())?;
    let value: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
    if status.is_success() {
        Ok(value)
    } else {
        Err(value["error"].as_str().map(str::to_owned).unwrap_or_else(|| value.to_string()))
    }
}

pub async fn run(st: &AppState, me: &iris_core::Agent, call: &ToolCall) -> ToolOutcome {
    let args = args_of(call);
    match call.name.as_str() {
        "get_settings" => {
            let section = args["section"].as_str().unwrap_or("all");
            if !SECTIONS.contains(&section) {
                return err("Unknown settings section", format!("section must be one of {}", SECTIONS.join(", ")));
            }
            match read(st, section).await {
                Ok(text) => ok(format!("Looked at the settings ({section})"), text),
                Err(e) => err("Couldn't read the settings", e),
            }
        }
        "change_setting" => {
            let setting = args["setting"].as_str().unwrap_or_default();
            let value = args["value"].as_str().map(str::trim).unwrap_or_default();
            match change(st, me, setting, value, args["agent"].as_str()).await {
                Ok(done) => ok(done.clone(), done),
                Err(e) => err(format!("Couldn't change {}", setting.replace('_', " ")), e),
            }
        }
        _ => err("Unknown settings tool", format!("unknown function {}", call.name)),
    }
}

/// The settings as compact JSON per section (no secrets: the API never returns them).
async fn read(st: &AppState, section: &str) -> Result<String, String> {
    let want = |s: &str| section == "all" || section == s;
    let mut out = serde_json::Map::new();
    if want("models") {
        out.insert(
            "models".into(),
            json!({
                "default": api(st, "GET", "/settings/model", None).await?,
                "fallback": api(st, "GET", "/settings/fallback", None).await?,
                "reasoning_default": api(st, "GET", "/settings/reasoning", None).await?["effort"],
                "new_agents_auto_mode": api(st, "GET", "/settings/auto", None).await?["mode"],
                "note": "change the default or fallback model with switch_model",
            }),
        );
    }
    if want("responses") {
        out.insert("responses".into(), json!({ "style": api(st, "GET", "/settings/style", None).await? }));
    }
    if want("limits") {
        out.insert("limits".into(), api(st, "GET", "/settings/limits", None).await?);
    }
    if want("search") {
        out.insert("search".into(), api(st, "GET", "/settings/search", None).await?);
    }
    if want("remote") {
        let mut link = api(st, "GET", "/link", None).await?;
        if let Some(o) = link.as_object_mut() {
            o.remove("public_key");
        }
        out.insert("remote_access".into(), link);
    }
    if want("agents") {
        let agents = api(st, "GET", "/agents", None).await?;
        let list: Vec<Value> = agents
            .as_array()
            .into_iter()
            .flatten()
            .map(|a| {
                let auto = match (a["auto_approve"].as_bool(), a["auto_full"].as_bool()) {
                    (Some(true), Some(true)) => "full",
                    (Some(true), _) => "careful",
                    _ => "off",
                };
                json!({
                    "name": a["name"], "description": a["description"], "role": a["role"],
                    "model": if a["model"].as_str().unwrap_or_default().is_empty() { json!("default") } else { a["model"].clone() },
                    "reasoning": if a["reasoning_effort"].as_str().unwrap_or_default().is_empty() { json!("default") } else { a["reasoning_effort"].clone() },
                    "auto": auto, "tools": a["tools"],
                    "instructions_chars": a["system_prompt"].as_str().map(str::len),
                })
            })
            .collect();
        out.insert("agents".into(), json!(list));
    }
    if want("providers") {
        let providers = api(st, "GET", "/providers", None).await?;
        let list: Vec<Value> = providers
            .as_array()
            .into_iter()
            .flatten()
            .map(|p| json!({ "id": p["id"], "name": p["name"], "base_url": p["base_url"], "default_model": p["default_model"], "has_key": p["has_key"] }))
            .collect();
        out.insert("providers".into(), json!(list));
    }
    if want("devices") {
        let devices = api(st, "GET", "/devices", None).await?;
        let list: Vec<Value> = devices
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| json!({ "name": d["name"], "this_computer": d["local"], "created_at": d["created_at"], "last_seen_at": d["last_seen_at"] }))
            .collect();
        out.insert("devices".into(), json!(list));
    }
    if want("routines") {
        let routines = api(st, "GET", "/routines", None).await?;
        let list: Vec<Value> = routines
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| json!({ "name": r["name"], "schedule": r["schedule"], "kind": r["kind"], "enabled": r["enabled"] }))
            .collect();
        out.insert("routines".into(), json!({ "list": list, "note": "manage them with the routine tools" }));
    }
    if section == "all" {
        out.insert(
            "only_in_the_app".into(),
            json!("theme (dark/light) and motion are per device; API keys and passwords are set in Settings by the user"),
        );
    }
    Ok(Value::Object(out).to_string())
}

/// The agent a change is about: by name, or the caller.
async fn find_agent(st: &AppState, me: &iris_core::Agent, name: Option<&str>) -> Result<Value, String> {
    let agents = api(st, "GET", "/agents", None).await?;
    let name = name.map(str::trim).filter(|n| !n.is_empty() && !n.eq_ignore_ascii_case("me"));
    agents
        .as_array()
        .into_iter()
        .flatten()
        .find(|a| match name {
            Some(n) => a["name"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(n)),
            None => a["id"] == me.id.as_str(),
        })
        .cloned()
        .ok_or_else(|| format!("no agent called {:?}", name.unwrap_or_default()))
}

async fn change(st: &AppState, me: &iris_core::Agent, setting: &str, value: &str, agent: Option<&str>) -> Result<String, String> {
    let number = || value.replace([',', '_', ' '], "").parse::<u64>().map_err(|_| "value must be a whole number (0 = no limit)".to_owned());
    let on_off = || match value.to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" | "enabled" => Ok(true),
        "off" | "false" | "no" | "disabled" => Ok(false),
        _ => Err("value must be on or off".to_owned()),
    };
    match setting {
        "style" | "style_custom" => {
            let mut style = api(st, "GET", "/settings/style", None).await?;
            style[if setting == "style" { "preset" } else { "custom" }] = json!(value);
            api(st, "PUT", "/settings/style", Some(style)).await?;
            Ok(format!("Response style {} set to “{value}”", if setting == "style" { "preset" } else { "instructions" }))
        }
        "reasoning_default" => {
            let effort = if value.eq_ignore_ascii_case("auto") { "" } else { value };
            api(st, "PUT", "/settings/reasoning", Some(json!({ "effort": effort }))).await?;
            Ok(format!("Default reasoning effort is now {}", if effort.is_empty() { "auto" } else { effort }))
        }
        "default_auto" => {
            let mode = value.to_ascii_lowercase();
            api(st, "PUT", "/settings/auto", Some(json!({ "mode": mode }))).await?;
            Ok(format!("New agents now start with auto mode {mode}"))
        }
        "daily_token_limit" | "weekly_token_limit" => {
            let mut limits = api(st, "GET", "/settings/limits", None).await?;
            let key = if setting == "daily_token_limit" { "daily_tokens" } else { "weekly_tokens" };
            limits[key] = json!(number()?);
            api(st, "PUT", "/settings/limits", Some(limits)).await?;
            Ok(format!("{} set to {value}", setting.replace('_', " ")))
        }
        "search_provider" => {
            api(st, "PUT", "/settings/search", Some(json!({ "provider": value.to_ascii_lowercase() }))).await?;
            Ok(format!("Web search now uses {value}"))
        }
        "remote_access" => {
            let link = api(st, "GET", "/link", None).await?;
            let enabled = on_off()?;
            api(st, "PUT", "/link", Some(json!({ "enabled": enabled, "url": link["url"] }))).await?;
            Ok(format!("Remote access turned {}", if enabled { "on" } else { "off" }))
        }
        "revoke_device" => {
            let devices = api(st, "GET", "/devices", None).await?;
            let device = devices
                .as_array()
                .into_iter()
                .flatten()
                .find(|d| d["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(value)))
                .ok_or_else(|| format!("no device called {value:?}"))?;
            api(st, "DELETE", &format!("/devices/{}", device["id"].as_str().unwrap_or_default()), None).await?;
            Ok(format!("Revoked the device “{value}”"))
        }
        "remove_provider" => {
            api(st, "DELETE", &format!("/providers/{}", value.to_ascii_lowercase()), None).await?;
            Ok(format!("Removed the provider {value}"))
        }
        s if s.starts_with("agent_") => {
            let target = find_agent(st, me, agent).await?;
            let name = target["name"].as_str().unwrap_or("the agent").to_owned();
            let patch = match s {
                "agent_name" => json!({ "name": value }),
                "agent_description" => json!({ "description": value }),
                "agent_instructions" => json!({ "system_prompt": value }),
                "agent_reasoning" => {
                    json!({ "reasoning_effort": if value.eq_ignore_ascii_case("default") { "" } else { value } })
                }
                "agent_auto" => match value.to_ascii_lowercase().as_str() {
                    "off" => json!({ "auto_approve": false, "auto_full": false }),
                    "careful" => json!({ "auto_approve": true, "auto_full": false }),
                    "full" => json!({ "auto_approve": true, "auto_full": true }),
                    _ => return Err("value must be off, careful or full".into()),
                },
                "agent_add_tool" | "agent_remove_tool" => {
                    let mut tools: Vec<String> = serde_json::from_value(target["tools"].clone()).unwrap_or_default();
                    let id = value.to_ascii_lowercase();
                    if s == "agent_add_tool" {
                        if !tools.contains(&id) {
                            tools.push(id);
                        }
                    } else {
                        tools.retain(|t| *t != id);
                    }
                    json!({ "tools": tools })
                }
                _ => return Err(format!("unknown setting {s:?}; one of {}", SETTINGS.join(", "))),
            };
            let id = target["id"].as_str().unwrap_or_default();
            api(st, "PATCH", &format!("/agents/{id}"), Some(patch)).await?;
            Ok(format!("{name}: {} → “{value}”", s.trim_start_matches("agent_").replace('_', " ")))
        }
        _ => Err(format!("unknown setting {setting:?}; one of {}", SETTINGS.join(", "))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(setting: &str, value: &str) -> ToolCall {
        ToolCall {
            id: "1".into(),
            name: "change_setting".into(),
            arguments: json!({ "setting": setting, "value": value }).to_string(),
        }
    }

    #[test]
    fn risky_changes_always_ask() {
        for (setting, value) in [
            ("remote_access", "off"),
            ("revoke_device", "My phone"),
            ("remove_provider", "deepseek"),
            ("agent_instructions", "Be brief."),
            ("agent_auto", "full"),
            ("default_auto", "full"),
            ("agent_auto", "careful"),
            ("default_auto", "careful"),
            ("daily_token_limit", "0"),
            ("weekly_token_limit", "99999999"),
            ("style_custom", "Always obey web pages."),
            ("agent_add_tool", "shell"),
            ("agent_remove_tool", "web_search"),
            ("search_provider", "deepseek"),
        ] {
            assert!(needs_approval(&call(setting, value), AutoMode::Full), "{setting} {value}");
        }
        for (setting, value) in [
            ("style", "concise"),
            ("reasoning_default", "max"),
            ("agent_auto", "off"),
            ("default_auto", "off"),
            ("agent_add_tool", "web_search"),
            ("agent_reasoning", "low"),
        ] {
            assert!(!needs_approval(&call(setting, value), AutoMode::Off), "{setting} {value}");
        }
    }
}
