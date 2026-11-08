//! Browser tools: agents drive the server's Edge (headless) over the Chrome
//! DevTools Protocol. Pages are read as text with numbered buttons, links and
//! fields, so a text model can click and type by number. One browser serves
//! every agent (each gets its own tab, at most a few), with one profile so
//! logins persist; it closes after a few idle minutes to give the RAM back.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, LazyLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use iris_providers::ToolCall;
use regex::Regex;
use serde_json::{Value, json};
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tracing::{info, warn};

use crate::{api::AppState, tools::ToolOutcome};

const PORT: u16 = 9333;
const MAX_TABS: usize = 3;
const IDLE_CLOSE: Duration = Duration::from_secs(5 * 60);
const MAX_TEXT: usize = 6_000;
const MAX_ELEMENTS: usize = 120;

/// Clicking something with a label like this spends money or commits to
/// something: it always waits for the user, even in Full auto.
static COMMIT_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(pay|buy|purchase|checkout|check out|place order|order now|confirm order|complete order|subscribe|donate|send money|transfer)\b|πληρωμ|αγορά|ολοκλήρωση|υποβολή παραγγελίας|παραγγελία",
    )
    .expect("valid")
});

/// Clicking (or pressing Enter on) something like this sends, posts or
/// deletes something in the user's name: it asks unless the agent is in Full
/// auto. Unlabeled submit buttons count, as "submit form".
static SEND_WORDS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(send|post|publish|reply|comment|tweet|share|submit|sign up|register|delete|remove|unsubscribe|invite|follow|book|reserve)\b|αποστολή|δημοσίευση|διαγραφή|κράτηση",
    )
    .expect("valid")
});

/// What pressing Enter in a field does, from the page listing: "search" for
/// search boxes, otherwise the label of what it submits.
const SEARCH: &str = "search";

/// The shared browser and each agent's tab.
#[derive(Default)]
pub struct Browser {
    inner: Mutex<Option<Session>>,
}

struct Session {
    process: tokio::process::Child,
    cdp: Arc<Cdp>,
    /// agent id → (target id, CDP session id, labels of the numbered elements).
    tabs: HashMap<String, Tab>,
    last_used: Instant,
}

struct Tab {
    target: String,
    session: String,
    labels: HashMap<u32, String>,
    /// For fields: what pressing Enter in them submits (see [`SEARCH`]).
    submits: HashMap<u32, String>,
}

/// A tiny CDP client: one websocket, requests matched to answers by id.
struct Cdp {
    tx: Mutex<futures_util::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, WsMessage>>,
    pending: Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    next: AtomicU64,
    /// Recent answers to "does this host point at this computer or the LAN?",
    /// so a page with fifty images from one host costs one lookup.
    hosts: std::sync::Mutex<HashMap<String, (bool, Instant)>>,
}

impl Cdp {
    async fn connect(url: &str) -> anyhow::Result<Arc<Self>> {
        let (ws, _) = tokio_tungstenite::connect_async(url).await?;
        let (tx, mut rx) = ws.split();
        let pending: Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Value>>>> = Arc::default();
        let waiting = pending.clone();
        // Requests the browser has paused, waiting for our verdict.
        let (paused_tx, mut paused_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = rx.next().await {
                let Ok(text) = msg.to_text() else { continue };
                let Ok(v) = serde_json::from_str::<Value>(text) else { continue };
                if let Some(id) = v["id"].as_u64() {
                    if let Some(tx) = waiting.lock().unwrap().remove(&id) {
                        let _ = tx.send(v);
                    }
                } else if v["method"] == "Fetch.requestPaused" {
                    let _ = paused_tx.send(v);
                }
            }
        });
        let cdp = Arc::new(Self { tx: Mutex::new(tx), pending, next: AtomicU64::new(1), hosts: Default::default() });
        // Every request a page makes (the page itself, redirects, frames,
        // images, scripts, fetches) waits here until it has been judged.
        let judge = cdp.clone();
        tokio::spawn(async move {
            while let Some(event) = paused_rx.recv().await {
                let judge = judge.clone();
                tokio::spawn(async move { judge.judge(event).await });
            }
        });
        Ok(cdp)
    }

    /// Lets a paused request go on, or fails it if it would reach this
    /// computer or the local network.
    async fn judge(&self, event: Value) {
        let session = event["sessionId"].as_str();
        let (Some(request), Some(url)) = (event["params"]["requestId"].as_str(), event["params"]["request"]["url"].as_str()) else {
            return;
        };
        let allowed = match quick_verdict(url) {
            Some(verdict) => verdict,
            None => !self.points_local(url).await,
        };
        let _ = if allowed {
            self.call(session, "Fetch.continueRequest", json!({ "requestId": request })).await
        } else {
            warn!("the browser was stopped from requesting a local address: {}", trim(url, 80));
            self.call(session, "Fetch.failRequest", json!({ "requestId": request, "errorReason": "AccessDenied" })).await
        };
    }

    /// Whether the URL's host resolves to this computer or the LAN (cached for a minute).
    async fn points_local(&self, url: &str) -> bool {
        let Some(host) = url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_ascii_lowercase)) else { return true };
        if let Some((local, at)) = self.hosts.lock().unwrap().get(&host) {
            if at.elapsed() < Duration::from_secs(60) {
                return *local;
            }
        }
        let local = blocked_after_dns(url).await;
        let mut hosts = self.hosts.lock().unwrap();
        if hosts.len() > 500 {
            hosts.clear();
        }
        hosts.insert(host, (local, Instant::now()));
        local
    }

    async fn call(&self, session: Option<&str>, method: &str, params: Value) -> anyhow::Result<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut msg = json!({ "id": id, "method": method, "params": params });
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.tx.lock().await.send(WsMessage::text(msg.to_string())).await?;
        let v = tokio::time::timeout(Duration::from_secs(30), rx).await??;
        if let Some(e) = v.get("error") {
            anyhow::bail!("{}", e["message"].as_str().unwrap_or("browser error"));
        }
        Ok(v["result"].clone())
    }
}

fn edge_path() -> Option<PathBuf> {
    [r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe", r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"]
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.exists())
}

impl Browser {
    /// Starts Edge if needed and returns the agent's tab.
    async fn tab(&self, st: &AppState, agent_id: &str) -> anyhow::Result<(Arc<Cdp>, String)> {
        let mut guard = self.inner.lock().await;
        let alive = match guard.as_mut() {
            Some(s) => s.process.try_wait().map(|e| e.is_none()).unwrap_or(false),
            None => false,
        };
        if !alive {
            *guard = Some(launch(st).await?);
        }
        let s = guard.as_mut().expect("just launched");
        s.last_used = Instant::now();
        if let Some(t) = s.tabs.get(agent_id) {
            return Ok((s.cdp.clone(), t.session.clone()));
        }
        if s.tabs.len() >= MAX_TABS {
            anyhow::bail!("the browser already has {MAX_TABS} tabs open (one per agent); try again when another agent is done");
        }
        let target = s.cdp.call(None, "Target.createTarget", json!({ "url": "about:blank" })).await?;
        let target = target["targetId"].as_str().unwrap_or_default().to_owned();
        let attached = s.cdp.call(None, "Target.attachToTarget", json!({ "targetId": target, "flatten": true })).await?;
        let session = attached["sessionId"].as_str().unwrap_or_default().to_owned();
        s.cdp.call(Some(&session), "Page.enable", json!({})).await?;
        // Pause every request of this tab until `Cdp::judge` has looked at it.
        s.cdp.call(Some(&session), "Fetch.enable", json!({ "patterns": [{ "urlPattern": "*" }] })).await?;
        s.tabs.insert(agent_id.to_owned(), Tab { target, session: session.clone(), labels: HashMap::new(), submits: HashMap::new() });
        Ok((s.cdp.clone(), session))
    }

    async fn set_labels(&self, agent_id: &str, labels: HashMap<u32, String>, submits: HashMap<u32, String>) {
        if let Some(t) = self.inner.lock().await.as_mut().and_then(|s| s.tabs.get_mut(agent_id)) {
            t.labels = labels;
            t.submits = submits;
        }
    }

    /// What pressing Enter in field `n` submits, if known.
    async fn submit_target(&self, agent_id: &str, n: u32) -> Option<String> {
        self.inner.lock().await.as_ref()?.tabs.get(agent_id)?.submits.get(&n).cloned()
    }

    /// The label of numbered element `n` in the agent's tab, if known.
    pub async fn label(&self, agent_id: &str, n: u32) -> Option<String> {
        self.inner.lock().await.as_ref()?.tabs.get(agent_id)?.labels.get(&n).cloned()
    }

    /// Closes the agent's tab; the last tab closing closes the browser.
    async fn close_tab(&self, agent_id: &str) {
        let mut guard = self.inner.lock().await;
        if let Some(s) = guard.as_mut() {
            if let Some(t) = s.tabs.remove(agent_id) {
                let _ = s.cdp.call(None, "Target.closeTarget", json!({ "targetId": t.target })).await;
            }
            if s.tabs.is_empty() {
                if let Some(mut s) = guard.take() {
                    let _ = s.process.kill().await;
                    info!("closed the browser (no tabs left)");
                }
            }
        }
    }

    /// Closes the browser after a few idle minutes (called periodically).
    pub async fn close_if_idle(&self) {
        let mut guard = self.inner.lock().await;
        if guard.as_ref().is_some_and(|s| s.last_used.elapsed() > IDLE_CLOSE) {
            if let Some(mut s) = guard.take() {
                let _ = s.process.kill().await;
                info!("closed the idle browser");
            }
        }
    }
}

async fn launch(st: &AppState) -> anyhow::Result<Session> {
    let edge = edge_path().ok_or_else(|| anyhow::anyhow!("Microsoft Edge isn't installed"))?;
    let profile = st.data_dir.join("browser");
    std::fs::create_dir_all(&profile)?;
    let mut cmd = tokio::process::Command::new(edge);
    cmd.args([
        "--headless=new",
        &format!("--remote-debugging-port={PORT}"),
        &format!("--user-data-dir={}", profile.display()),
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-extensions",
        "--disable-background-networking",
        // Keep the memory down on an 8 GB server.
        "--renderer-process-limit=2",
        "--disable-gpu",
        "--disable-component-update",
        "--disable-features=Translate,OptimizationHints,MediaRouter,BackForwardCache,EdgeCollections,msEdgeShopping",
        "--js-flags=--max-old-space-size=256",
        "--window-size=1280,900",
        "--lang=en-US",
        "about:blank",
    ])
    .kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000);
    let process = cmd.spawn()?;
    // Wait for the DevTools endpoint.
    let mut ws = None;
    for _ in 0..40 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        if let Ok(r) = st.http.get(format!("http://127.0.0.1:{PORT}/json/version")).send().await {
            if let Ok(v) = r.json::<Value>().await {
                ws = v["webSocketDebuggerUrl"].as_str().map(str::to_owned);
                break;
            }
        }
    }
    let ws = ws.ok_or_else(|| anyhow::anyhow!("the browser didn't start"))?;
    let cdp = Cdp::connect(&ws).await?;
    info!("started the browser");
    Ok(Session { process, cdp, tabs: HashMap::new(), last_used: Instant::now() })
}

/// Pages agents may not open: this computer and the local network (the
/// engine's own API among them).
fn blocked(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else { return true };
    if !matches!(u.scheme(), "http" | "https") {
        return true;
    }
    match u.host() {
        Some(url::Host::Domain(d)) => {
            let d = d.to_ascii_lowercase();
            d == "localhost" || d.ends_with(".localhost") || d.ends_with(".local") || d.ends_with(".internal")
        }
        Some(url::Host::Ipv4(ip)) => local_ip(ip.into()),
        Some(url::Host::Ipv6(ip)) => local_ip(ip.into()),
        None => true,
    }
}

fn local_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || v4.is_unspecified() || v4.octets()[0] == 0
                // Carrier-grade NAT, which Tailscale uses for its addresses.
                || (v4.octets()[0] == 100 && (64..128).contains(&v4.octets()[1]))
        }
        std::net::IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => local_ip(v4.into()),
            // Loopback, unspecified, unique local (fc00::/7), link-local (fe80::/10).
            None => v6.is_loopback() || v6.is_unspecified() || (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80,
        },
    }
}

/// The quick part of judging a browser request: `Some(allowed)` when the URL
/// alone settles it, `None` when the host name still has to be looked up.
fn quick_verdict(url: &str) -> Option<bool> {
    let scheme = url.split(':').next().unwrap_or_default().to_ascii_lowercase();
    match scheme.as_str() {
        // Content that is part of the page itself.
        "data" | "blob" | "about" => Some(true),
        "http" | "https" => blocked(url).then_some(false),
        // file:, ftp:, chrome:, extensions…: never.
        _ => Some(false),
    }
}

/// Like [`blocked`], but also looks the name up, so a public-looking name
/// that points at this computer or the LAN is refused too.
async fn blocked_after_dns(url: &str) -> bool {
    if blocked(url) {
        return true;
    }
    let Some(host) = url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned)) else { return true };
    match tokio::net::lookup_host((host.trim_matches(['[', ']']), 0)).await {
        Ok(addrs) => addrs.into_iter().any(|a| local_ip(a.ip())),
        // Let the browser report names that don't resolve.
        Err(_) => false,
    }
}

/// Whether this call must wait for the user: clicking something that looks
/// like paying or ordering (whatever the auto mode), or signing in with a
/// saved account (unless Full auto).
pub async fn needs_approval(st: &AppState, agent_id: &str, call: &ToolCall, mode: crate::api::AutoMode) -> bool {
    let full = mode == crate::api::AutoMode::Full;
    if call.name == "browser_login" {
        return !full;
    }
    let args = serde_json::from_str::<Value>(&call.arguments).unwrap_or(Value::Null);
    let n = args["n"].as_u64().unwrap_or(0) as u32;
    match call.name.as_str() {
        "browser_click" => st.browser.label(agent_id, n).await.is_some_and(|label| risky_label(&label, full)),
        // Enter submits the field's form: the same as clicking its button.
        "browser_type" if args["submit"].as_bool().unwrap_or(false) => {
            match st.browser.submit_target(agent_id, n).await.as_deref() {
                Some(SEARCH) => false,
                Some(target) => risky_label(target, full),
                // A field we haven't seen: only Full auto goes ahead.
                None => !full,
            }
        }
        _ => false,
    }
}

/// Whether acting on something labeled like this needs the user.
fn risky_label(label: &str, full: bool) -> bool {
    COMMIT_WORDS.is_match(label) || (!full && SEND_WORDS.is_match(label))
}

pub async fn run(st: &AppState, agent_id: &str, call: &ToolCall) -> ToolOutcome {
    let args: Value = serde_json::from_str(if call.arguments.trim().is_empty() { "{}" } else { &call.arguments })
        .unwrap_or(Value::Null);
    match act(st, agent_id, call.name.as_str(), &args).await {
        Ok((summary, output)) => ToolOutcome { ok: true, output, summary },
        Err(e) => ToolOutcome {
            ok: false,
            output: format!("Error: {e:#}"),
            summary: format!("Browser: {}", e.to_string().chars().take(80).collect::<String>()),
        },
    }
}

async fn act(st: &AppState, agent: &str, name: &str, args: &Value) -> anyhow::Result<(String, String)> {
    if name == "browser_close" {
        st.browser.close_tab(agent).await;
        return Ok(("Closed the browser tab".into(), "Closed.".into()));
    }
    let (cdp, s) = st.browser.tab(st, agent).await?;
    let s = Some(s.as_str());
    let n = args["n"].as_u64().unwrap_or(0);
    let summary = match name {
        "browser_open" => {
            let mut url = args["url"].as_str().unwrap_or_default().trim().to_owned();
            if !url.contains("://") {
                url = format!("https://{url}");
            }
            if blocked_after_dns(&url).await {
                anyhow::bail!("this address isn't allowed (only public http/https pages)");
            }
            cdp.call(s, "Page.navigate", json!({ "url": url })).await?;
            format!("Opened {}", url.trim_start_matches("https://").trim_start_matches("http://"))
        }
        "browser_click" => {
            let r = eval(&cdp, s, &format!(
                "(() => {{ const el = document.querySelector('[data-iris-n=\"{n}\"]'); if (!el) return null; \
                 el.scrollIntoView({{block: 'center'}}); const r = el.getBoundingClientRect(); \
                 return {{x: r.left + r.width / 2, y: r.top + r.height / 2}}; }})()"
            )).await?;
            let (Some(x), Some(y)) = (r["x"].as_f64(), r["y"].as_f64()) else {
                anyhow::bail!("there's no element [{n}] on the page now; read the page again");
            };
            for kind in ["mousePressed", "mouseReleased"] {
                cdp.call(s, "Input.dispatchMouseEvent", json!({ "type": kind, "x": x, "y": y, "button": "left", "clickCount": 1 })).await?;
            }
            format!("Clicked [{n}]{}", st.browser.label(agent, n as u32).await.map(|l| format!(" “{}”", trim(&l, 40))).unwrap_or_default())
        }
        "browser_type" => {
            let text = args["text"].as_str().unwrap_or_default();
            let ok = eval(&cdp, s, &format!(
                "(() => {{ const el = document.querySelector('[data-iris-n=\"{n}\"]'); if (!el) return false; \
                 if (el.type === 'password') return 'password'; el.scrollIntoView({{block: 'center'}}); el.focus(); \
                 if ('value' in el) {{ el.value = ''; el.dispatchEvent(new Event('input', {{bubbles: true}})); }} return true; }})()"
            )).await?;
            if ok == json!("password") {
                anyhow::bail!("agents don't type passwords; the account vault fills logins (or the user does it)");
            }
            if ok != json!(true) {
                anyhow::bail!("there's no field [{n}] on the page now; read the page again");
            }
            cdp.call(s, "Input.insertText", json!({ "text": text })).await?;
            if args["submit"].as_bool().unwrap_or(false) {
                for kind in ["keyDown", "keyUp"] {
                    cdp.call(s, "Input.dispatchKeyEvent", json!({ "type": kind, "key": "Enter", "code": "Enter", "windowsVirtualKeyCode": 13, "text": "\r" })).await?;
                }
            }
            format!("Typed into [{n}]")
        }
        "browser_scroll" => {
            let dy = if args["direction"].as_str() == Some("up") { -700 } else { 700 };
            eval(&cdp, s, &format!("window.scrollBy(0, {dy})")).await?;
            "Scrolled the page".into()
        }
        "browser_back" => {
            eval(&cdp, s, "history.back()").await?;
            "Went back".into()
        }
        "browser_read" => "Read the page".into(),
        "browser_login" => {
            let wanted = args["account"].as_str().unwrap_or_default().trim().to_owned();
            let account = crate::vault::load(&st.data_dir)
                .into_iter()
                .find(|a| a.name.eq_ignore_ascii_case(&wanted) || a.id == wanted)
                .ok_or_else(|| anyhow::anyhow!("there's no saved account called {wanted}; see list_accounts"))?;
            let place = eval(&cdp, s, "[location.protocol, location.hostname].join('|')").await?.as_str().unwrap_or_default().to_owned();
            let (protocol, host) = place.split_once('|').unwrap_or_default();
            // Only ever on the account's own site, so a look-alike page can't collect it...
            if !crate::vault::host_matches(host, &account.site) {
                anyhow::bail!("this page is on {host}, but {} is saved for {}; open {} first", account.name, account.site, account.site);
            }
            // ...and only over HTTPS, so a page that was sent in plain text can't read it.
            if !crate::vault::login_allowed(protocol, host, &account.site) {
                anyhow::bail!("this page isn't secure (it doesn't use https), so the saved login for {} isn't filled in", account.name);
            }
            let js = LOGIN_JS
                .replace("__IRIS_USER__", &serde_json::to_string(&account.username)?)
                .replace("__IRIS_PASS__", &serde_json::to_string(&account.password)?);
            let filled = eval(&cdp, s, &js).await?;
            let filled = filled.as_str().unwrap_or("nothing").to_owned();
            if filled == "nothing" {
                anyhow::bail!("no login fields on this page; open the sign-in page first");
            }
            format!("Filled the {} login ({filled}); now click the sign-in button", account.name)
        }
        other => anyhow::bail!("no browser tool called {other}"),
    };
    // Let the page settle, then read it.
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(400)).await;
        if eval(&cdp, s, "document.readyState").await.ok() == Some(json!("complete")) {
            break;
        }
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    // A page can redirect or link anywhere: if it ended up on this computer
    // or the LAN (the engine, the browser's own control port), leave it unread.
    let href = eval(&cdp, s, "location.href").await?.as_str().unwrap_or_default().to_owned();
    if href.starts_with("http") && blocked_after_dns(&href).await {
        let _ = cdp.call(s, "Page.navigate", json!({ "url": "about:blank" })).await;
        anyhow::bail!("the page went to a local address ({}), which agents can't open", trim(&href, 80));
    }
    let snap = eval(&cdp, s, &snapshot_js()).await?;
    let labels: HashMap<u32, String> = snap["labels"]
        .as_object()
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_owned()))).collect())
        .unwrap_or_default();
    let submits: HashMap<u32, String> = snap["submits"]
        .as_object()
        .map(|m| m.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_str()?.to_owned()))).collect())
        .unwrap_or_default();
    st.browser.set_labels(agent, labels, submits).await;
    let page = snap["text"].as_str().unwrap_or_default().to_owned();
    Ok((summary, page))
}

async fn eval(cdp: &Cdp, session: Option<&str>, expr: &str) -> anyhow::Result<Value> {
    let r = cdp
        .call(session, "Runtime.evaluate", json!({ "expression": expr, "returnByValue": true, "awaitPromise": true }))
        .await?;
    if let Some(ex) = r.get("exceptionDetails") {
        warn!("page script failed: {ex}");
    }
    Ok(r["result"]["value"].clone())
}

fn trim(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max { format!("{t}…") } else { t }
}

/// Fills a login form; `__IRIS_USER__` / `__IRIS_PASS__` become JSON strings.
/// Filled fields are marked so the page listing never shows their values.
const LOGIN_JS: &str = r#"(() => {
  const vis = el => { const r = el.getBoundingClientRect(); return r.width > 2 && r.height > 2 && getComputedStyle(el).visibility !== 'hidden'; };
  const set = (el, v) => {
    el.focus();
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, v);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    el.dataset.irisFilled = '1';
  };
  const inputs = [...document.querySelectorAll('input')].filter(vis);
  const pass = inputs.find(i => i.type === 'password');
  const texty = inputs.filter(i => ['email', 'text', 'tel'].includes(i.type));
  const named = texty.find(i => /user|email|login|phone|mail|account/i.test([i.name, i.id, i.autocomplete, i.placeholder, i.getAttribute('aria-label')].join(' ')));
  const before = pass ? texty.filter(i => i.compareDocumentPosition(pass) & Node.DOCUMENT_POSITION_FOLLOWING).pop() : null;
  const user = named || before || texty.find(i => i.type === 'email');
  const filled = [];
  if (user) { set(user, __IRIS_USER__); filled.push('username'); }
  if (pass) { set(pass, __IRIS_PASS__); filled.push('password'); }
  return filled.join(' and ') || 'nothing';
})()"#;

/// Numbers the page's buttons, links and fields and returns the page as text
/// plus a list like `[3] button "Sign in"`.
fn snapshot_js() -> String {
    format!(
        r#"(() => {{
  document.querySelectorAll('[data-iris-n]').forEach(e => e.removeAttribute('data-iris-n'));
  const sel = 'a[href], button, input:not([type=hidden]), select, textarea, [role=button], [role=link], [role=checkbox], [role=tab], [role=menuitem], [contenteditable=true], summary';
  const items = [], labels = {{}}, submits = {{}};
  // What a field holds is never part of its label, and nothing secret is ever
  // shown: passwords, anything the vault filled, anything that looks like one.
  // Only buttons use their value (`<input type=submit value="Sign in">`).
  const secret = el => el.type === 'password' || !!el.dataset.irisFilled || /pass(word|wd|code|phrase)|\bpass\b|pwd|\botp\b|secret|cvv|cc-number/i.test([el.autocomplete, el.name, el.id].join(' '));
  const valueLabel = el => ['submit', 'button', 'reset'].includes(el.type) ? el.value : '';
  const textOf = el => (el.innerText || valueLabel(el) || el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.title || el.name || '').trim().replace(/\s+/g, ' ').slice(0, 80);
  const isSubmit = el => (el.tagName === 'BUTTON' && (el.type || 'submit') === 'submit' && el.form) || (el.tagName === 'INPUT' && ['submit', 'image'].includes(el.type));
  // What Enter in a field submits: search boxes are harmless; otherwise the
  // form's submit button (or "submit form" / "send message" when unlabeled).
  const enterTarget = el => {{
    const form = el.form || el.closest('form');
    const hint = [el.type, el.name, el.id, el.getAttribute('role'), el.getAttribute('aria-label'), el.placeholder, form && form.getAttribute('role'), form && form.getAttribute('action')].join(' ');
    if (el.type === 'search' || /\bsearch\b|\bq\b|\bquery\b|αναζήτηση/i.test(hint)) return '{SEARCH}';
    if (!form) return el.tagName === 'INPUT' ? 'submit form' : 'send message';
    const btn = [...form.querySelectorAll('button, input[type=submit], input[type=image]')].find(isSubmit);
    return (btn && textOf(btn)) || 'submit form';
  }};
  let n = 0;
  for (const el of document.querySelectorAll(sel)) {{
    const r = el.getBoundingClientRect(), cs = getComputedStyle(el);
    if (r.width < 2 || r.height < 2 || cs.visibility === 'hidden' || cs.display === 'none') continue;
    if (++n > {MAX_ELEMENTS}) break;
    el.setAttribute('data-iris-n', n);
    const tag = el.tagName.toLowerCase();
    const kind = el.getAttribute('role') || (tag === 'input' ? (el.type || 'text') + ' field' : tag === 'a' ? 'link' : tag);
    let label = textOf(el);
    labels[n] = label || (isSubmit(el) ? 'submit form' : '');
    if (tag === 'textarea' || el.isContentEditable || (tag === 'input' && ['text', 'search', 'email', 'url', 'tel', 'number', ''].includes(el.type))) submits[n] = enterTarget(el);
    let extra = '';
    if (tag === 'input' || tag === 'textarea') extra = el.dataset.irisFilled ? ' (filled from the vault)' : el.type === 'password' ? ' (password)' : secret(el) ? ' (hidden)' : (el.value && !['submit', 'button', 'reset'].includes(el.type) ? ` = "${{el.value.slice(0, 40)}}"` : (el.placeholder ? ` (${{el.placeholder.slice(0, 40)}})` : ''));
    if (tag === 'a') {{ try {{ extra = ' → ' + new URL(el.href).pathname.slice(0, 60); }} catch (e) {{}} }}
    items.push(`[${{n}}] ${{kind}} "${{label}}"${{extra}}`);
  }}
  const text = (document.body ? document.body.innerText : '').replace(/\n{{3,}}/g, '\n\n').slice(0, {MAX_TEXT});
  return {{ labels, submits, text: `Page: ${{document.title}}\nURL: ${{location.href}}\n\n${{text}}\n\nInteractive elements (use their numbers):\n${{items.join('\n')}}` }};
}})()"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_local_addresses_and_spots_purchases() {
        assert!(blocked("http://127.0.0.1:7700/v1/agents"));
        assert!(blocked("http://localhost:7700"));
        assert!(blocked("http://192.168.1.1/"));
        assert!(blocked("file:///C:/Windows"));
        assert!(blocked("http://172.20.0.5/"));
        assert!(blocked("http://100.101.1.2/"), "Tailscale addresses");
        assert!(blocked("http://[::ffff:127.0.0.1]:9333/json/list"));
        assert!(blocked("http://[fd00::1]/"));
        assert!(blocked("http://router.local/"));
        assert!(!blocked("https://www.shop.example/"));
        assert!(!blocked("http://172.32.0.1/"));
        assert!(blocked("http://169.254.169.254/latest/meta-data/"), "cloud metadata address");
        assert!(COMMIT_WORDS.is_match("Place order"));
        assert!(COMMIT_WORDS.is_match("Ολοκλήρωση παραγγελίας"));
        assert!(COMMIT_WORDS.is_match("Pay €12.50"));
        assert!(!COMMIT_WORDS.is_match("Sign in"));
        assert!(!COMMIT_WORDS.is_match("Menu"));
    }

    #[test]
    fn every_browser_request_is_judged_before_it_goes_out() {
        // Redirects, frames and subresources all arrive here as plain URLs.
        assert_eq!(quick_verdict("http://127.0.0.1:7700/v1/agents"), Some(false));
        assert_eq!(quick_verdict("http://192.168.1.1/admin"), Some(false));
        assert_eq!(quick_verdict("http://localhost:9333/json/list"), Some(false));
        assert_eq!(quick_verdict("https://[::1]/"), Some(false));
        assert_eq!(quick_verdict("file:///C:/Windows/win.ini"), Some(false));
        assert_eq!(quick_verdict("ftp://example.com/x"), Some(false));
        assert_eq!(quick_verdict("chrome://settings"), Some(false));
        assert_eq!(quick_verdict("data:text/html,<p>hi</p>"), Some(true));
        assert_eq!(quick_verdict("blob:https://example.com/1234"), Some(true));
        assert_eq!(quick_verdict("about:blank"), Some(true));
        // A public-looking name still needs its address checked.
        assert_eq!(quick_verdict("https://www.example.com/a.png"), None);
        assert_eq!(quick_verdict("HTTP://Example.com/"), None);
    }

    #[test]
    fn sending_asks_unless_full_auto() {
        for label in ["Send", "Post comment", "Reply", "submit form", "Delete account", "Αποστολή"] {
            assert!(risky_label(label, false), "{label}");
            assert!(!risky_label(label, true), "{label}");
        }
        assert!(risky_label("Place order", true));
        for label in ["Sign in", "Next", "Menu", "Search", "Show more", ""] {
            assert!(!risky_label(label, false), "{label}");
        }
    }
}
