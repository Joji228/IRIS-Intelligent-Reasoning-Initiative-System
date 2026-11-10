//! Engine configuration: `iris.toml` in the data directory.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::info;

/// Written on first start. Keys never go in this file directly unless you
/// choose `api_key`; prefer `api_key_env` or `api_key_file`.
const DEFAULT_CONFIG: &str = r#"# I.R.I.S. engine configuration

[server]
# Address the API listens on. The IRIS_ADDR environment variable overrides it.
addr = "0.0.0.0:7700"

# Any OpenAI-compatible provider works. The API key can come from:
#   api_key      = "sk-..."              (inline; avoid if this file is shared)
#   api_key_env  = "DEEPSEEK_API_KEY"    (environment variable)
#   api_key_file = 'C:\path\to\key.env'  (bare key, or NAME=value lines)
[[providers]]
id = "deepseek"
name = "DeepSeek"
base_url = "https://api.deepseek.com"
api_key_env = "DEEPSEEK_API_KEY"
default_model = "deepseek-flash"

# [[providers]]
# id = "openai"
# name = "OpenAI"
# base_url = "https://api.openai.com/v1"
# api_key_env = "OPENAI_API_KEY"
# default_model = "gpt-4o-mini"

# [[providers]]
# id = "ollama"
# name = "Ollama (local)"
# base_url = "http://localhost:11434/v1"
# default_model = "llama3.2"
"#;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub defaults: DefaultsConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub style: StyleConfig,
    #[serde(default)]
    pub link: crate::link::LinkConfig,
    #[serde(default)]
    pub update: UpdateConfig,
}

/// `[update]` in iris.toml: whether the engine looks on GitHub for new versions by itself.
/// (Updating itself always waits for the user to press the button.)
#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "yes")]
    pub check: bool,
}

fn yes() -> bool {
    true
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self { check: true }
    }
}

/// Saves `[update] check`.
pub fn set_update_check(dir: &Path, check: bool) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("update") {
        doc["update"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["update"]["check"] = toml_edit::value(check);
    write_doc(&path, &doc)
}

/// `[style]` in iris.toml: how agents talk to the user. `preset` is one of
/// [`STYLE_PRESETS`]; `custom` is free-form extra instructions.
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct StyleConfig {
    #[serde(default)]
    pub preset: String,
    #[serde(default)]
    pub custom: String,
}

/// Response styles: (id, instructions added to the system prompt).
pub const STYLE_PRESETS: &[(&str, &str)] = &[
    ("balanced", ""),
    (
        "concise",
        "Straight to the point. Give the answer in the first sentence and stop when it's answered: usually \
         one to three sentences, or at most five short bullets. No greetings, filler, background, caveats, \
         follow-up offers or recaps; add detail only when the user asks for it.",
    ),
    (
        "analyst",
        "Be thorough and analytical. Break the question down, look at it from several angles, state your \
         assumptions, weigh trade-offs and risks, point out what is uncertain, and end with a clear conclusion \
         or recommendation.",
    ),
    (
        "witty",
        "Be witty and playful: light banter, dry humor and the occasional friendly tease. Keep the facts \
         correct and the answer useful; never let a joke get in the way of the help.",
    ),
    (
        "teacher",
        "Explain like a patient teacher: plain words, step by step, with a simple example or analogy. Define \
         jargon the first time it appears, and end with a one-line recap.",
    ),
    (
        "professional",
        "Be polished and professional: neutral, precise and well structured, with headings or numbered points \
         where they help. Suitable to forward to a colleague or client.",
    ),
];

impl StyleConfig {
    /// The text added to agents' instructions, if any.
    pub fn instructions(&self) -> Option<String> {
        let preset = STYLE_PRESETS.iter().find(|(id, _)| *id == self.preset).map(|(_, t)| *t).unwrap_or_default();
        let parts: Vec<&str> = [preset, self.custom.trim()].into_iter().filter(|s| !s.is_empty()).collect();
        (!parts.is_empty()).then(|| parts.join("
"))
    }
}

/// Saves `[link]`: the relay's address and whether the link is on.
pub fn set_link(dir: &Path, link: &crate::link::LinkConfig) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("link") {
        doc["link"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["link"]["url"] = toml_edit::value(&link.url);
    doc["link"]["enabled"] = toml_edit::value(link.enabled);
    write_doc(&path, &doc)
}

pub fn set_style(dir: &Path, style: &StyleConfig) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("style") {
        doc["style"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["style"]["preset"] = toml_edit::value(&style.preset);
    doc["style"]["custom"] = toml_edit::value(&style.custom);
    write_doc(&path, &doc)
}

/// `[limits]` in iris.toml: token caps across all agents; 0 means no limit.
#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct LimitsConfig {
    #[serde(default)]
    pub daily_tokens: u64,
    #[serde(default)]
    pub weekly_tokens: u64,
}

pub fn set_limits(dir: &Path, limits: &LimitsConfig) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("limits") {
        doc["limits"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["limits"]["daily_tokens"] = toml_edit::value(limits.daily_tokens as i64);
    doc["limits"]["weekly_tokens"] = toml_edit::value(limits.weekly_tokens as i64);
    write_doc(&path, &doc)
}

/// Start of today and of this week (Monday), local time, in ms since epoch.
pub fn usage_windows() -> (i64, i64) {
    use chrono::{Datelike, Duration, Local, TimeZone};
    let now = Local::now();
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).expect("valid time");
    let day = Local.from_local_datetime(&midnight).earliest().map(|d| d.timestamp_millis()).unwrap_or(0);
    let monday = midnight - Duration::days(i64::from(now.weekday().num_days_from_monday()));
    let week = Local.from_local_datetime(&monday).earliest().map(|d| d.timestamp_millis()).unwrap_or(0);
    (day, week)
}

/// `[defaults]` in iris.toml: the model every agent uses unless it has its
/// own, plus an optional fallback used when that model's provider fails.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DefaultsConfig {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    /// Override for the model's context window (tokens), for models the
    /// engine doesn't know.
    #[serde(default)]
    pub context_window: Option<u64>,
    #[serde(default)]
    pub fallback_enabled: bool,
    #[serde(default)]
    pub fallback_provider: Option<String>,
    #[serde(default)]
    pub fallback_model: Option<String>,
    /// How hard agents think unless they have their own level ("off", "low",
    /// "medium", "high", "max"). Unset means the provider's own default.
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// The auto mode new agents start with: "off", "careful" or "full".
    #[serde(default)]
    pub auto_mode: Option<String>,
}

/// Saves the default reasoning level; an empty `level` means the provider's default.
pub fn set_reasoning_effort(dir: &Path, level: &str) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("defaults") {
        doc["defaults"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["defaults"]["reasoning_effort"] = toml_edit::value(level);
    write_doc(&path, &doc)
}

/// Saves the auto mode new agents start with.
pub fn set_auto_mode(dir: &Path, mode: &str) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("defaults") {
        doc["defaults"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["defaults"]["auto_mode"] = toml_edit::value(mode);
    write_doc(&path, &doc)
}

pub fn set_fallback(dir: &Path, enabled: bool, provider: &str, model: &str) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("defaults") {
        doc["defaults"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["defaults"]["fallback_enabled"] = toml_edit::value(enabled);
    doc["defaults"]["fallback_provider"] = toml_edit::value(provider);
    doc["defaults"]["fallback_model"] = toml_edit::value(model);
    write_doc(&path, &doc)
}

/// Context windows of well-known models (tokens). DeepSeek V4 (flash and
/// pro) take 1M per DeepSeek's pricing docs.
pub fn known_context_window(model: &str) -> Option<u64> {
    let m = model.to_ascii_lowercase();
    if m.starts_with("deepseek-flash") || m.starts_with("deepseek-v4") || m == "deepseek-chat" || m == "deepseek-reasoner" {
        Some(1_000_000)
    } else if m.starts_with("gemini") {
        Some(1_000_000)
    } else if m.starts_with("gpt-5") {
        Some(400_000)
    } else if m.starts_with("claude") {
        Some(200_000)
    } else if m.starts_with("gpt-4o") || m.starts_with("gpt-4.1") || m.starts_with("o3") || m.starts_with("o4") {
        Some(128_000)
    } else {
        None
    }
}

pub fn set_default_model(dir: &Path, provider: &str, model: &str) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("defaults") {
        doc["defaults"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["defaults"]["provider"] = toml_edit::value(provider);
    doc["defaults"]["model"] = toml_edit::value(model);
    write_doc(&path, &doc)
}

/// `[search]` in iris.toml. `provider`: "auto" (Tavily when a key is stored,
/// else DuckDuckGo), "tavily" or "duckduckgo". The Tavily key lives in
/// `secrets/tavily.key`, never in iris.toml.
#[derive(Debug, Default, Deserialize)]
pub struct SearchConfig {
    #[serde(default)]
    pub provider: Option<String>,
}

pub fn tavily_key_path(dir: &Path) -> PathBuf {
    dir.join("secrets").join("tavily.key")
}

/// What the search settings resolve to, for the app.
#[derive(Debug, serde::Serialize)]
pub struct SearchStatus {
    pub provider: String,
    pub has_tavily_key: bool,
    /// A DeepSeek provider with a key is set up (its Search API can be used).
    pub has_deepseek_key: bool,
    /// The backend actually used: "deepseek", "tavily" or "duckduckgo".
    pub active: String,
}

/// The key of the DeepSeek provider (Settings → AI providers), if there is one.
fn deepseek_key(cfg: &Config) -> Option<String> {
    cfg.providers
        .iter()
        .find(|p| p.base_url.contains("api.deepseek.com"))
        .and_then(|p| p.resolve_api_key().ok().flatten())
}

pub fn search_status(dir: &Path, cfg: &Config) -> SearchStatus {
    let provider = cfg.search.provider.clone().unwrap_or_else(|| "auto".into());
    let has_tavily_key = fs::read_to_string(tavily_key_path(dir)).is_ok_and(|k| !k.trim().is_empty());
    let has_deepseek_key = deepseek_key(cfg).is_some();
    // DeepSeek is paid per search, so only an explicit choice uses it.
    let active = if provider == "deepseek" && has_deepseek_key {
        "deepseek"
    } else if provider != "duckduckgo" && has_tavily_key {
        "tavily"
    } else {
        "duckduckgo"
    };
    SearchStatus { provider, has_tavily_key, has_deepseek_key, active: active.into() }
}

pub fn search_settings(dir: &Path, cfg: &Config) -> crate::tools::SearchSettings {
    let status = search_status(dir, cfg);
    let tavily_key = (status.active == "tavily")
        .then(|| fs::read_to_string(tavily_key_path(dir)).ok().and_then(|k| parse_key_file(&k)))
        .flatten();
    let deepseek_key = (status.active == "deepseek").then(|| deepseek_key(cfg)).flatten();
    crate::tools::SearchSettings { tavily_key, deepseek_key }
}

/// Saves the search provider and (optionally) the Tavily key:
/// `None` keeps it, `Some("")` removes it.
pub fn set_search(dir: &Path, provider: &str, tavily_key: Option<&str>) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("search") {
        doc["search"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    doc["search"]["provider"] = toml_edit::value(provider);
    write_doc(&path, &doc)?;
    if let Some(key) = tavily_key {
        let key_path = tavily_key_path(dir);
        if key.trim().is_empty() {
            let _ = fs::remove_file(&key_path);
        } else {
            fs::create_dir_all(key_path.parent().expect("has parent"))?;
            fs::write(&key_path, format!("{}\n", key.trim()))?;
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_addr")]
    pub addr: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { addr: default_addr() }
    }
}

fn default_addr() -> String {
    "0.0.0.0:7700".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub api_key_file: Option<PathBuf>,
}

impl ProviderConfig {
    /// The API key, from the first of `api_key`, `api_key_env`, `api_key_file`
    /// that yields one. `None` is valid for keyless local servers.
    pub fn resolve_api_key(&self) -> Result<Option<String>> {
        if let Some(key) = self.api_key.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
            return Ok(Some(key.to_owned()));
        }
        if let Some(var) = &self.api_key_env {
            if let Ok(v) = std::env::var(var) {
                if !v.trim().is_empty() {
                    return Ok(Some(v.trim().to_owned()));
                }
            }
        }
        if let Some(path) = &self.api_key_file {
            let text = fs::read_to_string(path)
                .with_context(|| format!("reading api_key_file {}", path.display()))?;
            return Ok(parse_key_file(&text));
        }
        Ok(None)
    }
}

/// Accepts either a bare key or `NAME=value` lines; the first value wins.
pub fn parse_key_file(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.split_once('=').map_or(l, |(_, v)| v).trim().trim_matches(['"', '\'']))
        .find(|v| !v.is_empty())
        .map(str::to_owned)
}

/// The desktop app's identifier; the default data dir is named after it so it
/// never collides with the app's install folder (`%LOCALAPPDATA%\IRIS`).
pub const APP_ID: &str = "dev.iris.desktop";

/// Where the database, config and tokens live: `IRIS_DATA_DIR`, or the
/// platform's local data dir (`%LOCALAPPDATA%\dev.iris.desktop` on Windows).
pub fn data_dir() -> PathBuf {
    std::env::var_os("IRIS_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join(APP_ID))
}

pub fn config_path(dir: &Path) -> PathBuf {
    dir.join("iris.toml")
}

pub fn load(dir: &Path) -> Result<Config> {
    let path = config_path(dir);
    if !path.exists() {
        fs::write(&path, DEFAULT_CONFIG).with_context(|| format!("writing {}", path.display()))?;
        info!("wrote default config to {}", path.display());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// A provider as edited from the app.
#[derive(Debug, Deserialize)]
pub struct ProviderUpdate {
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    /// `None` leaves the key alone, `Some("")` removes it, anything else
    /// replaces it (stored in `secrets/<id>.key`, never in iris.toml).
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Provider ids end up in file names and URLs, so keep them simple.
pub fn valid_provider_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Adds or updates a `[[providers]]` entry in iris.toml, keeping the rest of
/// the file (comments included) untouched.
pub fn upsert_provider(dir: &Path, id: &str, update: &ProviderUpdate) -> Result<()> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    if !doc.contains_key("providers") {
        doc["providers"] = toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new());
    }
    let providers = doc["providers"]
        .as_array_of_tables_mut()
        .context("`providers` in iris.toml must be a list of [[providers]] tables")?;

    let existing = providers.iter().position(|t| t.get("id").and_then(|v| v.as_str()) == Some(id));
    let idx = match existing {
        Some(i) => i,
        None => {
            let mut table = toml_edit::Table::new();
            table["id"] = toml_edit::value(id);
            providers.push(table);
            providers.len() - 1
        }
    };
    let table = providers.get_mut(idx).expect("index is valid");
    table["name"] = toml_edit::value(update.name.trim());
    table["base_url"] = toml_edit::value(update.base_url.trim());
    table["default_model"] = toml_edit::value(update.default_model.trim());

    if let Some(key) = &update.api_key {
        for field in ["api_key", "api_key_env", "api_key_file"] {
            table.remove(field);
        }
        let key_file = dir.join("secrets").join(format!("{id}.key"));
        if key.trim().is_empty() {
            let _ = fs::remove_file(&key_file);
        } else {
            fs::create_dir_all(key_file.parent().expect("has parent"))?;
            fs::write(&key_file, format!("{}\n", key.trim()))?;
            table["api_key_file"] = toml_edit::value(key_file.display().to_string());
        }
    }
    write_doc(&path, &doc)
}

/// Removes a provider and its stored key. Returns false if it didn't exist.
pub fn remove_provider(dir: &Path, id: &str) -> Result<bool> {
    let path = config_path(dir);
    let mut doc = read_doc(&path)?;
    let Some(providers) = doc.get_mut("providers").and_then(|p| p.as_array_of_tables_mut()) else {
        return Ok(false);
    };
    let Some(idx) = providers.iter().position(|t| t.get("id").and_then(|v| v.as_str()) == Some(id)) else {
        return Ok(false);
    };
    providers.remove(idx);
    write_doc(&path, &doc)?;
    let _ = fs::remove_file(dir.join("secrets").join(format!("{id}.key")));
    Ok(true)
}

fn read_doc(path: &Path) -> Result<toml_edit::DocumentMut> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    text.parse().with_context(|| format!("parsing {}", path.display()))
}

/// Write via a temp file so a crash never leaves a half-written config.
fn write_doc(path: &Path, doc: &toml_edit::DocumentMut) -> Result<()> {
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, doc.to_string())?;
    fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_file_formats() {
        assert_eq!(parse_key_file("sk-abc\n").as_deref(), Some("sk-abc"));
        assert_eq!(parse_key_file("DEEPSEEK_API_KEY=sk-abc").as_deref(), Some("sk-abc"));
        assert_eq!(parse_key_file("# comment\nKEY = \"sk-abc\"\n").as_deref(), Some("sk-abc"));
        assert_eq!(parse_key_file("\n\n"), None);
    }

    #[test]
    fn provider_edit_roundtrip() {
        let dir = std::env::temp_dir().join(format!("iris-cfg-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        load(&dir).unwrap(); // writes the default config

        let update = |key: Option<&str>| ProviderUpdate {
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            default_model: "x/y".into(),
            api_key: key.map(str::to_owned),
        };
        upsert_provider(&dir, "openrouter", &update(Some("sk-test"))).unwrap();
        let cfg = load(&dir).unwrap();
        let p = cfg.providers.iter().find(|p| p.id == "openrouter").unwrap();
        assert_eq!(p.resolve_api_key().unwrap().as_deref(), Some("sk-test"));
        assert!(fs::read_to_string(config_path(&dir)).unwrap().contains("# I.R.I.S. engine configuration"),
            "comments survive edits");

        // None keeps the key; "" removes it.
        upsert_provider(&dir, "openrouter", &update(None)).unwrap();
        let cfg = load(&dir).unwrap();
        assert!(cfg.providers.iter().find(|p| p.id == "openrouter").unwrap().resolve_api_key().unwrap().is_some());
        upsert_provider(&dir, "openrouter", &update(Some(""))).unwrap();
        let cfg = load(&dir).unwrap();
        assert!(cfg.providers.iter().find(|p| p.id == "openrouter").unwrap().resolve_api_key().unwrap().is_none());

        assert!(remove_provider(&dir, "openrouter").unwrap());
        assert!(!remove_provider(&dir, "openrouter").unwrap());
        assert_eq!(load(&dir).unwrap().providers.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn limits_roundtrip_and_windows() {
        let dir = std::env::temp_dir().join(format!("iris-limits-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(load(&dir).unwrap().limits.daily_tokens, 0);
        set_limits(&dir, &LimitsConfig { daily_tokens: 500_000, weekly_tokens: 2_000_000 }).unwrap();
        let l = load(&dir).unwrap().limits;
        assert_eq!((l.daily_tokens, l.weekly_tokens), (500_000, 2_000_000));
        let (day, week) = usage_windows();
        let now = chrono::Local::now().timestamp_millis();
        assert!(week <= day && day <= now && now - day < 86_400_000 + 3_600_000);
        assert!(day - week < 7 * 86_400_000 + 3_600_000);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_model_roundtrip() {
        let dir = std::env::temp_dir().join(format!("iris-defaults-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        assert!(load(&dir).unwrap().defaults.model.is_none());
        set_default_model(&dir, "deepseek", "deepseek-flash").unwrap();
        let d = load(&dir).unwrap().defaults;
        assert_eq!((d.provider.as_deref(), d.model.as_deref()), (Some("deepseek"), Some("deepseek-flash")));
        assert!(d.reasoning_effort.is_none());
        set_reasoning_effort(&dir, "high").unwrap();
        let cfg = load(&dir).unwrap();
        assert_eq!(cfg.defaults.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(cfg.defaults.model.as_deref(), Some("deepseek-flash"), "the model survives");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn fallback_and_style_roundtrip() {
        let dir = std::env::temp_dir().join(format!("iris-fallback-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        let cfg = load(&dir).unwrap();
        assert!(!cfg.defaults.fallback_enabled);
        assert!(cfg.style.instructions().is_none());
        set_default_model(&dir, "deepseek", "deepseek-flash").unwrap();
        set_fallback(&dir, true, "openrouter", "x/y").unwrap();
        set_style(&dir, &StyleConfig { preset: "concise".into(), custom: "Call me Sam.".into() }).unwrap();
        let cfg = load(&dir).unwrap();
        assert!(cfg.defaults.fallback_enabled);
        assert_eq!(cfg.defaults.fallback_provider.as_deref(), Some("openrouter"));
        assert_eq!(cfg.defaults.model.as_deref(), Some("deepseek-flash"), "the default survives");
        let text = cfg.style.instructions().unwrap();
        assert!(text.starts_with("Straight to the point.") && text.ends_with("Call me Sam."));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn search_settings_roundtrip() {
        let dir = std::env::temp_dir().join(format!("iris-search-{}", uuid::Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();
        let cfg = load(&dir).unwrap();
        assert_eq!(search_status(&dir, &cfg).active, "duckduckgo");

        set_search(&dir, "auto", Some("tvly-test")).unwrap();
        let cfg = load(&dir).unwrap();
        let st = search_status(&dir, &cfg);
        assert!(st.has_tavily_key);
        assert_eq!(st.active, "tavily");
        assert_eq!(search_settings(&dir, &cfg).tavily_key.as_deref(), Some("tvly-test"));

        set_search(&dir, "duckduckgo", None).unwrap();
        let cfg = load(&dir).unwrap();
        assert_eq!(search_status(&dir, &cfg).active, "duckduckgo", "explicit choice wins over a stored key");
        assert!(search_settings(&dir, &cfg).tavily_key.is_none());

        set_search(&dir, "auto", Some("")).unwrap();
        assert!(!search_status(&dir, &load(&dir).unwrap()).has_tavily_key);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn provider_ids() {
        assert!(valid_provider_id("deepseek"));
        assert!(valid_provider_id("my-local_1"));
        assert!(!valid_provider_id("Bad Id"));
        assert!(!valid_provider_id("../x"));
        assert!(!valid_provider_id(""));
    }

    #[test]
    fn default_config_parses() {
        let cfg: Config = toml::from_str(DEFAULT_CONFIG).unwrap();
        assert_eq!(cfg.server.addr, "0.0.0.0:7700");
        assert_eq!(cfg.providers.len(), 1);
        assert_eq!(cfg.providers[0].id, "deepseek");
    }
}
