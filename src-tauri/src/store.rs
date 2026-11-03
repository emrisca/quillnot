//! The API keys, the settings file and the catalogue cache.
//!
//! Keys go to the OS keychain — Keychain on macOS, Credential Manager on Windows —
//! one entry per provider, and are never written to disk by this app. Settings and
//! the cached model list live under the app's data directory, which is where M4's
//! history will go as well.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::KeyStatus;

/// The OpenRouter entry as it existed before the provider list; kept so a key stored
/// by an earlier build keeps working.
const SERVICE: &str = "dev.emrys.quillnot.openrouter";
const ACCOUNT: &str = "api-key";
/// Everything added by the provider list, one entry per provider id.
const PROVIDER_SERVICE: &str = "dev.emrys.quillnot.provider";
const SETTINGS_FILE: &str = "settings.json";
const MODELS_CACHE: &str = "catalog-models.json";
const RANKING_CACHE: &str = "catalog-ranking.json";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub default_model: Option<String>,
    /// Which provider answers a rewrite. Absent means OpenRouter.
    pub active_provider: Option<String>,
    /// Per-provider model choice, so a provider whose ids move can be corrected
    /// without a new release.
    pub provider_models: BTreeMap<String, String>,
    /// Base URL for the "bring your own endpoint" provider.
    pub custom_base_url: Option<String>,
    /// The rewrite mode the workspace opens in. Absent means Standard.
    pub default_mode: Option<String>,
    /// True unless the person turned Auto off. Absent means on.
    pub auto_model: Option<bool>,
    /// A model pinned from the AI Models table. A pin beats Auto.
    pub pinned_model: Option<String>,
    /// The humanizer's skill text, as the person last left it. The request bundles this
    /// verbatim, which is why its edits have to survive a restart.
    pub humanizer_skill: Option<String>,
    /// Which preset that text started from, so the menu still shows the right entry after
    /// the person has edited it.
    pub humanizer_preset: Option<String>,
    /// Typer's stop chord, in the plugin's spelling: "Control+Alt+Shift+KeyS". Absent
    /// means the platform default, so clearing it is always a way back to something that
    /// works.
    pub typer_stop_shortcut: Option<String>,
}

impl Settings {
    /// Auto is on unless it was explicitly turned off: the ranking is the reason the
    /// catalogue exists, so picking from it is the default rather than an opt-in.
    pub fn auto_enabled(&self) -> bool {
        self.auto_model.unwrap_or(true)
    }
}

fn legacy_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("the OS key store is unavailable: {e}"))
}

fn entry_for(provider: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(PROVIDER_SERVICE, provider)
        .map_err(|e| format!("the OS key store is unavailable: {e}"))
}

pub fn data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// The key for one provider, falling back to the entry an earlier build wrote for
/// OpenRouter so nobody has to sign in twice across an update.
pub fn load_key_for(provider: &str) -> Option<String> {
    if let Ok(entry) = entry_for(provider) {
        if let Ok(key) = entry.get_password() {
            if !key.trim().is_empty() {
                return Some(key);
            }
        }
    }
    if provider == "openrouter" {
        return load_key();
    }
    None
}

pub fn save_key_for(provider: &str, key: &str) -> Result<(), String> {
    entry_for(provider)?
        .set_password(key)
        .map_err(|e| format!("could not write the key to the OS key store: {e}"))
}

pub fn clear_key_for(provider: &str) -> Result<(), String> {
    match entry_for(provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(err) => return Err(format!("could not remove the stored key: {err}")),
    }
    if provider == "openrouter" {
        // The pre-provider entry would otherwise resurrect the old key.
        clear_key()?;
    }
    Ok(())
}

/* ------------------------------------------------------- the OpenRouter entry */

pub fn load_key() -> Option<String> {
    let entry = legacy_entry().ok()?;
    match entry.get_password() {
        Ok(key) if !key.trim().is_empty() => Some(key),
        _ => None,
    }
}

pub fn save_key(key: &str) -> Result<(), String> {
    legacy_entry()?
        .set_password(key)
        .map_err(|e| format!("could not write the key to the OS key store: {e}"))
}

pub fn clear_key() -> Result<(), String> {
    match legacy_entry()?.delete_credential() {
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

/// The header badge: is the provider that will answer a rewrite connected?
pub fn active_status(app: &AppHandle) -> KeyStatus {
    let settings = read_settings(app);
    let provider = settings
        .active_provider
        .unwrap_or_else(|| crate::providers::DEFAULT_PROVIDER.to_string());

    match crate::providers::get(&provider) {
        Some(info) if !info.needs_key => KeyStatus {
            connected: true,
            hint: Some(info.label.to_string()),
        },
        _ => match load_key_for(&provider) {
            Some(key) => KeyStatus {
                connected: true,
                hint: Some(hint(&key)),
            },
            None => KeyStatus {
                connected: false,
                hint: None,
            },
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

pub fn write_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = data_dir(app)?.join(SETTINGS_FILE);
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| e.to_string())
}

/// Where the last good model list is kept so the picker works without a network.
pub fn models_cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join(MODELS_CACHE))
}

/// Where the last ranked table is kept: Auto answers from here mid-run instead of
/// waiting on seventeen HTTP requests.
pub fn ranking_cache_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(data_dir(app)?.join(RANKING_CACHE))
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

    /// Touches the real OS key store, so it is `#[ignore]`d out of the normal run.
    /// This is the test that catches a build where keyring has no backend compiled
    /// in — `Entry::new` fails there, and nobody can sign in at all.
    #[test]
    #[ignore = "touches the real OS key store"]
    fn the_os_key_store_is_available() {
        for entry in [
            super::legacy_entry().expect("a key store backend is compiled in"),
            super::entry_for("openrouter").expect("a key store backend is compiled in"),
        ] {
            // Reading an item that does not exist must report absence, not break the
            // store — the app asks this on every launch.
            match entry.get_password() {
                Ok(_) | Err(keyring::Error::NoEntry) => {}
                Err(other) => panic!("unexpected key store error: {other}"),
            }
        }
    }
}
