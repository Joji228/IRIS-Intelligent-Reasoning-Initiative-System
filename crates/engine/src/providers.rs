//! The configured LLM providers, ready to use. Rebuilt from iris.toml whenever
//! providers are edited from the app.

use iris_providers::OpenAiCompatible;
use serde::Serialize;
use tracing::warn;

use crate::config::ProviderConfig;

pub struct Provider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    pub key_source: KeySource,
    pub client: OpenAiCompatible,
}

/// Where a provider's API key comes from; shown in the app instead of the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeySource {
    Inline,
    Env,
    File,
    /// No key configured, or the configured source yielded nothing.
    None,
}

/// What the API exposes about a provider (never the key).
#[derive(Serialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    pub has_key: bool,
    pub key_source: KeySource,
}

pub struct Providers(Vec<Provider>);

impl Providers {
    pub fn from_config(configs: &[ProviderConfig]) -> Self {
        let providers = configs
            .iter()
            .map(|c| {
                let key = c.resolve_api_key().unwrap_or_else(|e| {
                    warn!("provider {}: {e:#}", c.id);
                    None
                });
                // Same precedence as `resolve_api_key`: inline, then env, then file.
                let env_set = c
                    .api_key_env
                    .as_ref()
                    .and_then(|v| std::env::var(v).ok())
                    .is_some_and(|v| !v.trim().is_empty());
                let key_source = if key.is_none() {
                    KeySource::None
                } else if c.api_key.as_deref().is_some_and(|k| !k.trim().is_empty()) {
                    KeySource::Inline
                } else if env_set {
                    KeySource::Env
                } else {
                    KeySource::File
                };
                if key.is_none() {
                    warn!("provider {} has no API key configured", c.id);
                }
                Provider {
                    id: c.id.clone(),
                    name: c.name.clone(),
                    base_url: c.base_url.clone(),
                    default_model: c.default_model.clone(),
                    key_source,
                    client: OpenAiCompatible::new(c.base_url.clone(), key),
                }
            })
            .collect();
        Self(providers)
    }

    pub fn get(&self, id: &str) -> Option<&Provider> {
        self.0.iter().find(|p| p.id == id)
    }

    pub fn all(&self) -> &[Provider] {
        &self.0
    }

    pub fn first(&self) -> Option<&Provider> {
        self.0.first()
    }

    pub fn infos(&self) -> Vec<ProviderInfo> {
        self.0
            .iter()
            .map(|p| ProviderInfo {
                id: p.id.clone(),
                name: p.name.clone(),
                base_url: p.base_url.clone(),
                default_model: p.default_model.clone(),
                has_key: p.key_source != KeySource::None,
                key_source: p.key_source,
            })
            .collect()
    }
}
