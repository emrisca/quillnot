//! The API key, the settings file and the catalogue cache.
//!
//! The key goes to the OS keychain — Keychain on macOS, Credential Manager on
//! Windows — and is never written to disk by this app. Settings and the cached
//! model list live under the app's data directory, which is where M4's history
//! will go as well.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::KeyStatus;

const SERVICE: &str = "dev.emrys.quillnot.openrouter";
const ACCOUNT: &str = "api-key";
const SETTINGS_FILE: &str = "settings.json";
const MODELS_CACHE: &str = "catalog-models.json";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub default_model: Option<String>,
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("the OS key store is unavailable: {e}"))
}

pub fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn load_key() -> Option<String> {
    let entry = entry().ok()?;
    match entry.get_password() {
        Ok(key) if !key.trim().is_empty() => Some(key),
        _ => None,
    }
}

pub fn save_key(key: &str) -> Result<(), String> {
    entry()?
        .set_password(key)
        .map_err(|e| format!("could not write the key to the OS key store: {e}"))
}

pub fn clear_key() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) => Ok(()),
        // Nothing stored is the state we wanted anyway.
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(err) => Err(format!("could not remove the stored key: {err}")),
    }
}

/// `sk-or-…4f2a`: enough to tell two keys apart, useless to anyone who sees it.
pub fn hint(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 12 {
        return "…".to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

pub fn key_status() -> KeyStatus {
    match load_key() {
        Some(key) => KeyStatus {
            connected: true,
            hint: Some(hint(&key)),
        },
        None => KeyStatus {
            connected: false,
            hint: None,
        },
    }
}

pub fn read_settings(app: &AppHandle) -> Settings {
    data_dir(app)
        .ok()
        .and_then(|dir| std::fs::read(dir.join(SETTINGS_FILE)).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Where the last good model list is kept so the picker works without a network.
pub fn models_cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join(MODELS_CACHE))
}

#[cfg(test)]
mod tests {
    use super::hint;

    #[test]
    fn a_hint_shows_both_ends_and_never_the_middle() {
        let key = "sk-or-v1-0123456789abcdef";
        let hint = hint(key);
        assert!(hint.starts_with("sk-or-"));
        assert!(hint.ends_with("cdef"));
        assert!(!hint.contains("0123456789"));
    }

    #[test]
    fn a_short_string_does_not_panic() {
        assert_eq!(hint("abc"), "…");
    }
}
