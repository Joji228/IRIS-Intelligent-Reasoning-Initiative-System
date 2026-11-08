//! The account vault: logins the user saves for agents to use on websites.
//! Stored in `secrets/vault.bin`, encrypted with Windows DPAPI (machine
//! scope, so the boot-time engine can read it too, plus an app-specific
//! entropy). Agents only ever see an account's name and site; the browser
//! fills the username and password in itself, and only on that site.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A saved login. `password` never leaves the engine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Account {
    pub id: String,
    /// What agents call it, e.g. "shop".
    pub name: String,
    /// The site it belongs to, e.g. "shop.example" (logins are only filled there).
    pub site: String,
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub notes: String,
    pub created_at: i64,
}

/// What the app and agents may see.
#[derive(Debug, Serialize)]
pub struct AccountInfo {
    pub id: String,
    pub name: String,
    pub site: String,
    pub username: String,
    pub has_password: bool,
    pub notes: String,
}

impl From<&Account> for AccountInfo {
    fn from(a: &Account) -> Self {
        Self {
            id: a.id.clone(),
            name: a.name.clone(),
            site: a.site.clone(),
            username: a.username.clone(),
            has_password: !a.password.is_empty(),
            notes: a.notes.clone(),
        }
    }
}

fn path(data_dir: &Path) -> PathBuf {
    data_dir.join("secrets").join("vault.bin")
}

/// "https://www.shop.example/login" → "shop.example".
pub fn normalize_site(site: &str) -> String {
    let s = site.trim().to_ascii_lowercase();
    let s = s.split("://").last().unwrap_or(&s);
    let host = s.split(['/', '?', '#', ':']).next().unwrap_or_default();
    host.trim_start_matches("www.").to_owned()
}

/// Whether a page on `host` belongs to `site` (the site itself or a subdomain).
pub fn host_matches(host: &str, site: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let site = normalize_site(site);
    !site.is_empty() && (host == site || host.ends_with(&format!(".{site}")))
}

/// Whether a saved login for `site` may be filled into the page now open at
/// `protocol` (`location.protocol`, e.g. "https:") on `host`: only on the
/// account's own site, and only over HTTPS, so a page that was served in plain
/// text (and could have been changed on the way) never receives a password.
pub fn login_allowed(protocol: &str, host: &str, site: &str) -> bool {
    protocol.eq_ignore_ascii_case("https:") && host_matches(host, site)
}

pub fn load(data_dir: &Path) -> Vec<Account> {
    let Ok(bytes) = std::fs::read(path(data_dir)) else { return Vec::new() };
    let Some(plain) = dpapi::unprotect(&bytes) else {
        tracing::warn!("the account vault couldn't be decrypted");
        return Vec::new();
    };
    serde_json::from_slice(&plain).unwrap_or_default()
}

pub fn save(data_dir: &Path, accounts: &[Account]) -> std::io::Result<()> {
    let plain = serde_json::to_vec(accounts).expect("serializable");
    let sealed = dpapi::protect(&plain).ok_or_else(|| std::io::Error::other("encrypting the vault failed"))?;
    let p = path(data_dir);
    std::fs::create_dir_all(p.parent().expect("has parent"))?;
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, sealed)?;
    std::fs::rename(tmp, p)
}

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData},
    };

    const LOCAL_MACHINE: u32 = 0x4;
    const UI_FORBIDDEN: u32 = 0x1;
    const ENTROPY: &[u8] = b"I.R.I.S. account vault v1";

    fn run(data: &[u8], protect: bool) -> Option<Vec<u8>> {
        let input = CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 };
        let entropy = CRYPT_INTEGER_BLOB { cbData: ENTROPY.len() as u32, pbData: ENTROPY.as_ptr() as *mut u8 };
        let mut out = CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() };
        // SAFETY: the blobs point at live buffers for the duration of the
        // call; the output buffer is allocated by Windows and freed below.
        let ok = unsafe {
            if protect {
                CryptProtectData(&input, std::ptr::null(), &entropy, std::ptr::null(), std::ptr::null(), LOCAL_MACHINE | UI_FORBIDDEN, &mut out)
            } else {
                CryptUnprotectData(&input, std::ptr::null_mut(), &entropy, std::ptr::null(), std::ptr::null(), UI_FORBIDDEN, &mut out)
            }
        };
        if ok == 0 || out.pbData.is_null() {
            return None;
        }
        // SAFETY: Windows filled `out` with cbData bytes at pbData.
        let bytes = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
        unsafe { LocalFree(out.pbData as _) };
        Some(bytes)
    }

    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, true)
    }

    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, false)
    }
}

/// Elsewhere (development only): not encrypted.
#[cfg(not(windows))]
mod dpapi {
    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }
    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sites_and_roundtrip() {
        assert_eq!(normalize_site("https://www.shop.example/login?x=1"), "shop.example");
        assert!(host_matches("www.shop.example", "shop.example"));
        assert!(host_matches("shop.example", "https://shop.example"));
        assert!(!host_matches("shop.example.evil.com", "shop.example"), "look-alike hosts don't match");
        assert!(!host_matches("notshop.example", "shop.example"));

        // Logins are only filled over HTTPS, on the account's own site.
        assert!(login_allowed("https:", "www.shop.example", "shop.example"));
        assert!(!login_allowed("http:", "shop.example", "shop.example"), "plain HTTP never gets a password");
        assert!(!login_allowed("file:", "shop.example", "shop.example"));
        assert!(!login_allowed("https:", "shop.example.evil.com", "shop.example"));
        assert!(!login_allowed("", "shop.example", "shop.example"));

        let dir = std::env::temp_dir().join(format!("iris-vault-{}", uuid::Uuid::new_v4().simple()));
        let a = Account {
            id: "1".into(),
            name: "shop".into(),
            site: "shop.example".into(),
            username: "me@example.com".into(),
            password: "s3cret".into(),
            notes: String::new(),
            created_at: 0,
        };
        save(&dir, std::slice::from_ref(&a)).unwrap();
        let raw = std::fs::read(path(&dir)).unwrap();
        if cfg!(windows) {
            assert!(!raw.windows(6).any(|w| w == b"s3cret"), "the file is encrypted");
        }
        assert_eq!(load(&dir), vec![a]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
