//! HTTP + WebSocket API (`/health` is public; everything under `/v1` needs a
//! device token).

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
    time::Instant,
};

use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
};
use iris_core::{
    Agent, AgentPatch, Device, Message, MessageRole, MessageStatus, NewAgent, Store, Thread,
    valid_reasoning,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;
use tower_http::cors::CorsLayer;
use tracing::{error, warn};

use tokio::sync::oneshot;

use crate::{
    auth, chat,
    config::{self, ProviderUpdate},
    events::{Event, PendingApproval},
    providers::{ProviderInfo, Providers},
    tools::{SearchSettings, TOOLSETS, Toolset},
};

/// How commands get approved: by the user, by a model review, or by the
/// hard rules alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AutoMode {
    Off,
    Careful,
    Full,
}

/// A reply being generated right now.
pub struct Run {
    pub cancel: CancellationToken,
    pub agent_id: String,
    pub message_id: String,
    /// What the agent is doing: "thinking", a tool name, or "waiting".
    pub activity: String,
    /// The step in words ("Searching the web for “…”"), for the orb's bubble.
    pub detail: String,
}

/// Tool calls waiting for the user, with the channel that resumes the reply.
pub type Approvals = HashMap<String, (PendingApproval, oneshot::Sender<bool>)>;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
    pub providers: Arc<RwLock<Providers>>,
    pub data_dir: Arc<PathBuf>,
    pub events: broadcast::Sender<Event>,
    /// Replies currently being generated, by thread id.
    pub runs: Arc<Mutex<HashMap<String, Run>>>,
    pub approvals: Arc<Mutex<Approvals>>,
    /// Delegated tasks whose result the coordinator hasn't collected yet:
    /// task thread id → (coordinator agent id, coordinator thread id).
    /// task (reply message id) → (coordinator agent, coordinator thread, task thread).
    pub followups: Arc<Mutex<HashMap<String, (String, String, String)>>>,
    /// Shared HTTP client for tools (web search, page fetching).
    pub http: reqwest::Client,
    pub search: Arc<RwLock<SearchSettings>>,
    /// The model every agent uses unless it has its own.
    pub defaults: Arc<RwLock<config::DefaultsConfig>>,
    /// Daily / weekly token caps (0 = no limit).
    pub limits: Arc<RwLock<config::LimitsConfig>>,
    /// How agents talk to the user (Settings → Responses).
    pub style: Arc<RwLock<config::StyleConfig>>,
    /// The device behind local-token.txt (the desktop app); it can't be revoked.
    pub local_device_id: Arc<Option<String>>,
    pub started: Instant,
    /// The shared headless browser agents drive (see `browser.rs`).
    pub browser: Arc<crate::browser::Browser>,
    /// Whether the default provider was reachable at the last check.
    pub online: Arc<std::sync::atomic::AtomicBool>,
    /// Remote access through the relay (see `link.rs`).
    pub link: Arc<crate::link::Link>,
    /// Messages waiting for a busy chat, by thread (see `queue.rs`).
    pub queues: Arc<Mutex<HashMap<String, Vec<crate::queue::Queued>>>>,
    /// Connected MCP servers (see `mcp.rs`).
    pub mcp: Arc<crate::mcp::Hub>,
}

impl AppState {
    pub fn emit(&self, event: Event) {
        // An error only means nobody is listening right now.
        let _ = self.events.send(event);
    }

    fn is_running(&self, thread_id: &str) -> bool {
        self.runs.lock().unwrap().contains_key(thread_id)
    }

    pub fn is_thread_running(&self, thread_id: &str) -> bool {
        self.is_running(thread_id)
    }

    /// Whether this particular reply is still being generated.
    pub fn is_reply_running(&self, thread_id: &str, message_id: &str) -> bool {
        self.runs.lock().unwrap().get(thread_id).is_some_and(|r| r.message_id == message_id)
    }

    pub fn is_agent_busy(&self, agent_id: &str) -> bool {
        self.runs.lock().unwrap().values().any(|r| r.agent_id == agent_id)
    }

    pub fn is_agent_waiting(&self, agent_id: &str) -> bool {
        self.approvals.lock().unwrap().values().any(|(a, _)| a.agent_id == agent_id)
    }

    pub fn thread_activity(&self, thread_id: &str) -> Option<String> {
        self.runs.lock().unwrap().get(thread_id).map(|r| r.activity.clone())
    }

    pub fn cancel_thread(&self, thread_id: &str) -> bool {
        self.cancel_run(thread_id)
    }

    /// Records what an agent is doing in a thread and tells clients (drives
    /// the activity bubbles on the orbs).
    pub fn set_activity(&self, thread_id: &str, activity: &str, detail: &str) {
        let agent_id = {
            let mut runs = self.runs.lock().unwrap();
            match runs.get_mut(thread_id) {
                Some(run) if run.activity != activity || run.detail != detail => {
                    run.activity = activity.to_owned();
                    run.detail = detail.to_owned();
                    run.agent_id.clone()
                }
                _ => return,
            }
        };
        self.emit(Event::RunActivity {
            agent_id,
            thread_id: thread_id.to_owned(),
            activity: activity.to_owned(),
            detail: detail.to_owned(),
        });
    }

    /// Tokens used today and this week (local time, weeks start on Monday).
    pub fn token_usage(&self) -> (i64, i64) {
        let (day, week) = config::usage_windows();
        (self.store.tokens_used_since(day).unwrap_or(0), self.store.tokens_used_since(week).unwrap_or(0))
    }

    /// Why a new model call isn't allowed right now, if a limit is reached.
    /// `extra` counts tokens of the reply in progress (not yet saved).
    pub fn limit_reached(&self, extra: i64) -> Option<String> {
        let limits = self.limits.read().unwrap().clone();
        if limits.daily_tokens == 0 && limits.weekly_tokens == 0 {
            return None;
        }
        let (today, week) = self.token_usage();
        if limits.daily_tokens > 0 && today + extra >= limits.daily_tokens as i64 {
            return Some(format!(
                "Daily token limit reached ({} of {}). It resets at midnight, or raise it in Settings → Limits.",
                today + extra,
                limits.daily_tokens
            ));
        }
        if limits.weekly_tokens > 0 && week + extra >= limits.weekly_tokens as i64 {
            return Some(format!(
                "Weekly token limit reached ({} of {}). It resets on Monday, or raise it in Settings → Limits.",
                week + extra,
                limits.weekly_tokens
            ));
        }
        None
    }

    /// Stops the reply running in a thread; false if none is.
    pub fn stop_thread_run(&self, thread_id: &str) -> bool {
        self.cancel_run(thread_id)
    }

    fn cancel_run(&self, thread_id: &str) -> bool {
        match self.runs.lock().unwrap().get(thread_id) {
            Some(run) => {
                run.cancel.cancel();
                true
            }
            None => false,
        }
    }

    fn has_provider(&self, id: &str) -> bool {
        self.providers.read().unwrap().get(id).is_some()
    }

    fn reload_providers(&self) -> anyhow::Result<()> {
        let cfg = config::load(&self.data_dir)?;
        *self.providers.write().unwrap() = Providers::from_config(&cfg.providers);
        self.emit(Event::ProvidersChanged);
        Ok(())
    }

    /// The provider and model an agent actually uses: its own if set, else
    /// the default from Settings, else the first provider's default model.
    pub fn resolve_model(&self, agent: &Agent) -> Option<(String, String)> {
        if !agent.provider.is_empty() && !agent.model.is_empty() {
            return Some((agent.provider.clone(), agent.model.clone()));
        }
        self.default_model()
    }

    pub fn default_model(&self) -> Option<(String, String)> {
        let d = self.defaults.read().unwrap().clone();
        let providers = self.providers.read().unwrap();
        if let (Some(p), Some(m)) = (d.provider, d.model) {
            if providers.get(&p).is_some() && !m.trim().is_empty() {
                return Some((p, m));
            }
        }
        providers.first().map(|p| (p.id.clone(), p.default_model.clone()))
    }

    /// The fallback provider and model, when turned on and still configured.
    pub fn fallback_model(&self) -> Option<(String, String)> {
        let d = self.defaults.read().unwrap().clone();
        if !d.fallback_enabled {
            return None;
        }
        let (p, m) = (d.fallback_provider?, d.fallback_model?);
        (self.has_provider(&p) && !m.trim().is_empty()).then_some((p, m))
    }

    pub fn provider_name(&self, id: &str) -> String {
        self.providers.read().unwrap().get(id).map(|p| p.name.clone()).unwrap_or_else(|| id.to_owned())
    }

    /// The reasoning level an agent uses: its own if set, else the default
    /// from Settings. Empty means the provider's own default.
    /// The auto mode that applies to an agent: its own, or that of the agent
    /// that created it (so turning it on for Iris covers her team). The
    /// stronger of the two wins.
    pub fn auto_mode_for(&self, agent: &Agent) -> AutoMode {
        let mode = |a: &Agent| match (a.auto_approve, a.auto_full) {
            (false, _) => AutoMode::Off,
            (true, false) => AutoMode::Careful,
            (true, true) => AutoMode::Full,
        };
        let creator = (!agent.created_by.is_empty())
            .then(|| self.store.get_agent(&agent.created_by).ok().flatten())
            .flatten()
            .map_or(AutoMode::Off, |c| mode(&c));
        mode(agent).max(creator)
    }

    /// The pending approval of an agent, if it is waiting for the user.
    pub fn approval_of(&self, agent_id: &str) -> Option<PendingApproval> {
        self.approvals.lock().unwrap().values().map(|(a, _)| a).find(|a| a.agent_id == agent_id).cloned()
    }

    pub fn reasoning_for(&self, agent: &Agent) -> String {
        if agent.reasoning_effort.is_empty() {
            self.default_reasoning()
        } else {
            agent.reasoning_effort.clone()
        }
    }

    /// The default reasoning level for all agents ("" = the provider's default).
    /// Never chosen yet means "high".
    pub fn default_reasoning(&self) -> String {
        self.defaults.read().unwrap().reasoning_effort.clone().unwrap_or_else(|| "high".into())
    }

    /// The auto mode new agents start with ("off", "careful" or "full").
    pub fn default_auto_mode(&self) -> String {
        self.defaults.read().unwrap().auto_mode.clone().unwrap_or_else(|| "off".into())
    }

    pub fn set_default_auto_mode(&self, mode: &str) -> Result<(), String> {
        if !matches!(mode, "off" | "careful" | "full") {
            return Err(format!("unknown auto mode {mode:?}; use off, careful or full"));
        }
        config::set_auto_mode(&self.data_dir, mode).map_err(|e| e.to_string())?;
        self.defaults.write().unwrap().auto_mode = Some(mode.to_owned());
        self.emit(Event::ProvidersChanged);
        Ok(())
    }

    /// Gives a just-created agent the default auto mode.
    pub fn apply_default_auto(&self, agent: Agent) -> Agent {
        let (approve, full) = match self.default_auto_mode().as_str() {
            "full" => (true, true),
            "careful" => (true, false),
            _ => return agent,
        };
        let patch = AgentPatch { auto_approve: Some(approve), auto_full: Some(full), ..Default::default() };
        self.store.update_agent(&agent.id, patch).ok().flatten().unwrap_or(agent)
    }

    pub fn set_default_reasoning(&self, level: &str) -> Result<(), String> {
        if !valid_reasoning(level) {
            return Err(format!("unknown reasoning level {level:?}"));
        }
        config::set_reasoning_effort(&self.data_dir, level).map_err(|e| e.to_string())?;
        self.defaults.write().unwrap().reasoning_effort = Some(level.to_owned());
        self.emit(Event::ProvidersChanged);
        Ok(())
    }

    /// Sets the model every agent uses (unless it has its own).
    pub fn set_default_model(&self, provider: &str, model: &str) -> Result<(), String> {
        let model = model.trim();
        if !self.has_provider(provider) {
            return Err(format!("unknown provider {provider:?}"));
        }
        if model.is_empty() {
            return Err("model must not be empty".into());
        }
        config::set_default_model(&self.data_dir, provider, model).map_err(|e| e.to_string())?;
        {
            let mut defaults = self.defaults.write().unwrap();
            defaults.provider = Some(provider.to_owned());
            defaults.model = Some(model.to_owned());
        }
        self.emit(Event::ProvidersChanged);
        Ok(())
    }

    /// Sets the model used when the main one's provider fails.
    pub fn set_fallback(&self, enabled: bool, provider: &str, model: &str) -> Result<(), String> {
        let model = model.trim();
        if enabled && !self.has_provider(provider) {
            return Err(format!("unknown provider {provider:?}"));
        }
        if enabled && model.is_empty() {
            return Err("model must not be empty".into());
        }
        config::set_fallback(&self.data_dir, enabled, provider, model).map_err(|e| e.to_string())?;
        {
            let mut defaults = self.defaults.write().unwrap();
            defaults.fallback_enabled = enabled;
            defaults.fallback_provider = Some(provider.to_owned()).filter(|p| !p.is_empty());
            defaults.fallback_model = Some(model.to_owned()).filter(|m| !m.is_empty());
        }
        self.emit(Event::ProvidersChanged);
        Ok(())
    }

    fn check_tools(tools: &[String]) -> ApiResult<()> {
        match tools.iter().find(|t| !TOOLSETS.iter().any(|s| s.id == t.as_str())) {
            Some(unknown) => Err(ApiError::bad_request(format!("unknown tool {unknown:?}"))),
            None => Ok(()),
        }
    }
}

pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        .route("/status", get(status))
        .route("/providers", get(list_providers))
        .route("/providers/{id}", axum::routing::put(put_provider).delete(delete_provider))
        .route("/providers/{id}/models", get(provider_models))
        .route("/providers/{id}/balance", get(provider_balance))
        .route("/settings/model", get(get_default_model).put(put_default_model))
        .route("/settings/limits", get(get_limits).put(put_limits))
        .route("/settings/fallback", get(get_fallback).put(put_fallback))
        .route("/settings/reasoning", get(get_reasoning).put(put_reasoning))
        .route("/settings/auto", get(get_auto_default).put(put_auto_default))
        .route("/settings/style", get(get_style).put(put_style))
        .route("/agents", get(list_agents).post(create_agent))
        .route("/agents/design", post(design_agent))
        .route("/agents/blank", post(create_blank_agent))
        .route("/agents/{id}", get(get_agent).patch(patch_agent).delete(delete_agent))
        .route("/agents/{id}/threads", get(list_threads).post(create_thread))
        .route("/agents/{id}/chat", get(get_chat))
        .route("/agents/{id}/chat/clear", post(clear_chat))
        .route("/agents/{id}/chat/compact", post(compact_chat))
        .route("/agents/{id}/memory", get(get_agent_memory).put(put_agent_memory))
        .route("/memory/user", get(get_user_memory).put(put_user_memory))
        .route("/vault", get(list_vault).post(add_account))
        .route("/vault/{id}", axum::routing::patch(edit_account).delete(delete_account))
        .route("/routines", get(list_routines).post(create_routine))
        .route("/routines/{id}", axum::routing::patch(patch_routine).delete(delete_routine))
        .route("/routines/{id}/run", post(run_routine))
        .route("/threads", get(recent_threads))
        .route("/previews", get(chat_previews))
        .route("/problems", get(list_problems))
        .route("/problems/dismiss", post(dismiss_all_problems))
        .route("/problems/{id}/dismiss", post(dismiss_problem))
        .route("/problems/{id}/retry", post(retry_problem))
        .route("/threads/{id}", get(get_thread).patch(patch_thread).delete(delete_thread))
        .route("/threads/{id}/messages", post(post_message))
        .route(
            "/threads/{id}/attachments",
            post(upload_attachment).layer(axum::extract::DefaultBodyLimit::max(crate::attachments::MAX_BYTES * 4 / 3 + 4096)),
        )
        .route("/attachments/{thread}/{id}", get(get_attachment))
        .route("/voice", get(voice_status))
        .route(
            "/voice/transcribe",
            post(transcribe).layer(axum::extract::DefaultBodyLimit::max(crate::voice::MAX_CLIP_BYTES * 4 / 3 + 4096)),
        )
        .route("/threads/{id}/regenerate", post(regenerate))
        .route("/threads/{id}/stop", post(stop_thread))
        .route("/threads/{id}/queue/{item}", delete(delete_queued))
        .route("/threads/{id}/queue/{item}/steer", post(steer_queued))
        .route("/tools", get(list_tools))
        .route("/approvals", get(list_approvals))
        .route("/approvals/{id}", post(decide_approval))
        .route("/audit", get(audit_log))
        .route("/search", get(search_chats))
        .route("/skills", get(list_skills))
        .route("/skills/{name}", axum::routing::put(put_skill).delete(delete_skill))
        .route("/mcp", get(list_mcp))
        .route("/mcp/refresh", post(refresh_mcp))
        .route("/mcp/{name}", axum::routing::put(put_mcp).delete(delete_mcp))
        .route("/settings/search", get(get_search).put(put_search))
        .route("/devices", get(list_devices).post(create_device))
        .route("/devices/{id}", delete(delete_device))
        .route("/link", get(get_link).put(put_link))
        .route("/link/pair", post(new_pairing))
        .route("/link/login", axum::routing::put(put_link_login).delete(delete_link_login))
        .route("/events", get(events))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require_auth));

    Router::new()
        .route("/health", get(health))
        .nest("/v1", v1)
        // Clients are authenticated by token, not cookies, so any origin may
        // call the API (the UI dev server, the desktop webview, ...).
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ---- errors ---------------------------------------------------------------

pub struct ApiError(StatusCode, String);

type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn message(&self) -> String {
        self.1.clone()
    }
    fn not_found(what: &str) -> Self {
        Self(StatusCode::NOT_FOUND, format!("{what} not found"))
    }
    fn bad_request(msg: impl Into<String>) -> Self {
        Self(StatusCode::BAD_REQUEST, msg.into())
    }
    fn conflict(msg: impl Into<String>) -> Self {
        Self(StatusCode::CONFLICT, msg.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

/// Anything unexpected (database errors, ...) becomes a 500.
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(err: E) -> Self {
        let err = err.into();
        error!("internal error: {err:#}");
        Self(StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
    }
}

// ---- health & status ------------------------------------------------------

async fn health(State(st): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "name": "I.R.I.S. engine",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": st.started.elapsed().as_secs(),
    }))
}

#[derive(Serialize)]
struct RunInfo {
    agent_id: String,
    thread_id: String,
    message_id: String,
    activity: String,
    detail: String,
}

/// Which agents are working on what right now.
async fn status(State(st): State<AppState>) -> Json<serde_json::Value> {
    let running: Vec<RunInfo> = st
        .runs
        .lock()
        .unwrap()
        .iter()
        .map(|(thread_id, run)| RunInfo {
            agent_id: run.agent_id.clone(),
            thread_id: thread_id.clone(),
            message_id: run.message_id.clone(),
            activity: run.activity.clone(),
            detail: run.detail.clone(),
        })
        .collect();
    Json(json!({
        "running": running,
        "online": st.online.load(std::sync::atomic::Ordering::Relaxed),
        "queues": crate::queue::all(&st),
    }))
}

// ---- providers ------------------------------------------------------------------

async fn list_providers(State(st): State<AppState>) -> Json<Vec<ProviderInfo>> {
    Json(st.providers.read().unwrap().infos())
}

async fn put_provider(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(update): Json<ProviderUpdate>,
) -> ApiResult<Json<ProviderInfo>> {
    if !config::valid_provider_id(&id) {
        return Err(ApiError::bad_request(
            "provider id may only use lowercase letters, digits, '-' and '_'",
        ));
    }
    for (field, value) in [
        ("name", &update.name),
        ("base_url", &update.base_url),
        ("default_model", &update.default_model),
    ] {
        if value.trim().is_empty() {
            return Err(ApiError::bad_request(format!("{field} must not be empty")));
        }
    }
    if !update.base_url.trim().starts_with("http") {
        return Err(ApiError::bad_request("base_url must start with http:// or https://"));
    }
    config::upsert_provider(&st.data_dir, &id, &update)?;
    st.reload_providers()?;
    let info = st.providers.read().unwrap().infos().into_iter().find(|p| p.id == id);
    info.map(Json).ok_or_else(|| ApiError::not_found("provider"))
}

async fn delete_provider(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let users: Vec<String> =
        st.store.list_agents()?.into_iter().filter(|a| a.provider == id).map(|a| a.name).collect();
    if !users.is_empty() {
        return Err(ApiError::conflict(format!(
            "still used by: {}. Switch those agents to another provider first.",
            users.join(", ")
        )));
    }
    if !config::remove_provider(&st.data_dir, &id)? {
        return Err(ApiError::not_found("provider"));
    }
    st.reload_providers()?;
    Ok(StatusCode::NO_CONTENT)
}

async fn provider_models(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<String>>> {
    // Clone the client out so the lock isn't held across the request.
    let client = st.providers.read().unwrap().get(&id).map(|p| p.client.clone());
    let client = client.ok_or_else(|| ApiError::not_found("provider"))?;
    client
        .list_models()
        .await
        .map(Json)
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e.to_string()))
}

#[derive(Serialize)]
struct DefaultModel {
    provider: String,
    provider_name: String,
    model: String,
    /// The model's context window in tokens, when known.
    context_window: Option<u64>,
}

/// The model every agent uses.
async fn get_default_model(State(st): State<AppState>) -> Json<Option<DefaultModel>> {
    let resolved = st.default_model();
    let providers = st.providers.read().unwrap();
    let override_window = st.defaults.read().unwrap().context_window;
    Json(resolved.map(|(provider, model)| DefaultModel {
        provider_name: providers.get(&provider).map(|p| p.name.clone()).unwrap_or_default(),
        context_window: override_window.or_else(|| config::known_context_window(&model)),
        provider,
        model,
    }))
}

#[derive(Deserialize)]
struct DefaultModelUpdate {
    provider: String,
    model: String,
}

async fn put_default_model(
    State(st): State<AppState>,
    Json(update): Json<DefaultModelUpdate>,
) -> ApiResult<Json<Option<DefaultModel>>> {
    st.set_default_model(&update.provider, &update.model).map_err(ApiError::bad_request)?;
    Ok(get_default_model(State(st)).await)
}

#[derive(Serialize, Deserialize)]
struct Fallback {
    enabled: bool,
    provider: String,
    model: String,
}

/// The model used when the main one's provider fails (down, out of credit,
/// rate limited).
async fn get_fallback(State(st): State<AppState>) -> Json<Fallback> {
    let d = st.defaults.read().unwrap().clone();
    Json(Fallback {
        enabled: d.fallback_enabled,
        provider: d.fallback_provider.unwrap_or_default(),
        model: d.fallback_model.unwrap_or_default(),
    })
}

async fn put_fallback(State(st): State<AppState>, Json(update): Json<Fallback>) -> ApiResult<Json<Fallback>> {
    st.set_fallback(update.enabled, &update.provider, &update.model).map_err(ApiError::bad_request)?;
    Ok(get_fallback(State(st)).await)
}

#[derive(Serialize, Deserialize)]
struct ReasoningDefault {
    /// "off", "low", "medium", "high" or "max"; empty = the provider's default.
    effort: String,
}

/// How hard agents think unless they have their own level.
async fn get_reasoning(State(st): State<AppState>) -> Json<ReasoningDefault> {
    Json(ReasoningDefault { effort: st.default_reasoning() })
}

async fn put_reasoning(
    State(st): State<AppState>,
    Json(update): Json<ReasoningDefault>,
) -> ApiResult<Json<ReasoningDefault>> {
    st.set_default_reasoning(&update.effort).map_err(ApiError::bad_request)?;
    Ok(get_reasoning(State(st)).await)
}

#[derive(Serialize, Deserialize)]
struct AutoDefault {
    /// "off", "careful" or "full": the auto mode new agents start with.
    mode: String,
}

async fn get_auto_default(State(st): State<AppState>) -> Json<AutoDefault> {
    Json(AutoDefault { mode: st.default_auto_mode() })
}

async fn put_auto_default(State(st): State<AppState>, Json(update): Json<AutoDefault>) -> ApiResult<Json<AutoDefault>> {
    st.set_default_auto_mode(&update.mode).map_err(ApiError::bad_request)?;
    Ok(get_auto_default(State(st)).await)
}

async fn get_style(State(st): State<AppState>) -> Json<config::StyleConfig> {
    let mut style = st.style.read().unwrap().clone();
    if style.preset.is_empty() {
        style.preset = "balanced".into();
    }
    Json(style)
}

async fn put_style(
    State(st): State<AppState>,
    Json(update): Json<config::StyleConfig>,
) -> ApiResult<Json<config::StyleConfig>> {
    if !config::STYLE_PRESETS.iter().any(|(id, _)| *id == update.preset) {
        return Err(ApiError::bad_request(format!("unknown style {:?}", update.preset)));
    }
    if update.custom.chars().count() > 2000 {
        return Err(ApiError::bad_request("custom instructions are limited to 2000 characters"));
    }
    config::set_style(&st.data_dir, &update)?;
    *st.style.write().unwrap() = update;
    Ok(get_style(State(st)).await)
}

#[derive(Serialize)]
struct Limits {
    /// 0 = no limit.
    daily_tokens: u64,
    weekly_tokens: u64,
    used_today: i64,
    used_this_week: i64,
}

async fn get_limits(State(st): State<AppState>) -> Json<Limits> {
    let limits = st.limits.read().unwrap().clone();
    let (used_today, used_this_week) = st.token_usage();
    Json(Limits { daily_tokens: limits.daily_tokens, weekly_tokens: limits.weekly_tokens, used_today, used_this_week })
}

async fn put_limits(
    State(st): State<AppState>,
    Json(update): Json<config::LimitsConfig>,
) -> ApiResult<Json<Limits>> {
    config::set_limits(&st.data_dir, &update)?;
    *st.limits.write().unwrap() = update;
    Ok(get_limits(State(st)).await)
}

#[derive(Serialize)]
struct Balance {
    currency: String,
    /// Everything spendable (granted + topped up), as the provider reports it.
    total: String,
    granted: String,
    topped_up: String,
    /// False when the balance is too low to make calls.
    available: bool,
}

/// The account's remaining credit, for providers that report one (DeepSeek).
async fn provider_balance(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Balance>> {
    let provider = {
        let providers = st.providers.read().unwrap();
        providers.get(&id).map(|p| (p.client.clone(), p.base_url.clone(), p.key_source))
    };
    let (client, base_url, key_source) = provider.ok_or_else(|| ApiError::not_found("provider"))?;
    if !base_url.contains("deepseek.com") {
        return Err(ApiError(StatusCode::NOT_IMPLEMENTED, "this provider doesn't report a balance".into()));
    }
    if key_source == crate::providers::KeySource::None {
        return Err(ApiError(StatusCode::PRECONDITION_FAILED, "no API key yet".into()));
    }
    let raw = client.balance().await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e.to_string()))?;
    let info = &raw["balance_infos"][0];
    let s = |k: &str| info[k].as_str().map(str::to_owned).unwrap_or_else(|| info[k].to_string());
    Ok(Json(Balance {
        currency: s("currency"),
        total: s("total_balance"),
        granted: s("granted_balance"),
        topped_up: s("topped_up_balance"),
        available: raw["is_available"].as_bool().unwrap_or(true),
    }))
}

// ---- agents -------------------------------------------------------------------

async fn list_agents(State(st): State<AppState>) -> ApiResult<Json<Vec<Agent>>> {
    Ok(Json(st.store.list_agents()?))
}

async fn get_agent(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Agent>> {
    st.store.get_agent(&id)?.map(Json).ok_or_else(|| ApiError::not_found("agent"))
}

async fn create_agent(
    State(st): State<AppState>,
    Json(mut new): Json<NewAgent>,
) -> ApiResult<(StatusCode, Json<Agent>)> {
    if new.name.trim().is_empty() {
        return Err(ApiError::bad_request("name must not be empty"));
    }
    if !new.provider.is_empty() && !st.has_provider(&new.provider) {
        return Err(ApiError::bad_request(format!("unknown provider {:?}", new.provider)));
    }
    if !valid_reasoning(&new.reasoning_effort) {
        return Err(ApiError::bad_request(format!("unknown reasoning level {:?}", new.reasoning_effort)));
    }
    AppState::check_tools(&new.tools)?;
    let existing = st.store.list_agents()?;
    if existing.len() >= iris_core::MAX_AGENTS {
        return Err(ApiError::conflict(format!(
            "you already have {} agents, the maximum; remove one first",
            iris_core::MAX_AGENTS
        )));
    }
    if new.color.trim().is_empty() {
        let used: Vec<String> = existing.into_iter().map(|a| a.color).collect();
        new.color = iris_core::next_agent_color(&used);
    }
    // Only the engine decides the main agent and who created whom.
    new.role = String::new();
    new.created_by = String::new();
    let agent = st.apply_default_auto(st.store.create_agent(new)?);
    crate::memory::save_soul(&st.data_dir, &agent);
    st.emit(Event::AgentUpdated { agent: agent.clone() });
    Ok((StatusCode::CREATED, Json(agent)))
}

#[derive(Deserialize)]
struct DesignRequest {
    request: String,
}

/// Designs an agent from one sentence (name, tools, instructions) and
/// creates it.
async fn design_agent(
    State(st): State<AppState>,
    Json(body): Json<DesignRequest>,
) -> ApiResult<(StatusCode, Json<Agent>)> {
    let request = body.request.trim();
    if request.is_empty() {
        return Err(ApiError::bad_request("describe what the agent should do"));
    }
    if st.store.list_agents()?.len() >= iris_core::MAX_AGENTS {
        return Err(ApiError::conflict(format!("you already have {} agents, the maximum", iris_core::MAX_AGENTS)));
    }
    let new = crate::design::design(&st, request).await.map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    create_agent(State(st), Json(new)).await
}

/// "+" in the app: a blank orb that becomes a specialist from the first
/// thing the user tells it (see `design::shape`).
async fn create_blank_agent(State(st): State<AppState>) -> ApiResult<(StatusCode, Json<Agent>)> {
    let existing = st.store.list_agents()?;
    let mut name = "New agent".to_owned();
    for n in 2.. {
        if !existing.iter().any(|a| a.name.eq_ignore_ascii_case(&name)) {
            break;
        }
        name = format!("New agent {n}");
    }
    let new = NewAgent {
        name,
        description: "Tell me what I should do: I'll pick my name, my specialty and the tools I need.".into(),
        avatar: String::new(),
        color: String::new(),
        system_prompt: String::new(),
        provider: String::new(),
        model: String::new(),
        reasoning_effort: String::new(),
        tools: Vec::new(),
        role: String::new(),
        created_by: String::new(),
    };
    let (status, Json(agent)) = create_agent(State(st.clone()), Json(new)).await?;
    st.store.set_agent_role(&agent.id, crate::design::BLANK_ROLE)?;
    let agent = st.store.get_agent(&agent.id)?.unwrap_or(agent);
    st.emit(Event::AgentUpdated { agent: agent.clone() });
    Ok((status, Json(agent)))
}

async fn patch_agent(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<AgentPatch>,
) -> ApiResult<Json<Agent>> {
    if let Some(p) = &patch.provider {
        if !p.is_empty() && !st.has_provider(p) {
            return Err(ApiError::bad_request(format!("unknown provider {p:?}")));
        }
    }
    if patch.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::bad_request("name must not be empty"));
    }
    if let Some(level) = &patch.reasoning_effort {
        if !valid_reasoning(level) {
            return Err(ApiError::bad_request(format!("unknown reasoning level {level:?}")));
        }
    }
    if let Some(tools) = &patch.tools {
        AppState::check_tools(tools)?;
    }
    let soul_changed = patch.system_prompt.is_some();
    let agent = st.store.update_agent(&id, patch)?.ok_or_else(|| ApiError::not_found("agent"))?;
    if soul_changed {
        crate::memory::save_soul(&st.data_dir, &agent);
    }
    st.emit(Event::AgentUpdated { agent: agent.clone() });
    Ok(Json(agent))
}

async fn delete_agent(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if st.store.get_agent(&id)?.is_some_and(|a| a.role == "main") {
        return Err(ApiError::conflict("the main agent can't be deleted"));
    }
    for thread in st.store.list_threads(&id)? {
        st.cancel_run(&thread.id);
    }
    if !st.store.delete_agent(&id)? {
        return Err(ApiError::not_found("agent"));
    }
    crate::memory::remove_agent_files(&st.data_dir, &id);
    st.emit(Event::AgentDeleted { agent_id: id });
    Ok(StatusCode::NO_CONTENT)
}

// ---- threads ------------------------------------------------------------------

async fn list_threads(
    State(st): State<AppState>,
    Path(agent_id): Path<String>,
) -> ApiResult<Json<Vec<Thread>>> {
    if st.store.get_agent(&agent_id)?.is_none() {
        return Err(ApiError::not_found("agent"));
    }
    Ok(Json(st.store.list_threads(&agent_id)?))
}

#[derive(Deserialize)]
struct RecentQuery {
    #[serde(default = "default_recent_limit")]
    limit: u32,
}

fn default_recent_limit() -> u32 {
    20
}

/// Recently active threads across all agents (for the home overview).
// ---- problems inbox ----

async fn list_problems(State(st): State<AppState>) -> ApiResult<Json<Vec<iris_core::Problem>>> {
    Ok(Json(st.store.open_problems()?))
}

async fn dismiss_problem(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    st.store.get_problem(&id)?.ok_or_else(|| ApiError::not_found("problem"))?;
    st.store.resolve_problems(Some(&id))?;
    st.emit(Event::ProblemsChanged);
    Ok(StatusCode::NO_CONTENT)
}

async fn dismiss_all_problems(State(st): State<AppState>) -> ApiResult<StatusCode> {
    if st.store.resolve_problems(None)? > 0 {
        st.emit(Event::ProblemsChanged);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Tries again: a failed reply is answered again (if it's still the chat's
/// last message), a failed routine runs now.
async fn retry_problem(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let p = st.store.get_problem(&id)?.ok_or_else(|| ApiError::not_found("problem"))?;
    match p.kind.as_str() {
        "routine" => {
            run_routine(State(st.clone()), Path(p.routine_id.clone())).await?;
        }
        _ => {
            let last = st.store.last_message(&p.thread_id)?;
            if last.as_ref().map(|m| m.id.as_str()) != Some(p.message_id.as_str()) {
                return Err(ApiError::bad_request("the chat has moved on since; open it to ask again"));
            }
            let _ = regenerate(State(st.clone()), Path(p.thread_id.clone())).await?;
        }
    }
    st.store.resolve_problems(Some(&id))?;
    st.emit(Event::ProblemsChanged);
    Ok(StatusCode::ACCEPTED)
}

/// Each agent's latest chat line, for the chat list.
async fn chat_previews(State(st): State<AppState>) -> ApiResult<Json<Vec<iris_core::ChatPreview>>> {
    Ok(Json(st.store.chat_previews()?))
}

async fn recent_threads(
    State(st): State<AppState>,
    Query(q): Query<RecentQuery>,
) -> ApiResult<Json<Vec<Thread>>> {
    Ok(Json(st.store.list_recent_threads(q.limit.clamp(1, 200))?))
}

#[derive(Deserialize, Default)]
struct NewThread {
    #[serde(default)]
    title: String,
}

async fn create_thread(
    State(st): State<AppState>,
    Path(agent_id): Path<String>,
    Json(body): Json<NewThread>,
) -> ApiResult<(StatusCode, Json<Thread>)> {
    if st.store.get_agent(&agent_id)?.is_none() {
        return Err(ApiError::not_found("agent"));
    }
    // Agents have one chat; "creating" a thread returns it.
    let _ = body.title;
    let name = st.store.get_agent(&agent_id)?.map(|a| a.name).unwrap_or_default();
    let thread = st.store.chat_thread(&agent_id, &name)?;
    st.emit(Event::ThreadUpdated { thread: thread.clone() });
    Ok((StatusCode::CREATED, Json(thread)))
}

/// An agent's one ongoing chat (created on first use), with its messages.
async fn get_chat(State(st): State<AppState>, Path(agent_id): Path<String>) -> ApiResult<Json<ThreadDetail>> {
    let agent = st.store.get_agent(&agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
    let thread = st.store.chat_thread(&agent.id, &agent.name)?;
    let messages = st.store.list_messages(&thread.id)?;
    let running = st.is_running(&thread.id);
    Ok(Json(ThreadDetail { thread, messages, running }))
}

/// Empties an agent's chat (its memory files stay).
async fn clear_chat(State(st): State<AppState>, Path(agent_id): Path<String>) -> ApiResult<Json<ThreadDetail>> {
    let agent = st.store.get_agent(&agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
    let thread = st.store.chat_thread(&agent.id, &agent.name)?;
    if st.is_running(&thread.id) {
        return Err(ApiError::conflict(format!("{} is working; stop it first", agent.name)));
    }
    let thread = st.store.clear_thread(&thread.id)?.ok_or_else(|| ApiError::not_found("chat"))?;
    st.emit(Event::ThreadUpdated { thread: thread.clone() });
    Ok(Json(ThreadDetail { thread, messages: Vec::new(), running: false }))
}

/// Summarizes the older part of an agent's chat now.
async fn compact_chat(State(st): State<AppState>, Path(agent_id): Path<String>) -> ApiResult<Json<ThreadDetail>> {
    let agent = st.store.get_agent(&agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
    let thread = st.store.chat_thread(&agent.id, &agent.name)?;
    if st.is_running(&thread.id) {
        return Err(ApiError::conflict(format!("{} is working; try again when it's done", agent.name)));
    }
    let (client, model) = crate::compact::client_for(&st, &agent)
        .ok_or_else(|| ApiError::bad_request("no model is set up"))?;
    let done = crate::compact::compact(&st, &agent, &thread.id, &client, &model, true)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_GATEWAY, e))?;
    if !done {
        return Err(ApiError::bad_request("the chat is too short to compact"));
    }
    get_chat(State(st), Path(agent_id)).await
}

// ---- account vault (passwords go in, never come out) -----------------------------

async fn list_vault(State(st): State<AppState>) -> Json<Vec<crate::vault::AccountInfo>> {
    Json(crate::vault::load(&st.data_dir).iter().map(Into::into).collect())
}

#[derive(Deserialize)]
struct AccountInput {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    site: Option<String>,
    #[serde(default)]
    username: Option<String>,
    /// Empty or missing keeps the current password.
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    notes: Option<String>,
}

async fn add_account(
    State(st): State<AppState>,
    Json(a): Json<AccountInput>,
) -> ApiResult<(StatusCode, Json<crate::vault::AccountInfo>)> {
    let name = a.name.unwrap_or_default().trim().to_owned();
    let site = crate::vault::normalize_site(&a.site.unwrap_or_default());
    if name.is_empty() || site.is_empty() {
        return Err(ApiError::bad_request("an account needs a name and a site"));
    }
    let mut all = crate::vault::load(&st.data_dir);
    if all.iter().any(|x| x.name.eq_ignore_ascii_case(&name)) {
        return Err(ApiError::conflict(format!("there's already an account called {name}")));
    }
    let account = crate::vault::Account {
        id: iris_core::new_id(),
        name,
        site,
        username: a.username.unwrap_or_default().trim().to_owned(),
        password: a.password.unwrap_or_default(),
        notes: a.notes.unwrap_or_default(),
        created_at: iris_core::now_ms(),
    };
    let info = (&account).into();
    all.push(account);
    crate::vault::save(&st.data_dir, &all)?;
    Ok((StatusCode::CREATED, Json(info)))
}

async fn edit_account(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(a): Json<AccountInput>,
) -> ApiResult<Json<crate::vault::AccountInfo>> {
    let mut all = crate::vault::load(&st.data_dir);
    let acc = all.iter_mut().find(|x| x.id == id).ok_or_else(|| ApiError::not_found("account"))?;
    if let Some(v) = a.name.filter(|v| !v.trim().is_empty()) {
        acc.name = v.trim().to_owned();
    }
    if let Some(v) = a.site.filter(|v| !v.trim().is_empty()) {
        acc.site = crate::vault::normalize_site(&v);
    }
    if let Some(v) = a.username {
        acc.username = v.trim().to_owned();
    }
    if let Some(v) = a.password.filter(|v| !v.is_empty()) {
        acc.password = v;
    }
    if let Some(v) = a.notes {
        acc.notes = v;
    }
    let info = (&*acc).into();
    crate::vault::save(&st.data_dir, &all)?;
    Ok(Json(info))
}

async fn delete_account(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let mut all = crate::vault::load(&st.data_dir);
    let before = all.len();
    all.retain(|x| x.id != id);
    if all.len() == before {
        return Err(ApiError::not_found("account"));
    }
    crate::vault::save(&st.data_dir, &all)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- routines -------------------------------------------------------------------

async fn list_routines(State(st): State<AppState>) -> ApiResult<Json<Vec<iris_core::Routine>>> {
    Ok(Json(st.store.list_routines()?))
}

#[derive(Deserialize)]
struct NewRoutine {
    agent_id: String,
    name: String,
    schedule: String,
    kind: String,
    body: String,
    #[serde(default)]
    notify: String,
}

async fn create_routine(
    State(st): State<AppState>,
    Json(r): Json<NewRoutine>,
) -> ApiResult<(StatusCode, Json<iris_core::Routine>)> {
    st.store.get_agent(&r.agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
    let routine = crate::routines::create(&st, &r.agent_id, &r.name, &r.schedule, &r.kind, &r.body, &r.notify)
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::CREATED, Json(routine)))
}

#[derive(Deserialize)]
struct RoutinePatch {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    schedule: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    notify: Option<String>,
}

/// Edits a routine. Turning it on or changing its schedule reschedules it
/// from now.
async fn patch_routine(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(p): Json<RoutinePatch>,
) -> ApiResult<Json<iris_core::Routine>> {
    let mut r = st.store.get_routine(&id)?.ok_or_else(|| ApiError::not_found("routine"))?;
    if let Some(agent_id) = p.agent_id {
        st.store.get_agent(&agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
        r.agent_id = agent_id;
    }
    if let Some(name) = p.name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty()) {
        r.name = name.chars().take(60).collect();
    }
    if let Some(kind) = p.kind {
        if !matches!(kind.as_str(), "prompt" | "command" | "heartbeat") {
            return Err(ApiError::bad_request("kind must be prompt, command or heartbeat"));
        }
        r.kind = kind;
    }
    if let Some(body) = p.body.map(|b| b.trim().to_owned()).filter(|b| !b.is_empty()) {
        r.body = body;
    }
    if let Some(notify) = p.notify {
        r.notify = if notify == "always" { "always".into() } else { "failure".into() };
    }
    let rescheduled = p.schedule.is_some() || p.enabled == Some(true);
    if let Some(schedule) = p.schedule {
        r.schedule = schedule.trim().to_lowercase();
    }
    if let Some(enabled) = p.enabled {
        r.enabled = enabled;
    }
    if rescheduled {
        // Checks the schedule even for a paused routine; only a running one gets a next time.
        let next = crate::routines::next_run(&r.schedule, chrono::Local::now()).map_err(ApiError::bad_request)?;
        if r.enabled {
            r.next_run = Some(next.ok_or_else(|| ApiError::bad_request("that time has already passed"))?);
        }
    }
    st.store.save_routine(&r)?;
    st.emit(Event::RoutinesChanged);
    Ok(Json(r))
}

async fn delete_routine(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if !st.store.delete_routine(&id)? {
        return Err(ApiError::not_found("routine"));
    }
    st.emit(Event::RoutinesChanged);
    Ok(StatusCode::NO_CONTENT)
}

/// Runs a routine right away (its schedule carries on as before).
async fn run_routine(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let r = st.store.get_routine(&id)?.ok_or_else(|| ApiError::not_found("routine"))?;
    tokio::spawn(async move { crate::routines::fire(&st, r).await });
    Ok(StatusCode::ACCEPTED)
}

#[derive(Serialize, Deserialize)]
struct MemoryText {
    text: String,
    #[serde(default)]
    limit: usize,
}

async fn get_agent_memory(State(st): State<AppState>, Path(agent_id): Path<String>) -> ApiResult<Json<MemoryText>> {
    st.store.get_agent(&agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
    let text = crate::memory::read(&crate::memory::memory_path(&st.data_dir, &agent_id));
    Ok(Json(MemoryText { text, limit: crate::memory::MEMORY_LIMIT }))
}

async fn put_agent_memory(
    State(st): State<AppState>,
    Path(agent_id): Path<String>,
    Json(body): Json<MemoryText>,
) -> ApiResult<Json<MemoryText>> {
    st.store.get_agent(&agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
    if body.text.trim().chars().count() > crate::memory::MEMORY_LIMIT {
        return Err(ApiError::bad_request(format!("memory is limited to {} characters", crate::memory::MEMORY_LIMIT)));
    }
    crate::memory::write(&crate::memory::memory_path(&st.data_dir, &agent_id), &body.text)?;
    get_agent_memory(State(st), Path(agent_id)).await
}

async fn get_user_memory(State(st): State<AppState>) -> Json<MemoryText> {
    Json(MemoryText { text: crate::memory::read(&crate::memory::user_path(&st.data_dir)), limit: crate::memory::USER_LIMIT })
}

async fn put_user_memory(State(st): State<AppState>, Json(body): Json<MemoryText>) -> ApiResult<Json<MemoryText>> {
    if body.text.trim().chars().count() > crate::memory::USER_LIMIT {
        return Err(ApiError::bad_request(format!("this is limited to {} characters", crate::memory::USER_LIMIT)));
    }
    crate::memory::write(&crate::memory::user_path(&st.data_dir), &body.text)?;
    Ok(get_user_memory(State(st)).await)
}

#[derive(Serialize)]
struct ThreadDetail {
    thread: Thread,
    messages: Vec<Message>,
    /// True while a reply is being generated.
    running: bool,
}

async fn get_thread(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<ThreadDetail>> {
    let thread = st.store.get_thread(&id)?.ok_or_else(|| ApiError::not_found("thread"))?;
    let messages = st.store.list_messages(&id)?;
    let running = st.is_running(&id);
    Ok(Json(ThreadDetail { thread, messages, running }))
}

#[derive(Deserialize)]
struct ThreadPatch {
    title: String,
}

async fn patch_thread(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<ThreadPatch>,
) -> ApiResult<Json<Thread>> {
    let thread =
        st.store.rename_thread(&id, patch.title.trim())?.ok_or_else(|| ApiError::not_found("thread"))?;
    st.emit(Event::ThreadUpdated { thread: thread.clone() });
    Ok(Json(thread))
}

async fn delete_thread(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let thread = st.store.get_thread(&id)?.ok_or_else(|| ApiError::not_found("thread"))?;
    st.cancel_run(&id);
    st.store.delete_thread(&id)?;
    st.emit(Event::ThreadDeleted { thread_id: id, agent_id: thread.agent_id });
    Ok(StatusCode::NO_CONTENT)
}

// ---- messages & runs ------------------------------------------------------------

#[derive(Deserialize)]
struct PostMessage {
    #[serde(default)]
    content: String,
    /// Files uploaded first with `POST /threads/{id}/attachments`.
    #[serde(default)]
    attachments: Vec<iris_core::Attachment>,
}

#[derive(Deserialize)]
struct Upload {
    name: String,
    /// The file, base64-encoded.
    data: String,
}

/// Stores a file the user is about to send; returns what to pass along with
/// the message.
async fn upload_attachment(
    State(st): State<AppState>,
    Path(thread_id): Path<String>,
    Json(body): Json<Upload>,
) -> ApiResult<Json<iris_core::Attachment>> {
    use base64::Engine as _;
    st.store.get_thread(&thread_id)?.ok_or_else(|| ApiError::not_found("thread"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(body.data.trim())
        .map_err(|_| ApiError::bad_request("the file isn't valid base64"))?;
    if bytes.len() > crate::attachments::MAX_BYTES {
        return Err(ApiError::bad_request("files are limited to 20 MB"));
    }
    Ok(Json(crate::attachments::save(&st.data_dir, &thread_id, &body.name, &bytes)?))
}

async fn voice_status(State(st): State<AppState>) -> Json<crate::voice::VoiceStatus> {
    Json(crate::voice::status(&st))
}

#[derive(Deserialize)]
struct VoiceClip {
    /// The recording, base64.
    data: String,
    #[serde(default)]
    mime: String,
    /// e.g. "en" or "el"; empty lets the model detect it.
    #[serde(default)]
    language: String,
}

#[derive(Serialize)]
struct Transcript {
    text: String,
}

/// Speech to text for the composer's microphone.
async fn transcribe(State(st): State<AppState>, Json(clip): Json<VoiceClip>) -> ApiResult<Json<Transcript>> {
    use base64::Engine as _;
    let audio = base64::engine::general_purpose::STANDARD
        .decode(clip.data.trim())
        .map_err(|_| ApiError::bad_request("the recording isn't valid base64"))?;
    if audio.is_empty() || audio.len() > crate::voice::MAX_CLIP_BYTES {
        return Err(ApiError::bad_request("recordings must be between 1 byte and 24 MB"));
    }
    let lang = Some(clip.language.trim()).filter(|l| !l.is_empty());
    let text = crate::voice::transcribe(&st, audio, &clip.mime, lang).await.map_err(ApiError::bad_request)?;
    Ok(Json(Transcript { text }))
}

/// An attached file (for showing images in the app).
async fn get_attachment(
    State(st): State<AppState>,
    Path((thread_id, id)): Path<(String, String)>,
) -> ApiResult<Response> {
    let dir = st.data_dir.join("attachments").join(&thread_id);
    let prefix = format!("{id}-");
    let file = std::fs::read_dir(&dir)
        .ok()
        .and_then(|d| d.flatten().find(|e| e.file_name().to_string_lossy().starts_with(&prefix)))
        .ok_or_else(|| ApiError::not_found("attachment"))?;
    let name = file.file_name().to_string_lossy().into_owned();
    let bytes = std::fs::read(file.path())?;
    Ok(([(axum::http::header::CONTENT_TYPE, crate::attachments::mime_of(&name))], bytes).into_response())
}

#[derive(Serialize)]
struct PostedMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    user_message: Option<Message>,
    /// The reply placeholder; its text arrives as `message_delta` events and
    /// its final state as `message_completed`. None when the message was queued.
    #[serde(skip_serializing_if = "Option::is_none")]
    assistant_message: Option<Message>,
    /// The agent was busy: the message waits in its queue.
    #[serde(skip_serializing_if = "Option::is_none")]
    queued: Option<crate::queue::Queued>,
}

/// Reserves the thread for a new reply; only one reply runs per thread.
pub(crate) fn claim_thread(st: &AppState, thread_id: &str, agent_id: &str) -> ApiResult<CancellationToken> {
    let mut runs = st.runs.lock().unwrap();
    if runs.contains_key(thread_id) {
        return Err(ApiError::conflict("a reply is already being generated in this thread"));
    }
    let cancel = CancellationToken::new();
    runs.insert(
        thread_id.to_owned(),
        Run {
            cancel: cancel.clone(),
            agent_id: agent_id.to_owned(),
            message_id: String::new(),
            activity: "thinking".into(),
            detail: String::new(),
        },
    );
    Ok(cancel)
}

pub(crate) fn release_thread(st: &AppState, thread_id: &str) {
    st.runs.lock().unwrap().remove(thread_id);
}

/// Inserts the reply placeholder and starts generating it in the background.
pub(crate) fn start_reply(st: &AppState, agent: Agent, thread_id: &str, cancel: CancellationToken) -> ApiResult<Message> {
    let model = st.resolve_model(&agent).map(|(_, m)| m);
    let reply = st.store.insert_message(
        thread_id,
        MessageRole::Assistant,
        "",
        MessageStatus::Streaming,
        model.as_deref(),
    );
    let reply = match reply {
        Ok(r) => r,
        Err(e) => {
            release_thread(st, thread_id);
            return Err(e.into());
        }
    };
    if let Some(run) = st.runs.lock().unwrap().get_mut(thread_id) {
        run.message_id = reply.id.clone();
    }
    st.emit(Event::MessageCreated { message: reply.clone() });
    st.emit(Event::RunStarted {
        agent_id: agent.id.clone(),
        thread_id: thread_id.to_owned(),
        message_id: reply.id.clone(),
    });
    if let Ok(Some(thread)) = st.store.touch_thread(thread_id) {
        st.emit(Event::ThreadUpdated { thread });
    }
    tokio::spawn(chat::run(st.clone(), agent, thread_id.to_owned(), reply.id.clone(), cancel));
    Ok(reply)
}

/// Adds the user's message and starts generating the agent's reply in the
/// background. The reply keeps going even if every client disconnects.
async fn post_message(
    State(st): State<AppState>,
    Path(thread_id): Path<String>,
    Json(body): Json<PostMessage>,
) -> ApiResult<(StatusCode, Json<PostedMessage>)> {
    let content = body.content.trim();
    let attachments: Vec<iris_core::Attachment> = body
        .attachments
        .iter()
        .filter_map(|a| crate::attachments::verify(&st.data_dir, &thread_id, a))
        .collect();
    if content.is_empty() && attachments.is_empty() {
        return Err(ApiError::bad_request("content must not be empty"));
    }
    let thread = st.store.get_thread(&thread_id)?.ok_or_else(|| ApiError::not_found("thread"))?;
    let agent =
        st.store.get_agent(&thread.agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
    // Busy (or others are already waiting): the message joins the queue.
    let waiting = st.queues.lock().unwrap().get(&thread_id).is_some_and(|q| !q.is_empty());
    let claimed = if waiting { None } else { claim_thread(&st, &thread_id, &agent.id).ok() };
    let Some(cancel) = claimed else {
        let item = crate::queue::push(&st, &thread_id, content, attachments).map_err(ApiError::conflict)?;
        // Nothing running after all (it just ended): send the queue now.
        if !st.is_running(&thread_id) {
            crate::queue::start_next(&st, &thread_id);
        }
        let posted = PostedMessage { user_message: None, assistant_message: None, queued: Some(item) };
        return Ok((StatusCode::ACCEPTED, Json(posted)));
    };

    let user = st.store.insert_message(
        &thread_id,
        MessageRole::User,
        content,
        MessageStatus::Complete,
        None,
    );
    let mut user = match user {
        Ok(u) => u,
        Err(e) => {
            release_thread(&st, &thread_id);
            return Err(e.into());
        }
    };
    if !attachments.is_empty() {
        st.store.set_attachments(&user.id, &attachments)?;
        user.attachments = attachments;
    }
    st.emit(Event::MessageCreated { message: user.clone() });
    // A blank orb: this first message decides what it becomes. Shape it, then
    // it answers as the new specialist (the thread stays claimed meanwhile, so
    // more messages queue up).
    if agent.role == crate::design::BLANK_ROLE {
        st.set_activity(&thread_id, "designing", "");
        st.emit(Event::RunStarted { agent_id: agent.id.clone(), thread_id: thread_id.clone(), message_id: String::new() });
        let (st2, request, tid) = (st.clone(), content.to_owned(), thread_id.clone());
        tokio::spawn(async move {
            match crate::design::shape(&st2, &agent, &tid, &request).await {
                Ok(shaped) => {
                    if let Ok(Some(thread)) = st2.store.rename_thread(&tid, &shaped.name) {
                        st2.emit(Event::ThreadUpdated { thread });
                    }
                    if crate::api::start_reply(&st2, shaped, &tid, cancel).is_err() {
                        release_thread(&st2, &tid);
                    }
                }
                Err(e) => {
                    release_thread(&st2, &tid);
                    st2.emit(Event::RunFinished { agent_id: agent.id.clone(), thread_id: tid.clone(), message_id: String::new(), status: "error".into() });
                    let text = format!("{} I couldn't set myself up ({e}). Send your request again.", crate::design::SETUP_PREFIX);
                    if let Ok(m) = st2.store.insert_message(&tid, MessageRole::User, &text, MessageStatus::Complete, None) {
                        st2.emit(Event::MessageCreated { message: m });
                    }
                    crate::queue::start_next(&st2, &tid);
                }
            }
        });
        return Ok((StatusCode::ACCEPTED, Json(PostedMessage { user_message: Some(user), assistant_message: None, queued: None })));
    }
    if thread.title.is_empty() {
        if let Ok(Some(thread)) = st.store.rename_thread(&thread_id, &title_from(content)) {
            st.emit(Event::ThreadUpdated { thread });
        }
    }

    let reply = start_reply(&st, agent, &thread_id, cancel)?;
    Ok((StatusCode::ACCEPTED, Json(PostedMessage { user_message: Some(user), assistant_message: Some(reply), queued: None })))
}

/// Replaces the last reply of the thread with a freshly generated one.
async fn regenerate(
    State(st): State<AppState>,
    Path(thread_id): Path<String>,
) -> ApiResult<(StatusCode, Json<PostedMessage>)> {
    let thread = st.store.get_thread(&thread_id)?.ok_or_else(|| ApiError::not_found("thread"))?;
    let agent =
        st.store.get_agent(&thread.agent_id)?.ok_or_else(|| ApiError::not_found("agent"))?;
    let cancel = claim_thread(&st, &thread_id, &agent.id)?;

    let last = match st.store.last_message(&thread_id) {
        Ok(last) => last,
        Err(e) => {
            release_thread(&st, &thread_id);
            return Err(e.into());
        }
    };
    match last {
        Some(m) if m.role == MessageRole::Assistant => {
            if let Err(e) = st.store.delete_message(&m.id) {
                release_thread(&st, &thread_id);
                return Err(e.into());
            }
            st.emit(Event::MessageDeleted { thread_id: thread_id.clone(), message_id: m.id });
        }
        // After a failed request there may be no reply yet: just answer the user.
        Some(m) if m.role == MessageRole::User => {}
        _ => {
            release_thread(&st, &thread_id);
            return Err(ApiError::bad_request("nothing to regenerate"));
        }
    }

    let reply = start_reply(&st, agent, &thread_id, cancel)?;
    Ok((StatusCode::ACCEPTED, Json(PostedMessage { user_message: None, assistant_message: Some(reply), queued: None })))
}

async fn stop_thread(
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    Json(json!({ "stopped": st.cancel_run(&id) }))
}

async fn delete_queued(State(st): State<AppState>, Path((id, item)): Path<(String, String)>) -> ApiResult<StatusCode> {
    if crate::queue::remove(&st, &id, &item) {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("queued message"))
    }
}

/// Sends a queued message next, stopping the reply in progress.
async fn steer_queued(State(st): State<AppState>, Path((id, item)): Path<(String, String)>) -> ApiResult<StatusCode> {
    if crate::queue::steer(&st, &id, &item) {
        Ok(StatusCode::ACCEPTED)
    } else {
        Err(ApiError::not_found("queued message"))
    }
}

/// A short title from the first line of the first message.
fn title_from(content: &str) -> String {
    const MAX: usize = 60;
    let line = content.lines().next().unwrap_or_default().trim();
    if line.chars().count() <= MAX {
        line.to_owned()
    } else {
        let cut: String = line.chars().take(MAX - 1).collect();
        format!("{}…", cut.trim_end())
    }
}

// ---- tools, approvals, audit --------------------------------------------------

/// The toolsets an agent can switch on.
async fn list_tools() -> Json<&'static [Toolset]> {
    Json(TOOLSETS)
}

/// Tool calls currently waiting for the user, oldest first.
async fn list_approvals(State(st): State<AppState>) -> Json<Vec<PendingApproval>> {
    let mut pending: Vec<PendingApproval> =
        st.approvals.lock().unwrap().values().map(|(a, _)| a.clone()).collect();
    pending.sort_by_key(|a| a.created_at);
    Json(pending)
}

#[derive(Deserialize)]
struct Decision {
    approve: bool,
}

async fn decide_approval(
    State(st): State<AppState>,
    Path(id): Path<String>,
    Json(decision): Json<Decision>,
) -> ApiResult<StatusCode> {
    // The waiting reply removes the entry and announces the outcome itself.
    let sender = st.approvals.lock().unwrap().remove(&id).map(|(_, tx)| tx);
    match sender {
        Some(tx) => {
            let _ = tx.send(decision.approve);
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(ApiError::not_found("pending approval")),
    }
}

#[derive(Deserialize)]
struct AuditQuery {
    #[serde(default = "default_audit_limit")]
    limit: u32,
}

fn default_audit_limit() -> u32 {
    100
}

/// Every tool call agents made, newest first.
async fn audit_log(
    State(st): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> ApiResult<Json<Vec<iris_core::ToolLogEntry>>> {
    Ok(Json(st.store.list_tool_log(q.limit.clamp(1, 1000))?))
}

#[derive(Deserialize)]
struct ChatSearchQuery {
    q: String,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

/// Full-text search over every conversation, best matches first.
async fn search_chats(
    State(st): State<AppState>,
    Query(q): Query<ChatSearchQuery>,
) -> ApiResult<Json<Vec<iris_core::SearchHit>>> {
    let agent = q.agent_id.as_deref().filter(|a| !a.is_empty());
    Ok(Json(st.store.search_messages(&q.q, agent, q.limit.unwrap_or(30))?))
}

/// Connected services (MCP servers): config without secret values, state, tools.
async fn list_mcp(State(st): State<AppState>) -> Json<Vec<crate::mcp::ServerStatus>> {
    Json(st.mcp.status(&st.data_dir).await)
}

/// Adds or changes a server, then (re)connects it.
async fn put_mcp(
    State(st): State<AppState>,
    Path(name): Path<String>,
    Json(update): Json<crate::mcp::ServerUpdate>,
) -> ApiResult<Json<Vec<crate::mcp::ServerStatus>>> {
    crate::mcp::put_server(&st.data_dir, &name, update).map_err(ApiError::bad_request)?;
    st.mcp.disconnect(&name).await;
    st.mcp.refresh(&st.data_dir, &st.http).await;
    Ok(list_mcp(State(st)).await)
}

async fn delete_mcp(State(st): State<AppState>, Path(name): Path<String>) -> ApiResult<StatusCode> {
    if !crate::mcp::delete_server(&st.data_dir, &name).map_err(ApiError::bad_request)? {
        return Err(ApiError::not_found("connection"));
    }
    st.mcp.disconnect(&name).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Reconnects every server (after editing mcp.json by hand, or a server crashed).
async fn refresh_mcp(State(st): State<AppState>) -> Json<Vec<crate::mcp::ServerStatus>> {
    for s in st.mcp.status(&st.data_dir).await {
        st.mcp.disconnect(&s.name).await;
    }
    st.mcp.refresh(&st.data_dir, &st.http).await;
    list_mcp(State(st)).await
}

async fn list_skills(State(st): State<AppState>) -> Json<Vec<crate::skills::Skill>> {
    Json(crate::skills::list(&st.data_dir))
}

#[derive(Deserialize)]
struct SkillUpdate {
    description: String,
    body: String,
}

/// Creates or replaces a skill (the name in the path is turned into a slug).
async fn put_skill(
    State(st): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<SkillUpdate>,
) -> ApiResult<Json<crate::skills::Skill>> {
    crate::skills::save(&st.data_dir, &name, &body.description, &body.body).map(Json).map_err(ApiError::bad_request)
}

async fn delete_skill(State(st): State<AppState>, Path(name): Path<String>) -> ApiResult<StatusCode> {
    if crate::skills::delete(&st.data_dir, &name) { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::not_found("skill")) }
}

async fn get_search(State(st): State<AppState>) -> ApiResult<Json<config::SearchStatus>> {
    let cfg = config::load(&st.data_dir)?;
    Ok(Json(config::search_status(&st.data_dir, &cfg)))
}

#[derive(Deserialize)]
struct SearchUpdate {
    provider: String,
    /// Omit to keep the stored key, `""` to remove it.
    #[serde(default)]
    tavily_api_key: Option<String>,
}

async fn put_search(
    State(st): State<AppState>,
    Json(update): Json<SearchUpdate>,
) -> ApiResult<Json<config::SearchStatus>> {
    if !matches!(update.provider.as_str(), "auto" | "tavily" | "duckduckgo" | "deepseek") {
        return Err(ApiError::bad_request("provider must be auto, tavily, duckduckgo or deepseek"));
    }
    config::set_search(&st.data_dir, &update.provider, update.tavily_api_key.as_deref())?;
    let cfg = config::load(&st.data_dir)?;
    *st.search.write().unwrap() = config::search_settings(&st.data_dir, &cfg);
    Ok(Json(config::search_status(&st.data_dir, &cfg)))
}

// ---- devices ------------------------------------------------------------------

#[derive(Serialize)]
struct DeviceInfo {
    #[serde(flatten)]
    device: Device,
    /// The desktop app's own access on this computer (can't be revoked).
    local: bool,
}

async fn list_devices(State(st): State<AppState>) -> ApiResult<Json<Vec<DeviceInfo>>> {
    let local = st.local_device_id.as_deref();
    Ok(Json(
        st.store
            .list_devices()?
            .into_iter()
            .map(|device| DeviceInfo { local: Some(device.id.as_str()) == local, device })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct NewDevice {
    name: String,
}

#[derive(Serialize)]
struct CreatedDevice {
    device: Device,
    /// Shown once; only its hash is stored.
    token: String,
}

async fn create_device(
    State(st): State<AppState>,
    Json(body): Json<NewDevice>,
) -> ApiResult<(StatusCode, Json<CreatedDevice>)> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name must not be empty"));
    }
    let token = auth::new_token();
    let device = st.store.create_device(name, &auth::hash_token(&token))?;
    Ok((StatusCode::CREATED, Json(CreatedDevice { device, token })))
}

async fn delete_device(State(st): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    if st.local_device_id.as_deref() == Some(id.as_str()) {
        return Err(ApiError::conflict(
            "this is the desktop app's own access on this computer and can't be revoked",
        ));
    }
    if st.store.delete_device(&id)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("device"))
    }
}

// ---- remote access (I.R.I.S. Link) --------------------------------------------

#[derive(Serialize)]
struct LinkStatus {
    enabled: bool,
    url: String,
    connected: bool,
    /// The relay's server key is in place (`secrets/relay.key`).
    has_relay_key: bool,
    /// The engine's public key, which phones and browsers pin.
    public_key: String,
    /// The PC login's username, if a password is set.
    login_user: Option<String>,
}

fn link_status(st: &AppState) -> LinkStatus {
    let cfg = st.link.config.lock().unwrap().clone();
    let key = std::fs::read_to_string(st.data_dir.join("secrets").join("relay.key")).unwrap_or_default();
    LinkStatus {
        enabled: cfg.enabled,
        url: cfg.url,
        connected: st.link.connected.load(std::sync::atomic::Ordering::Relaxed),
        has_relay_key: !key.trim().is_empty(),
        public_key: st.link.identity.public.clone(),
        login_user: crate::link::load_login(&st.data_dir).map(|l| l.user),
    }
}

async fn get_link(State(st): State<AppState>) -> Json<LinkStatus> {
    Json(link_status(&st))
}

async fn put_link(State(st): State<AppState>, Json(body): Json<crate::link::LinkConfig>) -> ApiResult<Json<LinkStatus>> {
    let url = body.url.trim().trim_end_matches('/').to_owned();
    if !url.is_empty() && !url.starts_with("https://") {
        return Err(ApiError::bad_request("the relay address must start with https://"));
    }
    let cfg = crate::link::LinkConfig { url, enabled: body.enabled };
    config::set_link(&st.data_dir, &cfg)?;
    *st.link.config.lock().unwrap() = cfg;
    Ok(Json(link_status(&st)))
}

#[derive(Serialize)]
struct Pairing {
    code: String,
    expires_at: i64,
    /// What the QR code holds: the web app's address with the code and the
    /// engine's key in the fragment (never sent to the relay).
    url: String,
}

async fn new_pairing(State(st): State<AppState>) -> ApiResult<Json<Pairing>> {
    let base = st.link.config.lock().unwrap().url.clone();
    if base.is_empty() {
        return Err(ApiError::bad_request("set the relay address first"));
    }
    let (code, expires_at) = st.link.new_pairing_code();
    let url = format!("{base}/#pair={code}&k={}", st.link.identity.public);
    Ok(Json(Pairing { code, expires_at, url }))
}

#[derive(Deserialize)]
struct LinkLogin {
    user: String,
    password: String,
}

async fn put_link_login(State(st): State<AppState>, Json(body): Json<LinkLogin>) -> ApiResult<Json<LinkStatus>> {
    let user = body.user.trim();
    if user.is_empty() || user.len() > 60 {
        return Err(ApiError::bad_request("pick a username (up to 60 characters)"));
    }
    if body.password.chars().count() < 10 {
        return Err(ApiError::bad_request("use a password of at least 10 characters"));
    }
    crate::link::set_login(&st.data_dir, user, &body.password)?;
    Ok(Json(link_status(&st)))
}

async fn delete_link_login(State(st): State<AppState>) -> Json<LinkStatus> {
    crate::link::clear_login(&st.data_dir);
    Json(link_status(&st))
}

// ---- live events ----------------------------------------------------------------

async fn events(State(st): State<AppState>, ws: WebSocketUpgrade) -> Response {
    let rx = st.events.subscribe();
    ws.on_upgrade(move |socket| stream_events(socket, rx))
}

async fn stream_events(mut socket: WebSocket, mut rx: broadcast::Receiver<Event>) {
    let hello = json!({ "type": "hello", "version": env!("CARGO_PKG_VERSION") }).to_string();
    if socket.send(WsMessage::Text(hello.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            event = rx.recv() => {
                let text = match event {
                    Ok(event) => serde_json::to_string(&event).expect("events serialize"),
                    // The client was too slow and missed events; tell it so it
                    // can refetch whatever it is showing.
                    Err(broadcast::error::RecvError::Lagged(missed)) => {
                        warn!("events client lagged, missed {missed}");
                        json!({ "type": "lagged", "missed": missed }).to_string()
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                if socket.send(WsMessage::Text(text.into())).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => match incoming {
                Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => break,
                // Pings are answered automatically; clients have nothing else to say yet.
                Some(Ok(_)) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::title_from;

    #[test]
    fn titles() {
        assert_eq!(title_from("Hello there\nmore"), "Hello there");
        let long = "x".repeat(100);
        let t = title_from(&long);
        assert_eq!(t.chars().count(), 60);
        assert!(t.ends_with('…'));
    }
}
