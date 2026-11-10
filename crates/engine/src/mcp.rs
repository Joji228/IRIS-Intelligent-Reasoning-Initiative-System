//! MCP client: agents use tools from Model Context Protocol servers the user
//! connects (GitHub, Gmail, a database, …). Servers are listed in
//! `secrets/mcp.json` in the format other apps use:
//!
//! ```json
//! { "mcpServers": {
//!     "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"],
//!                 "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "…" } },
//!     "notes":  { "url": "https://example.com/mcp", "headers": { "Authorization": "Bearer …" } }
//! } }
//! ```
//!
//! plus two I.R.I.S. fields per server: `enabled` (default true) and
//! `autoApprove` (tool names that run without asking; `"*"` for all).
//!
//! Each tool is offered to agents with the Connections toolset as
//! `mcp__<server>__<tool>`. Calls ask the user unless the server marks the
//! tool read-only or it's in `autoApprove`. What tools return is treated like
//! web content (it can carry prompt injections).
//!
//! Transports: stdio (a local program, JSON-RPC lines) and Streamable HTTP
//! (JSON or SSE answers). Written by hand: only four methods are needed.

use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use iris_providers::{ToolCall, ToolSpec};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, RwLock, oneshot},
};
use tracing::{info, warn};

use crate::tools::ToolOutcome;

const PROTOCOL_VERSION: &str = "2025-06-18";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(60);
const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RESULT_CHARS: usize = 30_000;
/// Function names are limited to 64 characters by most providers.
const MAX_NAME: usize = 64;

// ---- configuration ---------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_approve: Vec<String>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    #[serde(default)]
    mcp_servers: BTreeMap<String, ServerConfig>,
}

pub fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("secrets").join("mcp.json")
}

pub fn load_config(data_dir: &Path) -> BTreeMap<String, ServerConfig> {
    match std::fs::read_to_string(config_path(data_dir)) {
        Ok(text) => serde_json::from_str::<ConfigFile>(&text)
            .map(|c| c.mcp_servers)
            .unwrap_or_else(|e| {
                warn!("mcp.json can't be read: {e}");
                BTreeMap::new()
            }),
        Err(_) => BTreeMap::new(),
    }
}

fn save_config(data_dir: &Path, servers: &BTreeMap<String, ServerConfig>) -> std::io::Result<()> {
    let path = config_path(data_dir);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = ConfigFile { mcp_servers: servers.clone() };
    std::fs::write(path, serde_json::to_string_pretty(&file).unwrap_or_default())
}

/// Server names become part of function names: letters, digits, `-` and `_`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 24 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn function_name(server: &str, tool: &str) -> String {
    let tool: String = tool.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    format!("mcp__{server}__{tool}").chars().take(MAX_NAME).collect()
}

// ---- what the app sees -----------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub name: String,
    /// "stdio" or "http".
    pub transport: &'static str,
    pub command: String,
    pub args: Vec<String>,
    pub url: String,
    /// Names only: the values (tokens) never leave the engine.
    pub env_keys: Vec<String>,
    pub header_keys: Vec<String>,
    pub enabled: bool,
    pub auto_approve: Vec<String>,
    pub connected: bool,
    pub error: Option<String>,
    pub tools: Vec<ToolInfo>,
}

// ---- the hub ---------------------------------------------------------------------------

/// A connected server and its tools.
struct Live {
    conn: Connection,
    tools: Vec<Tool>,
}

#[derive(Clone)]
struct Tool {
    name: String,
    description: String,
    schema: Value,
    read_only: bool,
}

/// Every configured server: connected ones, and why the others aren't.
#[derive(Default)]
pub struct Hub {
    live: RwLock<HashMap<String, Arc<Live>>>,
    errors: RwLock<HashMap<String, String>>,
    /// Serializes (re)connecting.
    connecting: Mutex<()>,
}

impl Hub {
    /// Connects every enabled server that isn't connected yet (and drops
    /// ones that were removed or turned off).
    pub async fn refresh(&self, data_dir: &Path, http: &reqwest::Client) {
        let _guard = self.connecting.lock().await;
        let config = load_config(data_dir);
        {
            let mut live = self.live.write().await;
            // Dropped: removed, turned off, or a program that exited (it reconnects below).
            live.retain(|name, l| config.get(name).is_some_and(|c| c.enabled) && l.conn.is_alive());
        }
        self.errors.write().await.retain(|name, _| config.get(name).is_some_and(|c| c.enabled));
        for (name, cfg) in config.iter().filter(|(_, c)| c.enabled) {
            if self.live.read().await.contains_key(name) {
                continue;
            }
            match tokio::time::timeout(CONNECT_TIMEOUT, connect(name, cfg, data_dir, http)).await {
                Ok(Ok(live)) => {
                    info!("MCP: connected to {name} ({} tools)", live.tools.len());
                    self.errors.write().await.remove(name);
                    self.live.write().await.insert(name.clone(), Arc::new(live));
                }
                Ok(Err(e)) => {
                    warn!("MCP: {name}: {e}");
                    self.errors.write().await.insert(name.clone(), e);
                }
                Err(_) => {
                    warn!("MCP: {name}: timed out connecting");
                    self.errors.write().await.insert(name.clone(), "timed out connecting".into());
                }
            }
        }
    }

    /// Drops a server's connection (it reconnects on the next refresh).
    pub async fn disconnect(&self, name: &str) {
        self.live.write().await.remove(name);
        self.errors.write().await.remove(name);
    }

    pub async fn status(&self, data_dir: &Path) -> Vec<ServerStatus> {
        let config = load_config(data_dir);
        let live = self.live.read().await;
        let errors = self.errors.read().await;
        config
            .into_iter()
            .map(|(name, c)| {
                let server = live.get(&name);
                ServerStatus {
                    transport: if c.url.is_empty() { "stdio" } else { "http" },
                    command: c.command,
                    args: c.args,
                    url: c.url,
                    env_keys: c.env.into_keys().collect(),
                    header_keys: c.headers.into_keys().collect(),
                    enabled: c.enabled,
                    auto_approve: c.auto_approve,
                    connected: server.is_some_and(|s| s.conn.is_alive()),
                    error: match server {
                        Some(s) if !s.conn.is_alive() => Some("the program stopped; press Reconnect".into()),
                        _ => errors.get(&name).cloned(),
                    },
                    tools: server
                        .map(|s| {
                            s.tools
                                .iter()
                                .map(|t| ToolInfo { name: t.name.clone(), description: t.description.clone(), read_only: t.read_only })
                                .collect()
                        })
                        .unwrap_or_default(),
                    name,
                }
            })
            .collect()
    }

    /// Function definitions for every connected tool.
    pub fn specs(&self) -> Vec<ToolSpec> {
        // try_read: building specs must never wait on a slow connect.
        let Ok(live) = self.live.try_read() else { return Vec::new() };
        let mut specs = Vec::new();
        let mut names: Vec<&String> = live.keys().collect();
        names.sort();
        for server in names {
            for t in &live[server].tools {
                let mut schema = t.schema.clone();
                if !schema.is_object() {
                    schema = json!({ "type": "object", "properties": {} });
                }
                specs.push(ToolSpec {
                    name: function_name(server, &t.name),
                    description: format!("[{server}] {}", crate::tools::truncate(&t.description, 900)),
                    parameters: schema,
                });
            }
        }
        specs
    }

    /// The server and tool behind a function name.
    fn find(&self, function: &str) -> Option<(Arc<Live>, String, Tool)> {
        let live = self.live.try_read().ok()?;
        live.iter().find_map(|(server, l)| {
            l.tools
                .iter()
                .find(|t| function_name(server, &t.name) == function)
                .map(|t| (l.clone(), server.clone(), t.clone()))
        })
    }

    /// Whether calling this function waits for the user.
    pub fn needs_approval(&self, data_dir: &Path, function: &str) -> bool {
        let Some((_, server, tool)) = self.find(function) else { return true };
        if tool.read_only {
            return false;
        }
        let auto = load_config(data_dir).get(&server).map(|c| c.auto_approve.clone()).unwrap_or_default();
        !auto.iter().any(|a| a == "*" || a == &tool.name)
    }

    pub async fn call(&self, call: &ToolCall) -> ToolOutcome {
        let Some((live, server, tool)) = self.find(&call.name) else {
            return ToolOutcome::err("Connection not available", "this connection isn't available right now (Settings → Connected services)");
        };
        let summary = format!("{server}: {}", tool.name);
        let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
            .unwrap_or_else(|_| json!({}));
        let result = tokio::time::timeout(
            CALL_TIMEOUT,
            live.conn.request("tools/call", json!({ "name": tool.name, "arguments": args })),
        )
        .await;
        match result {
            Ok(Ok(v)) => {
                let text = content_text(&v);
                let failed = v["isError"].as_bool().unwrap_or(false);
                let output = crate::tools::truncate(if text.is_empty() { "(no output)" } else { &text }, MAX_RESULT_CHARS);
                ToolOutcome { ok: !failed, summary: if failed { format!("{summary} · failed") } else { summary }, output }
            }
            Ok(Err(e)) => ToolOutcome::err(summary, e),
            Err(_) => ToolOutcome::err(summary, format!("no answer within {} seconds", CALL_TIMEOUT.as_secs())),
        }
    }
}

/// The text parts of a `tools/call` result (other content is described).
fn content_text(result: &Value) -> String {
    let mut parts = Vec::new();
    for c in result["content"].as_array().into_iter().flatten() {
        match c["type"].as_str() {
            Some("text") => parts.push(c["text"].as_str().unwrap_or_default().to_owned()),
            Some("resource") => parts.push(
                c["resource"]["text"].as_str().map(str::to_owned).unwrap_or_else(|| format!("[resource {}]", c["resource"]["uri"])),
            ),
            Some(kind) => parts.push(format!("[{kind} content]")),
            None => {}
        }
    }
    if parts.is_empty() {
        if let Some(s) = result.get("structuredContent") {
            return s.to_string();
        }
    }
    parts.join("\n")
}

// ---- adding and removing servers (from the app) -----------------------------------------

#[derive(Debug, Deserialize)]
pub struct ServerUpdate {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: String,
    /// Replaces the stored values when given; empty values keep the stored one.
    #[serde(default)]
    pub env: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub auto_approve: Vec<String>,
}

/// Saves a server. Secrets left empty keep their stored value.
pub fn put_server(data_dir: &Path, name: &str, u: ServerUpdate) -> Result<(), String> {
    if !valid_name(name) {
        return Err("names use letters, digits, - and _ (at most 24)".into());
    }
    if u.command.trim().is_empty() == u.url.trim().is_empty() {
        return Err("give either a command (a program on the server) or a URL".into());
    }
    if !u.url.is_empty() && !(u.url.starts_with("https://") || u.url.starts_with("http://")) {
        return Err("the URL must start with https://".into());
    }
    let mut all = load_config(data_dir);
    let old = all.get(name).cloned().unwrap_or_default();
    let merge = |new: Option<BTreeMap<String, String>>, old: BTreeMap<String, String>| match new {
        None => old,
        Some(new) => new
            .into_iter()
            .filter(|(k, _)| !k.trim().is_empty())
            .map(|(k, v)| {
                let v = if v.is_empty() { old.get(&k).cloned().unwrap_or_default() } else { v };
                (k.trim().to_owned(), v)
            })
            .collect(),
    };
    all.insert(
        name.to_owned(),
        ServerConfig {
            command: u.command.trim().to_owned(),
            args: u.args,
            env: merge(u.env, old.env),
            url: u.url.trim().to_owned(),
            headers: merge(u.headers, old.headers),
            enabled: u.enabled,
            auto_approve: u.auto_approve,
        },
    );
    save_config(data_dir, &all).map_err(|e| format!("can't save mcp.json: {e}"))
}

pub fn delete_server(data_dir: &Path, name: &str) -> Result<bool, String> {
    let mut all = load_config(data_dir);
    let found = all.remove(name).is_some();
    save_config(data_dir, &all).map_err(|e| format!("can't save mcp.json: {e}"))?;
    Ok(found)
}

// ---- connecting -------------------------------------------------------------------------

async fn connect(name: &str, cfg: &ServerConfig, data_dir: &Path, http: &reqwest::Client) -> Result<Live, String> {
    let conn = if cfg.url.is_empty() {
        Connection::Stdio(StdioConn::spawn(name, cfg, data_dir)?)
    } else {
        Connection::Http(HttpConn { url: cfg.url.clone(), headers: cfg.headers.clone(), http: http.clone(), session: Default::default(), ids: AtomicU64::new(1) })
    };
    conn.request(
        "initialize",
        json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "iris", "version": env!("CARGO_PKG_VERSION") }
        }),
    )
    .await?;
    conn.notify("notifications/initialized").await?;
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..20 {
        let params = match &cursor {
            Some(c) => json!({ "cursor": c }),
            None => json!({}),
        };
        let page = conn.request("tools/list", params).await?;
        for t in page["tools"].as_array().into_iter().flatten() {
            let Some(tool_name) = t["name"].as_str() else { continue };
            tools.push(Tool {
                name: tool_name.to_owned(),
                description: t["description"].as_str().unwrap_or_default().to_owned(),
                schema: t["inputSchema"].clone(),
                read_only: t["annotations"]["readOnlyHint"].as_bool().unwrap_or(false),
            });
        }
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    Ok(Live { conn, tools })
}

enum Connection {
    Stdio(StdioConn),
    Http(HttpConn),
}

impl Connection {
    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        match self {
            Connection::Stdio(s) => s.request(method, params).await,
            Connection::Http(h) => h.request(method, params).await,
        }
    }

    async fn notify(&self, method: &str) -> Result<(), String> {
        let msg = json!({ "jsonrpc": "2.0", "method": method });
        match self {
            Connection::Stdio(s) => s.send(&msg).await,
            Connection::Http(h) => h.post(&msg).await.map(|_| ()),
        }
    }

    /// False for a local program that has exited.
    fn is_alive(&self) -> bool {
        match self {
            Connection::Stdio(s) => s.alive.load(Ordering::Relaxed),
            Connection::Http(_) => true,
        }
    }
}

/// Picks the result (or error) out of a JSON-RPC response.
fn rpc_result(v: Value) -> Result<Value, String> {
    if let Some(e) = v.get("error") {
        return Err(format!("the server answered with an error: {}", e["message"].as_str().unwrap_or(&e.to_string())));
    }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

// ---- stdio ---------------------------------------------------------------------------------

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>>;

struct StdioConn {
    stdin: Mutex<tokio::process::ChildStdin>,
    pending: Pending,
    ids: AtomicU64,
    /// False once the program has exited (its output closed).
    alive: Arc<AtomicBool>,
    // Kept so the process lives (and is killed) with the connection.
    _child: tokio::process::Child,
}

impl StdioConn {
    fn spawn(name: &str, cfg: &ServerConfig, data_dir: &Path) -> Result<Self, String> {
        let workdir = data_dir.join("mcp").join(name);
        let _ = std::fs::create_dir_all(&workdir);
        // On Windows, `npx` and friends are .cmd scripts that need cmd.exe.
        #[cfg(windows)]
        let mut cmd = {
            let plain_exe = cfg.command.to_ascii_lowercase().ends_with(".exe");
            let mut c = if plain_exe {
                tokio::process::Command::new(&cfg.command)
            } else {
                let mut c = tokio::process::Command::new("cmd.exe");
                c.arg("/D").arg("/C").arg(&cfg.command);
                c
            };
            c.args(&cfg.args);
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            c.creation_flags(CREATE_NO_WINDOW);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = tokio::process::Command::new(&cfg.command);
            c.args(&cfg.args);
            c
        };
        cmd.envs(&cfg.env)
            .current_dir(&workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("couldn't start `{}`: {e}", cfg.command))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let pending: Pending = Arc::default();
        // Answers are routed to whoever waits for that id; anything else
        // (logs, notifications) is ignored.
        let routes = pending.clone();
        let server = name.to_owned();
        let alive = Arc::new(AtomicBool::new(true));
        let alive_flag = alive.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if let Some(id) = v["id"].as_u64() {
                    if let Some(tx) = routes.lock().unwrap().remove(&id) {
                        let _ = tx.send(v);
                    }
                }
            }
            info!("MCP: {server} closed its output");
            alive_flag.store(false, Ordering::Relaxed);
            routes.lock().unwrap().clear(); // waiting callers get an error
        });
        if let Some(stderr) = child.stderr.take() {
            let server = name.to_owned();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    tracing::debug!("MCP {server}: {line}");
                }
            });
        }
        Ok(Self { stdin: Mutex::new(stdin), pending, ids: AtomicU64::new(1), alive, _child: child })
    }

    async fn send(&self, msg: &Value) -> Result<(), String> {
        let mut line = msg.to_string();
        line.push('\n');
        let mut stdin = self.stdin.lock().await;
        stdin.write_all(line.as_bytes()).await.map_err(|e| format!("the server stopped: {e}"))?;
        stdin.flush().await.map_err(|e| format!("the server stopped: {e}"))
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        if !self.alive.load(Ordering::Relaxed) {
            return Err("the server program has stopped; reconnect it in Settings → Connected services".into());
        }
        let id = self.ids.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).await?;
        let answer = rx.await.map_err(|_| "the server stopped before answering".to_owned())?;
        rpc_result(answer)
    }
}

// ---- Streamable HTTP ---------------------------------------------------------------------

struct HttpConn {
    url: String,
    headers: BTreeMap<String, String>,
    http: reqwest::Client,
    session: std::sync::Mutex<Option<String>>,
    ids: AtomicU64,
}

impl HttpConn {
    /// Posts one message; returns the response's JSON-RPC messages.
    async fn post(&self, msg: &Value) -> Result<Vec<Value>, String> {
        let mut req = self
            .http
            .post(&self.url)
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", PROTOCOL_VERSION)
            .json(msg)
            .timeout(CALL_TIMEOUT);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        if let Some(s) = self.session.lock().unwrap().clone() {
            req = req.header("Mcp-Session-Id", s);
        }
        let resp = req.send().await.map_err(|e| format!("can't reach the server: {e}"))?;
        if let Some(s) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *self.session.lock().unwrap() = Some(s.to_owned());
        }
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("HTTP {status}: {}", crate::tools::truncate(body.trim(), 300)));
        }
        let sse = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|t| t.starts_with("text/event-stream"));
        let body = resp.text().await.map_err(|e| format!("reading the answer failed: {e}"))?;
        if body.trim().is_empty() {
            return Ok(Vec::new()); // 202 for notifications
        }
        Ok(if sse { sse_messages(&body) } else { serde_json::from_str::<Value>(&body).into_iter().collect() })
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.ids.fetch_add(1, Ordering::Relaxed);
        let messages = self.post(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })).await?;
        let answer = messages
            .into_iter()
            .flat_map(|m| if let Value::Array(batch) = m { batch } else { vec![m] })
            .find(|m| m["id"].as_u64() == Some(id))
            .ok_or("the server's answer had no result")?;
        rpc_result(answer)
    }
}

/// The JSON messages in an SSE body (`data:` lines, events split by blank lines).
fn sse_messages(body: &str) -> Vec<Value> {
    body.replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|event| {
            let data: Vec<&str> = event.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim_start).collect();
            (!data.is_empty()).then(|| serde_json::from_str(&data.join("\n")).ok()).flatten()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_parsing() {
        assert!(valid_name("github") && valid_name("my_db-2"));
        assert!(!valid_name("bad name") && !valid_name("") && !valid_name(&"x".repeat(25)));
        assert_eq!(function_name("gh", "search.issues"), "mcp__gh__search_issues");
        assert_eq!(function_name("gh", &"a".repeat(100)).len(), MAX_NAME);

        let msgs = sse_messages("event: message\r\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\r\n\r\n: ping\n\n");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["id"], 1);

        let r = json!({ "content": [{ "type": "text", "text": "a" }, { "type": "image", "data": "…" }] });
        assert_eq!(content_text(&r), "a\n[image content]");
        assert!(rpc_result(json!({ "error": { "message": "nope" } })).unwrap_err().contains("nope"));
    }

    #[test]
    fn config_keeps_secrets_when_left_empty() {
        let dir = std::env::temp_dir().join(format!("iris-mcp-{}", uuid::Uuid::new_v4().simple()));
        let update = |token: &str| ServerUpdate {
            command: "npx".into(),
            args: vec!["-y".into(), "server-github".into()],
            url: String::new(),
            env: Some(BTreeMap::from([("GITHUB_TOKEN".to_owned(), token.to_owned())])),
            headers: None,
            enabled: true,
            auto_approve: vec![],
        };
        put_server(&dir, "github", update("secret1")).unwrap();
        put_server(&dir, "github", update("")).unwrap();
        assert_eq!(load_config(&dir)["github"].env["GITHUB_TOKEN"], "secret1");
        assert!(put_server(&dir, "bad name", update("x")).is_err());
        let both = ServerUpdate { url: "https://x.example/mcp".into(), ..update("x") };
        assert!(put_server(&dir, "x", both).is_err(), "command or URL, not both");
        assert!(delete_server(&dir, "github").unwrap());
        assert!(load_config(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A tiny MCP server over stdio, written in PowerShell (or sh), answers
    /// initialize, tools/list and tools/call.
    #[cfg(windows)]
    #[tokio::test]
    async fn stdio_server_end_to_end() {
        let dir = std::env::temp_dir().join(format!("iris-mcp-e2e-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("server.ps1");
        std::fs::write(
            &script,
            r#"
while ($null -ne ($line = [Console]::In.ReadLine())) {
  $m = $line | ConvertFrom-Json
  if ($null -eq $m.id) { continue }
  switch ($m.method) {
    'initialize' { $r = @{ protocolVersion = '2025-06-18'; capabilities = @{ tools = @{} }; serverInfo = @{ name = 'test'; version = '1' } } }
    'tools/list' { $r = @{ tools = @(@{ name = 'echo'; description = 'Echoes text'; inputSchema = @{ type = 'object'; properties = @{ text = @{ type = 'string' } } }; annotations = @{ readOnlyHint = $true } }) } }
    'tools/call' { $r = @{ content = @(@{ type = 'text'; text = 'echo: ' + $m.params.arguments.text }) } }
  }
  [Console]::Out.WriteLine((@{ jsonrpc = '2.0'; id = $m.id; result = $r } | ConvertTo-Json -Depth 10 -Compress))
  [Console]::Out.Flush()
}
"#,
        )
        .unwrap();
        let data = dir.join("data");
        put_server(
            &data,
            "test",
            ServerUpdate {
                command: "powershell.exe".into(),
                args: vec!["-NoProfile".into(), "-ExecutionPolicy".into(), "Bypass".into(), "-File".into(), script.display().to_string()],
                url: String::new(),
                env: None,
                headers: None,
                enabled: true,
                auto_approve: vec![],
            },
        )
        .unwrap();
        let hub = Hub::default();
        hub.refresh(&data, &reqwest::Client::new()).await;
        let status = hub.status(&data).await;
        assert!(status[0].connected, "{:?}", status[0].error);
        assert_eq!(status[0].tools[0].name, "echo");
        assert_eq!(hub.specs()[0].name, "mcp__test__echo");
        assert!(!hub.needs_approval(&data, "mcp__test__echo"), "read-only tools run without asking");
        let call = ToolCall { id: "1".into(), name: "mcp__test__echo".into(), arguments: r#"{"text":"hi"}"#.into() };
        let out = hub.call(&call).await;
        assert!(out.ok && out.output == "echo: hi", "{}", out.output);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
