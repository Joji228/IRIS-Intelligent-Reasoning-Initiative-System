//! I.R.I.S. Link: reaching the engine from anywhere through the relay
//! (`relay/`, a Cloudflare Worker) without opening anything on the server.
//!
//! The engine dials out to the relay and keeps that connection. Each phone
//! or browser that connects to the relay becomes a session here:
//!
//! 1. **Handshake.** The client sends `{"t":"hello","v":1,"epk":…}` with a
//!    fresh P-256 public key. The engine answers `{"t":"welcome","epk":…}`
//!    with one of its own. Both derive two AES-256-GCM keys with HKDF-SHA256
//!    from ECDH(client, engine identity) and ECDH(client, engine ephemeral):
//!    only the real engine (holding the identity key the client pinned from
//!    the QR or the web app) can answer, and each session has fresh keys.
//! 2. **Sealed frames.** Everything after that is `{"s":n,"b":base64}`, AES-GCM
//!    with the counter `n` as nonce; counters only go up, so replays fail.
//! 3. **Sign-in.** `auth` (a device token), `pair` (a one-time code from the
//!    QR) or `login` (username + password, for browsers). Pairing and login
//!    create a device with its own token, revocable in Settings → Paired devices.
//! 4. **Use.** `req` carries any API request (answered by the engine's own
//!    router, so every feature works remotely); live events are pushed as `ev`.
//!
//! The relay only ever sees the handshake keys and sealed bytes.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use futures_util::{SinkExt, StreamExt};
use hkdf::Hkdf;
use p256::{PublicKey, SecretKey, ecdh::EphemeralSecret, elliptic_curve::sec1::ToEncodedPoint};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{Message as WsMessage, client::IntoClientRequest};
use tracing::{info, warn};

use crate::api::AppState;

const SALT: &[u8] = b"iris-link-v1";
/// Pairing codes last this long and work once.
const PAIR_TTL_MS: i64 = 10 * 60 * 1000;
/// Wrong passwords allowed before the login starts making people wait.
const MAX_LOGIN_FAILURES: u32 = 5;
/// The wait doubles with every further wrong password, up to this. Short on
/// purpose: anyone who finds the relay can type wrong passwords, and a long
/// lockout would let them keep the owner out (paired devices never wait).
const MAX_LOCKOUT_MS: i64 = 60 * 1000;
/// A client must sign in this soon after connecting, or it's dropped, so
/// strangers can't sit on the relay's few connection slots.
const SIGN_IN_WITHIN: Duration = Duration::from_secs(20);
/// Sign-in attempts per connection before it's dropped.
const MAX_SIGN_IN_ATTEMPTS: u32 = 3;
/// Largest API response sent back in one frame: base64 inside the sealed
/// message and again around it must stay under the relay's 32 MB frames.
const MAX_BODY: usize = 16 * 1024 * 1024;

// ---- identity -------------------------------------------------------------------

/// The engine's long-term key pair. Clients pin the public half.
pub struct Identity {
    secret: SecretKey,
    /// Uncompressed SEC1 public key, base64url.
    pub public: String,
}

fn b64url(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn from_b64url(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim_end_matches('=')).ok()
}

impl Identity {
    fn from_secret(secret: SecretKey) -> Self {
        let public = b64url(secret.public_key().to_encoded_point(false).as_bytes());
        Self { secret, public }
    }

    /// Loads `secrets/link-identity.key`, creating it on first use.
    pub fn load_or_create(data_dir: &Path) -> anyhow::Result<Self> {
        let path = data_dir.join("secrets").join("link-identity.key");
        if let Ok(hex_key) = std::fs::read_to_string(&path) {
            let bytes = hex::decode(hex_key.trim())?;
            return Ok(Self::from_secret(SecretKey::from_slice(&bytes)?));
        }
        let secret = SecretKey::random(&mut OsRng);
        std::fs::create_dir_all(path.parent().expect("has parent"))?;
        std::fs::write(&path, hex::encode(secret.to_bytes()))?;
        info!("created the I.R.I.S. Link identity key");
        Ok(Self::from_secret(secret))
    }
}

// ---- sealing ----------------------------------------------------------------------

/// One session's keys and counters.
pub struct Seal {
    send: Aes256Gcm,
    recv: Aes256Gcm,
    sent: u64,
    received: u64,
    /// Which way this side sends ("s2c" for the engine).
    send_aad: &'static [u8],
    recv_aad: &'static [u8],
}

fn nonce(counter: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[4..].copy_from_slice(&counter.to_be_bytes());
    n
}

/// Both directions' keys from the two ECDH results, bound to both public keys.
fn derive(ss: &[u8], ee: &[u8], client_pub: &[u8], server_pub: &[u8]) -> ([u8; 32], [u8; 32]) {
    let ikm = [ss, ee].concat();
    let info = [client_pub, server_pub].concat();
    let mut okm = [0u8; 64];
    Hkdf::<Sha256>::new(Some(SALT), &ikm).expand(&info, &mut okm).expect("64 bytes is a valid length");
    let (mut c2s, mut s2c) = ([0u8; 32], [0u8; 32]);
    c2s.copy_from_slice(&okm[..32]);
    s2c.copy_from_slice(&okm[32..]);
    (c2s, s2c)
}

impl Seal {
    fn new(send_key: &[u8; 32], recv_key: &[u8; 32], server_side: bool) -> Self {
        Self {
            send: Aes256Gcm::new_from_slice(send_key).expect("32-byte key"),
            recv: Aes256Gcm::new_from_slice(recv_key).expect("32-byte key"),
            sent: 0,
            received: 0,
            send_aad: if server_side { b"s2c" } else { b"c2s" },
            recv_aad: if server_side { b"c2s" } else { b"s2c" },
        }
    }

    pub fn seal(&mut self, plaintext: &[u8]) -> String {
        self.sent += 1;
        let ct = self
            .send
            .encrypt(Nonce::from_slice(&nonce(self.sent)), Payload { msg: plaintext, aad: self.send_aad })
            .expect("encryption can't fail");
        json!({ "s": self.sent, "b": B64.encode(ct) }).to_string()
    }

    /// Opens a frame; fails on tampering, a wrong key or a replayed counter.
    pub fn open(&mut self, frame: &str) -> Option<Vec<u8>> {
        let v: Value = serde_json::from_str(frame).ok()?;
        let counter = v["s"].as_u64()?;
        if counter <= self.received {
            return None;
        }
        let ct = B64.decode(v["b"].as_str()?).ok()?;
        let pt = self.recv.decrypt(Nonce::from_slice(&nonce(counter)), Payload { msg: &ct, aad: self.recv_aad }).ok()?;
        self.received = counter;
        Some(pt)
    }
}

/// The engine's half of the handshake: the reply to send and the session's seal.
fn accept_hello(identity: &Identity, hello: &Value) -> Option<(String, Seal)> {
    if hello["t"] != "hello" || hello["v"] != 1 {
        return None;
    }
    let client_bytes = from_b64url(hello["epk"].as_str()?)?;
    let client = PublicKey::from_sec1_bytes(&client_bytes).ok()?;
    let eph = EphemeralSecret::random(&mut OsRng);
    let eph_pub = eph.public_key().to_encoded_point(false);
    let ss = p256::ecdh::diffie_hellman(identity.secret.to_nonzero_scalar(), client.as_affine());
    let ee = eph.diffie_hellman(&client);
    let (c2s, s2c) = derive(ss.raw_secret_bytes(), ee.raw_secret_bytes(), &client_bytes, eph_pub.as_bytes());
    let reply = json!({ "t": "welcome", "v": 1, "epk": b64url(eph_pub.as_bytes()) }).to_string();
    Some((reply, Seal::new(&s2c, &c2s, true)))
}

// ---- settings ---------------------------------------------------------------------

/// `[link]` in iris.toml.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct LinkConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub enabled: bool,
}

/// The PC login (`secrets/link-login.json`): a username and an Argon2id hash.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Login {
    pub user: String,
    pub hash: String,
}

fn login_path(data_dir: &Path) -> PathBuf {
    data_dir.join("secrets").join("link-login.json")
}

pub fn load_login(data_dir: &Path) -> Option<Login> {
    serde_json::from_str(&std::fs::read_to_string(login_path(data_dir)).ok()?).ok()
}

pub fn set_login(data_dir: &Path, user: &str, password: &str) -> anyhow::Result<()> {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing the password failed: {e}"))?
        .to_string();
    let path = login_path(data_dir);
    std::fs::create_dir_all(path.parent().expect("has parent"))?;
    std::fs::write(path, serde_json::to_string(&Login { user: user.trim().to_owned(), hash })?)?;
    Ok(())
}

pub fn clear_login(data_dir: &Path) {
    let _ = std::fs::remove_file(login_path(data_dir));
}

fn check_password(login: &Login, user: &str, password: &str) -> bool {
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    let Ok(parsed) = PasswordHash::new(&login.hash) else { return false };
    let ok = Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok();
    ok && login.user.eq_ignore_ascii_case(user.trim())
}

// ---- shared state ---------------------------------------------------------------

/// Link state shared with the API (status, pairing codes, login lockout).
pub struct Link {
    pub identity: Identity,
    pub config: Mutex<LinkConfig>,
    pub connected: std::sync::atomic::AtomicBool,
    /// One-time pairing code → expiry (ms).
    pairing: Mutex<HashMap<String, i64>>,
    /// Failed logins in a row and when the lockout ends.
    failures: Mutex<(u32, i64)>,
    /// Woken when the remote-access settings change, so a connection that is
    /// already open notices (turned off, or pointed at another relay).
    changed: tokio::sync::Notify,
    /// Ids of devices that were just removed; live event streams stop for them.
    revoked: tokio::sync::broadcast::Sender<String>,
}

impl Link {
    pub fn new(identity: Identity, config: LinkConfig) -> Self {
        Self {
            identity,
            config: Mutex::new(config),
            connected: Default::default(),
            pairing: Mutex::default(),
            failures: Mutex::default(),
            changed: tokio::sync::Notify::new(),
            revoked: tokio::sync::broadcast::channel(64).0,
        }
    }

    /// Saves new remote-access settings and tells the open connection.
    pub fn set_config(&self, cfg: LinkConfig) {
        *self.config.lock().unwrap() = cfg;
        // A permit is kept if nobody is waiting yet, so the change isn't missed.
        self.changed.notify_one();
    }

    /// A device was removed: its live streams (direct and through the relay) end.
    pub fn revoke(&self, device_id: &str) {
        let _ = self.revoked.send(device_id.to_owned());
    }

    pub fn watch_revoked(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.revoked.subscribe()
    }

    /// A new one-time pairing code (8 easy-to-read characters).
    pub fn new_pairing_code(&self) -> (String, i64) {
        const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
        let mut bytes = [0u8; 8];
        rand_core::RngCore::fill_bytes(&mut OsRng, &mut bytes);
        let code: String = bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect();
        let expires = iris_core::now_ms() + PAIR_TTL_MS;
        let mut codes = self.pairing.lock().unwrap();
        codes.retain(|_, exp| *exp > iris_core::now_ms());
        codes.insert(code.clone(), expires);
        (code, expires)
    }

    fn take_pairing_code(&self, code: &str) -> bool {
        let code = code.trim().to_ascii_uppercase().replace(['-', ' '], "");
        let mut codes = self.pairing.lock().unwrap();
        matches!(codes.remove(&code), Some(exp) if exp > iris_core::now_ms())
    }

    /// Checks a login, with a lockout after repeated failures.
    fn try_login(&self, data_dir: &Path, user: &str, password: &str) -> Result<(), String> {
        let now = iris_core::now_ms();
        {
            let f = self.failures.lock().unwrap();
            if f.0 >= MAX_LOGIN_FAILURES && now < f.1 {
                return Err(format!("too many wrong passwords; try again in {} seconds", (f.1 - now) / 1000 + 1));
            }
        }
        let Some(login) = load_login(data_dir) else { return Err("password login isn't set up on the server".into()) };
        if check_password(&login, user, password) {
            *self.failures.lock().unwrap() = (0, 0);
            Ok(())
        } else {
            let mut f = self.failures.lock().unwrap();
            f.0 += 1;
            if f.0 >= MAX_LOGIN_FAILURES {
                let wait = (5_000i64 << (f.0 - MAX_LOGIN_FAILURES).min(4)).min(MAX_LOCKOUT_MS);
                f.1 = now + wait;
                warn!("I.R.I.S. Link: {} wrong passwords in a row; next try in {} s", f.0, wait / 1000);
            }
            Err("wrong username or password".into())
        }
    }
}

// ---- the connection to the relay -----------------------------------------------

/// Keeps the engine connected to the relay while the link is enabled.
pub async fn run_forever(st: AppState) {
    let mut backoff = 2u64;
    loop {
        let cfg = st.link.config.lock().unwrap().clone();
        let key = std::fs::read_to_string(st.data_dir.join("secrets").join("relay.key")).unwrap_or_default();
        if !cfg.enabled || cfg.url.trim().is_empty() || key.trim().is_empty() {
            tokio::time::sleep(Duration::from_secs(10)).await;
            continue;
        }
        match connect_once(&st, &cfg.url, key.trim()).await {
            Ok(()) => backoff = 2,
            Err(e) => warn!("I.R.I.S. Link: {e:#}"),
        }
        set_connected(&st, false);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(60);
    }
}

fn set_connected(st: &AppState, on: bool) {
    if st.link.connected.swap(on, std::sync::atomic::Ordering::Relaxed) != on {
        st.emit(crate::events::Event::LinkChanged { connected: on });
    }
}

/// Whether a connection to `connected_url` should stay open under the current
/// settings: remote access on, and still the same relay.
fn should_stay_connected(cfg: &LinkConfig, connected_url: &str) -> bool {
    let same = |a: &str, b: &str| a.trim().trim_end_matches('/').eq_ignore_ascii_case(b.trim().trim_end_matches('/'));
    cfg.enabled && !cfg.url.trim().is_empty() && same(&cfg.url, connected_url)
}

/// Returns once `device_id` has been removed: when its id arrives on
/// `revoked`, or, as a fallback if messages were missed and every minute,
/// when `alive()` says the device is gone.
pub async fn until_revoked(
    mut revoked: tokio::sync::broadcast::Receiver<String>,
    device_id: &str,
    alive: impl Fn() -> bool,
) {
    use tokio::sync::broadcast::error::RecvError;
    let mut recheck = tokio::time::interval(Duration::from_secs(60));
    recheck.tick().await; // the first tick is immediate
    loop {
        tokio::select! {
            r = revoked.recv() => match r {
                Ok(id) if id == device_id => return,
                Ok(_) => {}
                Err(RecvError::Lagged(_)) => if !alive() { return },
                Err(RecvError::Closed) => return,
            },
            _ = recheck.tick() => if !alive() { return },
        }
    }
}

/// Whether the device owning `token` still exists (a database error counts as yes).
pub fn device_alive(st: &AppState, token: &str) -> bool {
    !matches!(st.store.authenticate_device(&crate::auth::hash_token(token)), Ok(None))
}

/// One session with a phone or browser.
struct Session {
    seal: Option<Arc<Mutex<Seal>>>,
    /// The device token it signed in with (sent along with its requests).
    token: Option<String>,
    events: Option<tokio::task::JoinHandle<()>>,
    /// Sign-in attempts so far.
    attempts: u32,
}

impl Session {
    fn new() -> Self {
        Self { seal: None, token: None, events: None, attempts: 0 }
    }
}

async fn connect_once(st: &AppState, url: &str, key: &str) -> anyhow::Result<()> {
    let ws_url = format!("{}/link/server", url.trim_end_matches('/').replacen("http", "ws", 1));
    let mut req = ws_url.as_str().into_client_request()?;
    req.headers_mut().insert("X-Iris-Server-Key", key.parse()?);
    // Lets browsers signing in with a password find the key to expect.
    req.headers_mut().insert("X-Iris-Public-Key", st.link.identity.public.parse()?);
    let (ws, _) = tokio_tungstenite::connect_async(req).await?;
    info!("I.R.I.S. Link connected to {url}");
    set_connected(st, true);
    let (mut sink, mut stream) = ws.split();

    // Everything to the relay goes through one channel (frames for clients).
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        let mut ping = tokio::time::interval(Duration::from_secs(30));
        loop {
            tokio::select! {
                frame = rx.recv() => match frame {
                    Some(f) => if sink.send(WsMessage::text(f)).await.is_err() { break },
                    None => break,
                },
                _ = ping.tick() => if sink.send(WsMessage::Ping(Default::default())).await.is_err() { break },
            }
        }
    });

    let router = crate::api::router(st.clone());
    let mut sessions: HashMap<String, Session> = HashMap::new();
    // Clients whose time to sign in is up.
    let (deadline_tx, mut deadline_rx) = mpsc::unbounded_channel::<String>();
    // However the loop ends (closed, error, or switched off), the cleanup
    // after it runs, so no session task is left behind.
    let mut result: anyhow::Result<()> = Ok(());
    loop {
        let msg = tokio::select! {
            msg = stream.next() => match msg {
                Some(Ok(msg)) => msg,
                Some(Err(e)) => {
                    result = Err(e.into());
                    break;
                }
                None => break,
            },
            // Remote access was turned off, or pointed at another relay: hang up now.
            _ = st.link.changed.notified() => {
                if should_stay_connected(&st.link.config.lock().unwrap(), url) {
                    continue;
                }
                info!("I.R.I.S. Link: remote access changed, disconnecting");
                break;
            }
            Some(cid) = deadline_rx.recv() => {
                if sessions.get(&cid).is_some_and(|s| s.token.is_none()) {
                    if let Some(s) = sessions.remove(&cid) {
                        if let Some(h) = s.events {
                            h.abort();
                        }
                    }
                    let _ = tx.send(json!({ "c": cid, "kick": true }).to_string());
                }
                continue;
            }
        };
        // Nothing is processed (sign-ins included) once remote access is off.
        if !st.link.config.lock().unwrap().enabled {
            info!("I.R.I.S. Link: remote access is off, disconnecting");
            break;
        }
        let Ok(text) = msg.to_text() else { continue };
        let Ok(frame) = serde_json::from_str::<Value>(text) else { continue };
        let Some(cid) = frame["c"].as_str().map(str::to_owned) else { continue };
        // New clients, and ones that were already at the relay when this
        // connection started (they start over with a new hello).
        if frame["open"] == true || (!sessions.contains_key(&cid) && frame["close"] != true) {
            sessions.insert(cid.clone(), Session::new());
            let deadline_tx = deadline_tx.clone();
            let cid = cid.clone();
            tokio::spawn(async move {
                tokio::time::sleep(SIGN_IN_WITHIN).await;
                let _ = deadline_tx.send(cid);
            });
            if frame["open"] == true {
                continue;
            }
        }
        if frame["close"] == true {
            if let Some(s) = sessions.remove(&cid) {
                if let Some(h) = s.events {
                    h.abort();
                }
            }
            continue;
        }
        let Some(data) = frame["d"].as_str() else { continue };
        let Some(session) = sessions.get_mut(&cid) else { continue };
        let send = {
            let tx = tx.clone();
            let cid = cid.clone();
            move |d: String| {
                let _ = tx.send(json!({ "c": cid, "d": d }).to_string());
            }
        };
        let kick = |tx: &mpsc::UnboundedSender<String>| {
            let _ = tx.send(json!({ "c": cid, "kick": true }).to_string());
        };

        // 1. Handshake.
        let Some(seal) = session.seal.clone() else {
            match serde_json::from_str::<Value>(data).ok().and_then(|h| accept_hello(&st.link.identity, &h)) {
                Some((reply, seal)) => {
                    send(reply);
                    session.seal = Some(Arc::new(Mutex::new(seal)));
                }
                None => kick(&tx),
            }
            continue;
        };
        // 2. Everything else is sealed.
        let opened = seal.lock().unwrap().open(data);
        let Some(msg) = opened.and_then(|pt| serde_json::from_slice::<Value>(&pt).ok()) else {
            kick(&tx);
            continue;
        };
        let reply = {
            let seal = seal.clone();
            let send = send.clone();
            move |v: Value| {
                // Seal and queue under one lock, so counters go out in order.
                let mut s = seal.lock().unwrap();
                send(s.seal(v.to_string().as_bytes()));
            }
        };
        match msg["t"].as_str().unwrap_or_default() {
            "auth" | "pair" | "login" => match sign_in(st, &msg) {
                Ok((token, device, fresh, device_id)) => {
                    session.token = Some(token.clone());
                    if let Some(h) = session.events.take() {
                        h.abort();
                    }
                    session.events = Some(forward_events(st, reply.clone(), token.clone(), device_id));
                    let mut answer = json!({ "t": "signed_in", "device": device });
                    if fresh {
                        // A new device: this token is how it signs in next time.
                        answer["token"] = json!(token);
                    }
                    reply(answer);
                }
                Err(e) => {
                    reply(json!({ "t": "error", "error": e }));
                    session.attempts += 1;
                    if session.attempts >= MAX_SIGN_IN_ATTEMPTS {
                        kick(&tx);
                    }
                }
            },
            "req" => match session.token.clone() {
                Some(token) => {
                    let router = router.clone();
                    tokio::spawn(async move { reply(answer_request(router, &token, &msg).await) });
                }
                None => reply(json!({ "t": "res", "id": msg["id"], "status": 401, "body": "sign in first" })),
            },
            "ping" => reply(json!({ "t": "pong" })),
            _ => {}
        }
    }
    for (_, s) in sessions {
        if let Some(h) = s.events {
            h.abort();
        }
    }
    writer.abort();
    result
}

/// Signs a session in. Returns the device token, the device's name, whether
/// the device is new (its token is then sent to it once) and its id.
fn sign_in(st: &AppState, msg: &Value) -> Result<(String, String, bool, String), String> {
    let name = msg["name"].as_str().map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Remote device");
    let name: String = name.chars().take(60).collect();
    match msg["t"].as_str() {
        Some("auth") => {
            let token = msg["token"].as_str().unwrap_or_default();
            match st.store.authenticate_device(&crate::auth::hash_token(token)) {
                Ok(Some(device)) => Ok((token.to_owned(), device.name, false, device.id)),
                _ => Err("this device was removed or isn't paired; pair it again".into()),
            }
        }
        Some("pair") => {
            if !st.link.take_pairing_code(msg["code"].as_str().unwrap_or_default()) {
                return Err("that pairing code is wrong or has expired; make a new one on the server".into());
            }
            new_device(st, &name)
        }
        Some("login") => {
            st.link.try_login(&st.data_dir, msg["user"].as_str().unwrap_or_default(), msg["password"].as_str().unwrap_or_default())?;
            new_device(st, &name)
        }
        _ => Err("unknown sign-in".into()),
    }
}

fn new_device(st: &AppState, name: &str) -> Result<(String, String, bool, String), String> {
    let token = crate::auth::new_token();
    let device = st.store.create_device(name, &crate::auth::hash_token(&token)).map_err(|e| e.to_string())?;
    info!("I.R.I.S. Link: new device “{}”", device.name);
    st.emit(crate::events::Event::DeviceAdded { name: device.name.clone() });
    Ok((token, device.name, true, device.id))
}

/// Pushes the engine's live events to a signed-in session, until the session
/// ends or its device is removed.
fn forward_events(st: &AppState, reply: impl Fn(Value) + Send + 'static, token: String, device_id: String) -> tokio::task::JoinHandle<()> {
    let mut rx = st.events.subscribe();
    let revoked = st.link.watch_revoked();
    let state = st.clone();
    tokio::spawn(async move {
        let gone = until_revoked(revoked, &device_id, || device_alive(&state, &token));
        tokio::pin!(gone);
        loop {
            tokio::select! {
                _ = &mut gone => {
                    info!("I.R.I.S. Link: a removed device's live stream was closed");
                    reply(json!({ "t": "revoked" }));
                    break;
                }
                event = rx.recv() => match event {
                    Ok(event) => reply(json!({ "t": "ev", "e": event })),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                },
            }
        }
    })
}

/// Runs an API request through the engine's own router, as the session's device.
async fn answer_request(router: axum::Router, token: &str, msg: &Value) -> Value {
    use tower::ServiceExt;
    let id = msg["id"].clone();
    let method = msg["method"].as_str().unwrap_or("GET");
    let path = msg["path"].as_str().unwrap_or("/");
    if !path.starts_with("/v1/") || path.starts_with("/v1/events") {
        return json!({ "t": "res", "id": id, "status": 400, "body": "not available over the link" });
    }
    let body = match &msg["body"] {
        Value::Null => axum::body::Body::empty(),
        Value::String(s) => axum::body::Body::from(s.clone()),
        other => axum::body::Body::from(other.to_string()),
    };
    let request = axum::http::Request::builder()
        .method(method)
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(body);
    let Ok(request) = request else {
        return json!({ "t": "res", "id": id, "status": 400, "body": "bad request" });
    };
    let response = match router.oneshot(request).await {
        Ok(r) => r,
        Err(e) => match e {},
    };
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let Ok(bytes) = axum::body::to_bytes(response.into_body(), MAX_BODY).await else {
        return json!({ "t": "res", "id": id, "status": 413, "body": "too large to send over the link" });
    };
    let textual = content_type.starts_with("application/json") || content_type.starts_with("text/") || content_type.is_empty();
    if textual {
        json!({ "t": "res", "id": id, "status": status, "ct": content_type, "body": String::from_utf8_lossy(&bytes) })
    } else {
        json!({ "t": "res", "id": id, "status": status, "ct": content_type, "b64": B64.encode(&bytes) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A client handshake as the apps do it (in Rust here), against the engine's.
    fn client_handshake(identity_pub: &str, welcome_for: impl FnOnce(&Value) -> String) -> Seal {
        let eph = EphemeralSecret::random(&mut OsRng);
        let my_pub = eph.public_key().to_encoded_point(false);
        let hello = json!({ "t": "hello", "v": 1, "epk": b64url(my_pub.as_bytes()) });
        let welcome: Value = serde_json::from_str(&welcome_for(&hello)).unwrap();
        let server_eph = from_b64url(welcome["epk"].as_str().unwrap()).unwrap();
        let identity = PublicKey::from_sec1_bytes(&from_b64url(identity_pub).unwrap()).unwrap();
        let ss = eph.diffie_hellman(&identity);
        // The client's second ECDH needs the same ephemeral secret.
        let ee = eph.diffie_hellman(&PublicKey::from_sec1_bytes(&server_eph).unwrap());
        let (c2s, s2c) = derive(ss.raw_secret_bytes(), ee.raw_secret_bytes(), my_pub.as_bytes(), &server_eph);
        Seal::new(&c2s, &s2c, false)
    }

    #[test]
    fn handshake_seal_and_replay() {
        let identity = Identity::from_secret(SecretKey::random(&mut OsRng));
        let mut server: Option<Seal> = None;
        let mut client = client_handshake(&identity.public, |hello| {
            let (reply, seal) = accept_hello(&identity, hello).unwrap();
            server = Some(seal);
            reply
        });
        let mut server = server.unwrap();

        let frame = client.seal(br#"{"t":"ping"}"#);
        assert_eq!(server.open(&frame).unwrap(), br#"{"t":"ping"}"#);
        assert!(server.open(&frame).is_none(), "a replayed frame is refused");
        let back = server.seal(b"pong");
        assert_eq!(client.open(&back).unwrap(), b"pong");

        // Tampering is caught.
        let mut v: Value = serde_json::from_str(&client.seal(b"hello")).unwrap();
        let mut ct = B64.decode(v["b"].as_str().unwrap()).unwrap();
        ct[0] ^= 1;
        v["b"] = json!(B64.encode(ct));
        assert!(server.open(&v.to_string()).is_none());

        // A client that pinned a different engine key can't talk to this one.
        let other = Identity::from_secret(SecretKey::random(&mut OsRng));
        let mut server2: Option<Seal> = None;
        let mut wrong = client_handshake(&other.public, |hello| {
            let (reply, seal) = accept_hello(&identity, hello).unwrap();
            server2 = Some(seal);
            reply
        });
        assert!(server2.unwrap().open(&wrong.seal(b"hi")).is_none());
    }

    /// Fixed inputs for the key derivation and sealing. The phone app's test
    /// (android/app/src/test/…/LinkCryptoTest.kt) checks the same numbers, so
    /// the two sides cannot drift apart without a test failing on one of them.
    #[test]
    fn interop_vector() {
        let (ss, ee) = ([0x11u8; 32], [0x22u8; 32]);
        let client_pub = [&[4u8][..], &[0x33u8; 64]].concat();
        let server_pub = [&[4u8][..], &[0x44u8; 64]].concat();
        let (c2s, s2c) = derive(&ss, &ee, &client_pub, &server_pub);
        assert_eq!(hex::encode(c2s), "0cdb65aeff6df44c02ced2402ca043a1e57926b163d9eeb5d3c82e8f36b6fe96");
        assert_eq!(hex::encode(s2c), "7f64d85a2a88571cf19aa236978c4f8a11a2337da191f0124f76b4f9ad76398d");
        assert_eq!(Seal::new(&s2c, &c2s, true).seal(b"hello iris"), r#"{"b":"FlzAP5+iFnkx3hL10dk7eQhQ/9vIXWLwHMQ=","s":1}"#);
        assert_eq!(Seal::new(&c2s, &s2c, false).seal(b"hi engine"), r#"{"b":"D3dakGfWTbToWcf1QwsqhzW8XMqxM8vYHQ==","s":1}"#);
    }

    #[test]
    fn switching_off_or_moving_the_relay_drops_the_connection() {
        let on = LinkConfig { url: "https://relay.example".into(), enabled: true };
        assert!(should_stay_connected(&on, "https://relay.example"));
        assert!(should_stay_connected(&on, "https://relay.example/"), "a trailing slash is the same relay");
        assert!(!should_stay_connected(&LinkConfig { enabled: false, ..on.clone() }, "https://relay.example"), "turned off");
        assert!(!should_stay_connected(&LinkConfig { url: "https://other.example".into(), ..on.clone() }, "https://relay.example"), "another relay");
        assert!(!should_stay_connected(&LinkConfig { url: String::new(), ..on.clone() }, "https://relay.example"), "no address");
    }

    #[tokio::test]
    async fn a_removed_devices_streams_end() {
        use tokio::time::timeout;
        let (tx, _) = tokio::sync::broadcast::channel::<String>(2);

        // Its own id ends the wait, other devices' ids don't.
        let rx = tx.subscribe();
        tx.send("someone-else".into()).unwrap();
        tx.send("mine".into()).unwrap();
        assert!(timeout(Duration::from_secs(2), until_revoked(rx, "mine", || true)).await.is_ok());

        let rx = tx.subscribe();
        tx.send("someone-else".into()).unwrap();
        assert!(
            timeout(Duration::from_millis(150), until_revoked(rx, "mine", || true)).await.is_err(),
            "another device's removal leaves this stream alone"
        );

        // If messages were missed, the device is looked up instead.
        let rx = tx.subscribe();
        for i in 0..6 {
            tx.send(format!("other-{i}")).unwrap();
        }
        assert!(timeout(Duration::from_secs(2), until_revoked(rx, "mine", || false)).await.is_ok(), "gone after a lag");
        let rx = tx.subscribe();
        for i in 0..6 {
            tx.send(format!("other-{i}")).unwrap();
        }
        assert!(timeout(Duration::from_millis(150), until_revoked(rx, "mine", || true)).await.is_err(), "still there after a lag");
    }

    #[test]
    fn login_and_pairing_codes() {
        let dir = std::env::temp_dir().join(format!("iris-link-{}", uuid::Uuid::new_v4().simple()));
        set_login(&dir, "alice", "correct horse battery").unwrap();
        let login = load_login(&dir).unwrap();
        assert!(!login.hash.contains("correct"), "only the hash is stored");
        assert!(check_password(&login, "Alice", "correct horse battery"));
        assert!(!check_password(&login, "alice", "wrong"));

        let link = Link::new(Identity::from_secret(SecretKey::random(&mut OsRng)), LinkConfig::default());
        for _ in 0..MAX_LOGIN_FAILURES {
            assert!(link.try_login(&dir, "alice", "nope").is_err());
        }
        assert!(link.try_login(&dir, "alice", "correct horse battery").unwrap_err().contains("too many"), "locked out");
        {
            // The wait is short and capped, so nobody can keep the owner out for long.
            let mut f = link.failures.lock().unwrap();
            assert!(f.1 - iris_core::now_ms() <= 5_000);
            *f = (50, 0);
        }
        assert!(link.try_login(&dir, "alice", "x").is_err());
        assert!(link.failures.lock().unwrap().1 - iris_core::now_ms() <= MAX_LOCKOUT_MS);
        *link.failures.lock().unwrap() = (0, 0);
        assert!(link.try_login(&dir, "alice", "correct horse battery").is_ok());

        let (code, _) = link.new_pairing_code();
        assert_eq!(code.len(), 8);
        assert!(link.take_pairing_code(&code.to_lowercase()));
        assert!(!link.take_pairing_code(&code), "codes work once");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
