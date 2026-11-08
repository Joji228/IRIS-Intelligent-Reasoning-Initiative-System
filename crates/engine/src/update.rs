//! Updates from inside the app.
//!
//! The engine looks on GitHub for a newer *published* release of I.R.I.S. (drafts
//! and pre-releases are invisible to it). The app shows "version X is available";
//! when the user presses Update, the engine downloads that release's Windows
//! installer, checks it against the SHA-256 checksum GitHub lists for the file, and
//! hands it to a small script (`update-apply.ps1`) that closes the app, runs the
//! installer quietly, and starts it again. Chats, agents and settings are untouched.
//!
//! What it will and won't do:
//! - It only ever talks to the project's own repository, and only accepts download
//!   addresses of the exact form `https://github.com/<repo>/releases/download/v<version>/<file>`.
//! - Nothing a client sends chooses what gets installed: the app can only say "update".
//! - A file whose checksum doesn't match, or that has no checksum, is never run.
//! - The phone app uses the same answer (`GET /v1/update`) to learn about new APKs;
//!   it downloads and checks the APK itself.

use std::{
    path::Path,
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::api::AppState;

/// The project's repository; `IRIS_UPDATE_REPO` points a fork at its own releases.
const REPO: &str = "Joji228/IRIS-Intelligent-Reasoning-Initiative-System";
/// How long an answer from GitHub is reused.
const CACHE_FOR: Duration = Duration::from_secs(30 * 60);
/// Background checks happen this often (when automatic checks are on).
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// No installer is anywhere near this; anything bigger is refused.
const MAX_DOWNLOAD: u64 = 200 * 1024 * 1024;

fn repo() -> String {
    std::env::var("IRIS_UPDATE_REPO").ok().filter(|r| r.split('/').count() == 2).unwrap_or_else(|| REPO.to_owned())
}

/// A file attached to a release.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// GitHub's SHA-256 of the file, lowercase hex. Without it the file is not trusted.
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub published_at: String,
    pub page: String,
    pub windows: Option<Asset>,
    pub android: Option<Asset>,
}

/// What `GET /v1/update` answers.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    /// The engine's own version.
    pub current: String,
    /// The newest published release, if GitHub could be asked.
    pub latest: Option<Release>,
    /// `latest` is newer than `current`.
    pub available: bool,
    /// When GitHub was last asked (ms since 1970), 0 if never.
    pub checked_at: i64,
    /// Why the last check failed.
    pub error: Option<String>,
    /// Whether the engine checks for updates by itself.
    pub auto_check: bool,
    /// An update is being downloaded or installed right now.
    pub install: Install,
    /// How the last update went (written by the install script), if there was one.
    pub last_update: Option<Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Install {
    /// "" (nothing), "downloading", "installing" or "failed".
    pub phase: String,
    pub percent: u8,
    pub error: Option<String>,
}

struct Checked {
    at: Instant,
    at_ms: i64,
    result: Result<Release, String>,
}

static CACHE: Mutex<Option<Checked>> = Mutex::new(None);
static INSTALL: Mutex<Install> = Mutex::new(Install { phase: String::new(), percent: 0, error: None });

// ---- reading GitHub's answer -------------------------------------------------------

/// `"v0.2.1"` → `(0, 2, 1)`; anything else → None.
fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.trim().trim_start_matches('v').split('.');
    let triple = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some(triple)
}

/// Whether `latest` is a higher version than `current`.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(l), Some(c)) if l > c)
}

fn sha_of(digest: &Value) -> Option<String> {
    let hex = digest.as_str()?.strip_prefix("sha256:")?.to_ascii_lowercase();
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit())).then_some(hex)
}

/// The release described by one `releases/latest` answer from GitHub.
fn parse_release(v: &Value, repo: &str) -> Option<Release> {
    if v["draft"].as_bool().unwrap_or(false) || v["prerelease"].as_bool().unwrap_or(false) {
        return None;
    }
    let tag = v["tag_name"].as_str()?;
    parse_version(tag)?;
    let version = tag.trim_start_matches('v').to_owned();
    let asset = |name: String| -> Option<Asset> {
        let a = v["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name.as_str()))?;
        // Only the address this file must have on GitHub; nothing else from the answer is followed.
        let url = format!("https://github.com/{repo}/releases/download/v{version}/{name}");
        if a["browser_download_url"].as_str() != Some(url.as_str()) {
            return None;
        }
        Some(Asset { name, url, size: a["size"].as_u64().unwrap_or(0), sha256: sha_of(&a["digest"]) })
    };
    let page = v["html_url"].as_str().filter(|u| u.starts_with(&format!("https://github.com/{repo}/"))).unwrap_or_default();
    Some(Release {
        windows: asset(format!("IRIS-Setup-x64-v{version}.exe")),
        android: asset(format!("IRIS-Android-v{version}.apk")),
        notes: crate::tools::truncate(v["body"].as_str().unwrap_or_default().trim(), 2_000),
        published_at: v["published_at"].as_str().unwrap_or_default().to_owned(),
        page: page.to_owned(),
        version,
    })
}

async fn fetch_latest(http: &reqwest::Client) -> Result<Release, String> {
    let repo = repo();
    let base = std::env::var("IRIS_UPDATE_API").unwrap_or_else(|_| "https://api.github.com".into());
    let resp = http
        .get(format!("{base}/repos/{repo}/releases/latest"))
        .header("User-Agent", concat!("iris-updater/", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("couldn't reach GitHub: {e}"))?;
    match resp.status().as_u16() {
        200 => {}
        404 => return Err("there is no published release yet".into()),
        403 | 429 => return Err("GitHub is limiting requests right now; try again later".into()),
        s => return Err(format!("GitHub answered {s}")),
    }
    let v: Value = resp.json().await.map_err(|e| format!("GitHub's answer wasn't readable: {e}"))?;
    parse_release(&v, &repo).ok_or_else(|| "the newest release on GitHub isn't in the expected form".to_owned())
}

// ---- the answer for clients --------------------------------------------------------

fn last_update(data_dir: &Path) -> Option<Value> {
    serde_json::from_str(std::fs::read_to_string(data_dir.join("updates").join("update-result.json")).ok()?.trim_start_matches('\u{feff}')).ok()
}

/// Asks GitHub (or reuses a recent answer, unless `force`) and describes where things stand.
pub async fn status(st: &AppState, force: bool) -> Status {
    let fresh = {
        let cache = CACHE.lock().unwrap();
        cache.as_ref().is_some_and(|c| c.at.elapsed() < CACHE_FOR)
    };
    if force || !fresh {
        let result = fetch_latest(&st.http).await;
        *CACHE.lock().unwrap() = Some(Checked { at: Instant::now(), at_ms: iris_core::now_ms(), result });
    }
    let cache = CACHE.lock().unwrap();
    let current = env!("CARGO_PKG_VERSION").to_owned();
    let (latest, error, checked_at) = match cache.as_ref() {
        Some(c) => match &c.result {
            Ok(r) => (Some(r.clone()), None, c.at_ms),
            Err(e) => (None, Some(e.clone()), c.at_ms),
        },
        None => (None, None, 0),
    };
    let available = latest.as_ref().is_some_and(|r| is_newer(&r.version, &current));
    Status {
        available,
        latest,
        error,
        checked_at,
        auto_check: crate::config::load(&st.data_dir).map(|c| c.update.check).unwrap_or(true),
        install: INSTALL.lock().unwrap().clone(),
        last_update: last_update(&st.data_dir),
        current,
    }
}

/// Looks for updates now and then every few hours, unless the user turned that off.
pub async fn run_forever(st: AppState) {
    tokio::time::sleep(Duration::from_secs(90)).await;
    loop {
        if crate::config::load(&st.data_dir).map(|c| c.update.check).unwrap_or(true) {
            let s = status(&st, true).await;
            match (&s.latest, &s.error) {
                (Some(r), _) if s.available => info!("I.R.I.S. {} is available (this is {})", r.version, s.current),
                (_, Some(e)) => warn!("update check: {e}"),
                _ => {}
            }
        }
        tokio::time::sleep(CHECK_EVERY).await;
    }
}

// ---- installing ---------------------------------------------------------------------

fn set_install(phase: &str, percent: u8, error: Option<String>) {
    *INSTALL.lock().unwrap() = Install { phase: phase.to_owned(), percent, error };
}

/// Downloads `url` to `dest`, refusing anything over `max` bytes or whose SHA-256 isn't
/// `expected` (lowercase hex). A file that fails the check is deleted.
async fn download_verified(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected: &str,
    max: u64,
    progress: impl Fn(u8),
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let mut resp = http
        .get(url)
        .header("User-Agent", concat!("iris-updater/", env!("CARGO_PKG_VERSION")))
        .send()
        .await
        .map_err(|e| format!("the download failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("the download failed: GitHub answered {}", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);
    if total > max {
        return Err("the download is far bigger than an installer should be".into());
    }
    let mut file = tokio::fs::File::create(dest).await.map_err(|e| format!("can't save the download: {e}"))?;
    let (mut hash, mut got) = (Sha256::new(), 0u64);
    let outcome: Result<(), String> = async {
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("the download broke off: {e}"))? {
            got += chunk.len() as u64;
            if got > max {
                return Err("the download is far bigger than an installer should be".into());
            }
            hash.update(&chunk);
            file.write_all(&chunk).await.map_err(|e| format!("can't save the download: {e}"))?;
            if total > 0 {
                progress((got * 100 / total).min(100) as u8);
            }
        }
        file.flush().await.map_err(|e| format!("can't save the download: {e}"))
    }
    .await;
    drop(file);
    let actual = hex::encode(hash.finalize());
    let result = outcome.and_then(|()| {
        if actual == expected {
            Ok(())
        } else {
            Err("the downloaded file doesn't match the checksum GitHub lists for it, so it was thrown away".into())
        }
    });
    if result.is_err() {
        let _ = tokio::fs::remove_file(dest).await;
    }
    result
}

/// Why an install can't start right now, or what to do.
pub enum Refusal {
    /// Something the user can fix or override ("agents are working").
    Busy(String),
    /// Nothing to install, or not possible here.
    Cannot(String),
}

/// Starts an update in the background: download, check, hand over to the install script.
/// Returns once it is under way; `status()` reports progress.
pub async fn start_install(st: &AppState, force: bool) -> Result<(), Refusal> {
    if !cfg!(windows) {
        return Err(Refusal::Cannot("updating from inside the app works on Windows; on other systems, install the new version yourself".into()));
    }
    if INSTALL.lock().unwrap().phase == "downloading" || INSTALL.lock().unwrap().phase == "installing" {
        return Err(Refusal::Cannot("an update is already under way".into()));
    }
    let s = status(st, true).await;
    let release = match (s.available, s.latest) {
        (true, Some(r)) => r,
        _ => return Err(Refusal::Cannot(s.error.unwrap_or_else(|| "I.R.I.S. is already up to date".into()))),
    };
    let Some(asset) = release.windows.clone() else {
        return Err(Refusal::Cannot("that release has no Windows installer".into()));
    };
    let Some(sha) = asset.sha256.clone() else {
        return Err(Refusal::Cannot("GitHub lists no checksum for the installer, so it can't be checked and won't be installed".into()));
    };
    let working = st.runs.lock().unwrap().len();
    if working > 0 && !force {
        return Err(Refusal::Busy(format!(
            "{working} {} working. Updating closes I.R.I.S. for a minute or two and stops anything unfinished.",
            if working == 1 { "agent is" } else { "agents are" }
        )));
    }

    set_install("downloading", 0, None);
    let (http, dir, version) = (st.http.clone(), st.data_dir.join("updates"), release.version.clone());
    tokio::spawn(async move {
        let outcome = async {
            std::fs::create_dir_all(&dir).map_err(|e| format!("can't make the updates folder: {e}"))?;
            let installer = dir.join(&asset.name);
            download_verified(&http, &asset.url, &installer, &sha, MAX_DOWNLOAD, |p| set_install("downloading", p, None)).await?;
            info!("update {version}: downloaded and checked {}", asset.name);
            set_install("installing", 100, None);
            launch_installer(&dir, &installer, &version)
        }
        .await;
        if let Err(e) = outcome {
            warn!("update failed: {e}");
            set_install("failed", 0, Some(e));
        }
    });
    Ok(())
}

/// Starts the install script, detached so it outlives this engine.
fn launch_installer(dir: &Path, installer: &Path, version: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let script = dir.join("update-apply.ps1");
        std::fs::write(&script, include_str!("update-apply.ps1")).map_err(|e| format!("can't write the install script: {e}"))?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let desktop = exe.parent().map(|p| p.join("iris-desktop.exe")).unwrap_or_default();
        let result = dir.join("update-result.json");
        let _ = std::fs::remove_file(&result);
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
            .arg(&script)
            .arg("-Installer")
            .arg(installer)
            .arg("-Desktop")
            .arg(&desktop)
            .arg("-Result")
            .arg(&result)
            .arg("-Version")
            .arg(version)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            // no console window, and its own process group
            .creation_flags(0x0800_0000 | 0x0000_0200)
            .spawn()
            .map_err(|e| format!("can't start the install script: {e}"))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (dir, installer, version);
        Err("updating from inside the app works on Windows".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SHA: &str = "6c75e71fcd5259ab4e843d8f7c15817abbfe92288fdfe9bc6bce2abd1495d8fc";

    fn release_json(tag: &str) -> Value {
        let v = tag.trim_start_matches('v');
        let url = |name: &str| format!("https://github.com/{REPO}/releases/download/{tag}/{name}");
        json!({
            "tag_name": tag, "draft": false, "prerelease": false, "published_at": "2026-11-08T16:00:00Z",
            "html_url": format!("https://github.com/{REPO}/releases/tag/{tag}"), "body": "What's new",
            "assets": [
                { "name": format!("IRIS-Setup-x64-v{v}.exe"), "size": 7255131, "digest": format!("sha256:{SHA}"),
                  "browser_download_url": url(&format!("IRIS-Setup-x64-v{v}.exe")) },
                { "name": format!("IRIS-Android-v{v}.apk"), "size": 3537729, "digest": format!("sha256:{}", SHA.to_uppercase()),
                  "browser_download_url": url(&format!("IRIS-Android-v{v}.apk")) },
            ]
        })
    }

    #[test]
    fn versions_compare_by_number_not_text() {
        assert!(is_newer("0.2.1", "0.2.0"));
        assert!(is_newer("v0.10.0", "0.9.9"), "10 is more than 9");
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.2.0", "0.2.0"));
        assert!(!is_newer("0.1.9", "0.2.0"));
        assert!(!is_newer("garbage", "0.2.0") && !is_newer("0.2.0", "garbage"));
        assert!(!is_newer("0.2.1-beta", "0.2.0"), "only plain x.y.z releases count");
    }

    #[test]
    fn reads_a_release_and_its_checksums() {
        let r = parse_release(&release_json("v0.2.1"), REPO).unwrap();
        assert_eq!(r.version, "0.2.1");
        let w = r.windows.unwrap();
        assert_eq!((w.name.as_str(), w.sha256.as_deref()), ("IRIS-Setup-x64-v0.2.1.exe", Some(SHA)));
        assert_eq!(r.android.unwrap().sha256.as_deref(), Some(SHA), "checksums are compared in lowercase");
        assert!(r.page.ends_with("/releases/tag/v0.2.1"));
    }

    #[test]
    fn only_the_projects_own_download_addresses_are_accepted() {
        // A download address pointing anywhere else is ignored, so it can never be fetched.
        let mut v = release_json("v0.2.1");
        v["assets"][0]["browser_download_url"] = json!("https://evil.example/IRIS-Setup-x64-v0.2.1.exe");
        assert!(parse_release(&v, REPO).unwrap().windows.is_none());
        let mut v = release_json("v0.2.1");
        v["assets"][0]["browser_download_url"] = json!("https://github.com/someone-else/repo/releases/download/v0.2.1/IRIS-Setup-x64-v0.2.1.exe");
        assert!(parse_release(&v, REPO).unwrap().windows.is_none());
        // A file with a different name isn't the installer.
        let mut v = release_json("v0.2.1");
        v["assets"][0]["name"] = json!("other.exe");
        assert!(parse_release(&v, REPO).unwrap().windows.is_none());
        // No usable checksum: the file is listed but can't be installed.
        let mut v = release_json("v0.2.1");
        v["assets"][0]["digest"] = json!(null);
        assert_eq!(parse_release(&v, REPO).unwrap().windows.unwrap().sha256, None);
        v["assets"][0]["digest"] = json!("sha256:not-hex");
        assert_eq!(parse_release(&v, REPO).unwrap().windows.unwrap().sha256, None);
    }

    #[test]
    fn drafts_and_odd_tags_are_ignored() {
        let mut v = release_json("v0.2.1");
        v["draft"] = json!(true);
        assert!(parse_release(&v, REPO).is_none());
        let mut v = release_json("v0.2.1");
        v["prerelease"] = json!(true);
        assert!(parse_release(&v, REPO).is_none());
        assert!(parse_release(&release_json("nightly"), REPO).is_none());
        assert!(parse_release(&json!({}), REPO).is_none());
    }

    /// A one-shot local web server that answers every request with `body`.
    async fn serve(body: Vec<u8>) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut conn, _)) = listener.accept().await {
                let body = body.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = conn.read(&mut buf).await;
                    let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                    let _ = conn.write_all(head.as_bytes()).await;
                    let _ = conn.write_all(&body).await;
                });
            }
        });
        format!("http://{addr}/file")
    }

    #[tokio::test]
    async fn a_download_is_kept_only_if_its_checksum_matches() {
        let data = b"pretend installer".repeat(1000);
        let url = serve(data.clone()).await;
        let right = hex::encode(Sha256::digest(&data));
        let dir = std::env::temp_dir().join(format!("iris-upd-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let http = reqwest::Client::new();

        let seen = std::sync::atomic::AtomicU8::new(0);
        let good = dir.join("good.exe");
        download_verified(&http, &url, &good, &right, 10_000_000, |p| seen.store(p, std::sync::atomic::Ordering::Relaxed)).await.unwrap();
        assert_eq!(std::fs::read(&good).unwrap(), data);
        assert_eq!(seen.load(std::sync::atomic::Ordering::Relaxed), 100);

        // The wrong checksum: refused, and nothing is left on disk to be run.
        let bad = dir.join("bad.exe");
        let wrong = "0".repeat(64);
        let err = download_verified(&http, &url, &bad, &wrong, 10_000_000, |_| {}).await.unwrap_err();
        assert!(err.contains("doesn't match"), "{err}");
        assert!(!bad.exists());

        // Far bigger than allowed: refused as well.
        let big = dir.join("big.exe");
        let err = download_verified(&http, &url, &big, &right, 100, |_| {}).await.unwrap_err();
        assert!(err.contains("bigger"), "{err}");
        assert!(!big.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
