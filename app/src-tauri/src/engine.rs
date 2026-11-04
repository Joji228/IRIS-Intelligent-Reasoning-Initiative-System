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
