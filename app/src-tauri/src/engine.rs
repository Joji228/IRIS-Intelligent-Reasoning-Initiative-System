//! Runs the bundled `iris-engine` and keeps it alive.
//!
//! The engine is a separate process so that the agents keep working even if
//! the UI crashes or is being updated. A supervisor thread starts it, restarts
//! it with backoff if it exits, and stops it when the app quits. If an engine
//! is already answering on the port (e.g. left over from a previous app run)
//! it is used as-is instead of starting a second one.

use std::{
    fs::{self, OpenOptions},
    net::{SocketAddr, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub const ENGINE_PORT: u16 = 7700;
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// Same rule as the engine (crates/engine/src/config.rs): `IRIS_DATA_DIR`, else
/// `%LOCALAPPDATA%\dev.iris.desktop`, named after the app identifier so it is
/// separate from the install folder.
pub fn data_dir() -> PathBuf {
    std::env::var_os("IRIS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("dev.iris.desktop")
        })
}

pub fn engine_reachable() -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], ENGINE_PORT));
    TcpStream::connect_timeout(&addr, Duration::from_millis(400)).is_ok()
}

#[derive(Clone, Default)]
pub struct Supervisor {
    child: Arc<Mutex<Option<Child>>>,
    stopping: Arc<AtomicBool>,
}

impl Supervisor {
    pub fn start(&self) {
        let me = self.clone();
        thread::Builder::new()
            .name("engine-supervisor".into())
            .spawn(move || me.supervise())
            .expect("spawn supervisor thread");
    }

    /// Stops the engine we started (an adopted external one is left alone).
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn supervise(&self) {
        let mut backoff = Duration::from_secs(1);
        while !self.stopping.load(Ordering::SeqCst) {
            if engine_reachable() {
                // Someone else's engine is serving; check again later.
                thread::sleep(Duration::from_secs(3));
                continue;
            }
            let started = Instant::now();
            match spawn_engine() {
                Ok(child) => {
                    *self.child.lock().unwrap() = Some(child);
                    self.wait_for_exit();
                }
                Err(e) => eprintln!("could not start iris-engine: {e}"),
            }
            if self.stopping.load(Ordering::SeqCst) {
                break;
            }
            // A long healthy run resets the backoff; a crash loop slows down.
            if started.elapsed() > Duration::from_secs(60) {
                backoff = Duration::from_secs(1);
            }
            thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(30));
        }
    }

    fn wait_for_exit(&self) {
        loop {
            thread::sleep(Duration::from_millis(500));
            let mut guard = self.child.lock().unwrap();
            match guard.as_mut().map(|c| c.try_wait()) {
                Some(Ok(None)) => {}         // still running
                _ => {
                    guard.take();
                    return;
                }
            }
        }
    }
}

fn engine_path() -> std::io::Result<PathBuf> {
    // Tauri installs bundled sidecars next to the app's own executable.
    let dir = std::env::current_exe()?.parent().map(PathBuf::from).unwrap_or_default();
    let exe = dir.join(if cfg!(windows) { "iris-engine.exe" } else { "iris-engine" });
    if exe.exists() {
        Ok(exe)
    } else {
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} not found", exe.display())))
    }
}

fn spawn_engine() -> std::io::Result<Child> {
    let logs = data_dir().join("logs");
    fs::create_dir_all(&logs)?;
    let log_path = logs.join("engine.log");
    if fs::metadata(&log_path).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false) {
        let _ = fs::rename(&log_path, logs.join("engine.old.log"));
    }
    let log = OpenOptions::new().create(true).append(true).open(&log_path)?;

    let mut cmd = Command::new(engine_path()?);
    cmd.stdin(Stdio::null()).stdout(Stdio::from(log.try_clone()?)).stderr(Stdio::from(log));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.spawn()
}

/// Whether what answers on the engine's port is the real engine: it must be
/// able to prove it knows `token` (the engine reads it from the same file).
/// Something else that grabbed the port first can't, so it never gets the token.
pub fn engine_knows_token(token: &str) -> bool {
    use std::io::{Read, Write};
    use sha2::{Digest, Sha256};

    // A nonce that is new every time (it only has to be unique, not secret).
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
    let nonce = hex::encode(&Sha256::digest(format!("{nanos}:{}", std::process::id()).as_bytes())[..12]);
    let addr = SocketAddr::from(([127, 0, 0, 1], ENGINE_PORT));
    let Ok(mut conn) = TcpStream::connect_timeout(&addr, Duration::from_millis(800)) else { return false };
    let _ = conn.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = conn.set_write_timeout(Some(Duration::from_secs(2)));
    let request = format!("GET /health?nonce={nonce} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n");
    if conn.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut reply = Vec::new();
    let _ = conn.take(64 * 1024).read_to_end(&mut reply);
    let reply = String::from_utf8_lossy(&reply);
    let expected = hex::encode(Sha256::digest(format!("iris-proof:{nonce}:{token}").as_bytes()));
    reply.split("\"proof\":\"").nth(1).and_then(|rest| rest.get(..64)).is_some_and(|got| got == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::{Read, Write}, net::TcpListener};

    /// A stand-in engine on a spare port can't be tested (the port is fixed),
    /// but the check must refuse something that answers without a proof.
    #[test]
    fn a_listener_that_cannot_prove_itself_is_refused() {
        // Nothing (or an impostor) on the engine's port: no proof, no token.
        if TcpListener::bind(("127.0.0.1", ENGINE_PORT)).is_ok() {
            // Port free right now: bind it ourselves as the impostor.
            let listener = TcpListener::bind(("127.0.0.1", ENGINE_PORT)).unwrap();
            let t = std::thread::spawn(move || {
                if let Ok((mut c, _)) = listener.accept() {
                    let mut buf = [0u8; 512];
                    let _ = c.read(&mut buf);
                    let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"status\":\"ok\",\"name\":\"I.R.I.S. engine\"}");
                }
            });
            assert!(!engine_knows_token("iris_real_token"));
            let _ = t.join();
        } else {
            assert!(!engine_knows_token("iris_definitely_not_the_token"));
        }
    }
}
