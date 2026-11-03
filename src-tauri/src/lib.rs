//! The Rust half of QuillNot.
//!
//! Everything that touches the network or the OS lives here, because the key that
//! pays for the requests must never be reachable from the webview:
//!
//! - [`store`] — the API key in the OS keychain, settings and the catalogue cache
//!   in the app's data directory;
//! - [`catalog`] — every zero-priced model OpenRouter serves, plus the per-provider
//!   uptime/latency/throughput rows the ranking in M7 is built from;
//! - [`openrouter`] — OAuth PKCE sign-in over a loopback callback, and the chunked,
//!   streaming completion the workspace calls;
//! - [`prompt`] — the mode instructions and the block splitter.

mod catalog;
mod openrouter;
mod prompt;
pub mod self_test;
mod store;

use serde::Serialize;
use tauri::AppHandle;

use openrouter::{ParaphraseRequest, ParaphraseResult};

/// What the UI needs to know about the account: whether one is connected, and a
/// hint that is *not* the key. The frontend never receives the secret itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub connected: bool,
    pub hint: Option<String>,
}

#[tauri::command]
fn key_status() -> KeyStatus {
    store::key_status()
}

#[tauri::command]
fn save_key(key: String) -> Result<KeyStatus, String> {
    let key = key.trim().to_string();
    if !key.starts_with("sk-or-") {
        return Err("That does not look like an OpenRouter key: they start with sk-or-.".into());
    }
    store::save_key(&key)?;
    Ok(store::key_status())
}

#[tauri::command]
fn clear_key() -> Result<KeyStatus, String> {
    store::clear_key()?;
    Ok(store::key_status())
}

#[tauri::command]
async fn sign_in() -> Result<KeyStatus, String> {
    openrouter::sign_in().await
}

#[tauri::command]
async fn list_free_models(app: AppHandle) -> Result<Vec<catalog::ModelRow>, String> {
    catalog::free_models(&app).await
}

#[tauri::command]
async fn model_endpoints(id: String) -> Result<Vec<catalog::EndpointRow>, String> {
    catalog::endpoints(&id).await
}

#[tauri::command]
async fn paraphrase(app: AppHandle, request: ParaphraseRequest) -> Result<ParaphraseResult, String> {
    openrouter::paraphrase(app.clone(), request).await
}

// The event the workspace listens on while a rewrite streams in. Named here so the
// frontend and this file cannot drift: `src/lib/openrouter.ts` listens for exactly
// this string.
pub const DELTA_EVENT: &str = "paraphrase:delta";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            key_status,
            save_key,
            clear_key,
            sign_in,
            list_free_models,
            model_endpoints,
            paraphrase
        ])
        .run(tauri::generate_context!())
        .expect("error while running QuillNot");
}
