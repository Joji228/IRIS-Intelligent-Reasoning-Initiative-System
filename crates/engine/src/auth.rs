//! Device-token authentication.
//!
//! Every client (desktop app, phone, CLI) holds its own random token; the
//! database stores only its SHA-256 hash. HTTP clients send
//! `Authorization: Bearer <token>`; WebSocket clients, which can't set
//! headers from a browser, may pass `?token=<token>` instead.

use std::{fs, path::{Path, PathBuf}};

use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, Uri, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use iris_core::Store;
use sha2::{Digest, Sha256};
use tracing::error;

use crate::api::AppState;

pub fn new_token() -> String {
    let (a, b) = (uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
    format!("iris_{}{}", a.simple(), b.simple())
}

pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// What the real engine answers to `GET /health?nonce=…`: a hash that can only
/// be made by something that knows the local token. The desktop app checks it
/// before it hands that token to whatever is listening on the engine's port,
/// so another program that grabbed the port first never receives it.
pub fn proof(token: &str, nonce: &str) -> String {
    hex::encode(Sha256::digest(format!("iris-proof:{nonce}:{token}").as_bytes()))
}

pub const LOCAL_TOKEN_FILE: &str = "local-token.txt";

/// Makes sure `local-token.txt` in the data dir holds a valid token for the
/// device "This computer". The desktop app on the same machine reads it to
/// connect; being able to read the file already proves you are the local user.
/// Returns that device's id, and the token file's path if it had to be created.
pub fn ensure_local_token(store: &Store, data_dir: &Path) -> Result<(String, Option<PathBuf>)> {
    let path = data_dir.join(LOCAL_TOKEN_FILE);
    if let Ok(existing) = fs::read_to_string(&path) {
        if let Some(device) = store.authenticate_device(&hash_token(existing.trim()))? {
            return Ok((device.id, None));
        }
    }
    let token = new_token();
    let device = store.create_device("This computer", &hash_token(&token))?;
    fs::write(&path, format!("{token}\n"))?;
    Ok((device.id, Some(path)))
}

/// Who made a request that passed [`require_auth`] (handlers that keep a
/// connection open use it to notice when the device is removed).
#[derive(Clone)]
pub struct Authed {
    pub device_id: String,
    pub token_hash: String,
}

pub async fn require_auth(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let Some(token) = bearer(req.headers()).or_else(|| query_token(req.uri())) else {
        return (StatusCode::UNAUTHORIZED, "missing token").into_response();
    };
    let token_hash = hash_token(&token);
    match state.store.authenticate_device(&token_hash) {
        Ok(Some(device)) => {
            req.extensions_mut().insert(Authed { device_id: device.id, token_hash });
            next.run(req).await
        }
        Ok(None) => (StatusCode::UNAUTHORIZED, "invalid token").into_response(),
        Err(e) => {
            error!("auth lookup failed: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn bearer(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("Bearer ").map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

fn query_token(uri: &Uri) -> Option<String> {
    uri.query()?
        .split('&')
        .find_map(|pair| pair.strip_prefix("token="))
        .map(str::to_owned)
        .filter(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_hash_stably() {
        let (a, b) = (new_token(), new_token());
        assert_ne!(a, b);
        assert!(a.starts_with("iris_") && a.len() == 5 + 64);
        assert_eq!(hash_token(&a), hash_token(&a));
        assert_ne!(hash_token(&a), hash_token(&b));
    }

    #[test]
    fn proof_needs_the_token_and_the_nonce() {
        let p = proof("iris_secret", "abc123");
        assert_eq!(p.len(), 64);
        assert_eq!(p, proof("iris_secret", "abc123"));
        assert_ne!(p, proof("iris_other", "abc123"), "another token gives another proof");
        assert_ne!(p, proof("iris_secret", "abc124"), "a replayed proof fails for a new nonce");
        assert!(!p.contains("secret"));
    }

    #[test]
    fn token_from_query() {
        let uri: Uri = "/v1/events?x=1&token=abc".parse().unwrap();
        assert_eq!(query_token(&uri).as_deref(), Some("abc"));
        let uri: Uri = "/v1/events".parse().unwrap();
        assert_eq!(query_token(&uri), None);
    }
}
