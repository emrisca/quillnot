//! The Rust half of QuillNot.
//!
//! Everything that touches the network or the OS lives here, because the keys that
//! pay for the requests must never be reachable from the webview:
//!
//! - [`store`] — per-provider API keys in the OS keychain, settings and the catalogue
//!   cache in the app's data directory;
//! - [`providers`] — the registry of free-tier providers, which one is active, and the
//!   failover chain when it refuses;
//! - [`catalog`] — every zero-priced model OpenRouter serves, plus the per-provider
//!   uptime/latency/throughput rows the ranking is built from;
//! - [`openrouter`] — OAuth PKCE sign-in over a loopback callback, and the chunked,
//!   streaming completion the workspace calls;
//! - [`prompt`] — the mode instructions and the block splitter.

mod catalog;
mod grammar;
mod history;
mod openrouter;
mod prompt;
mod providers;
pub mod self_test;
mod store;

use serde::Serialize;
use tauri::AppHandle;

use openrouter::{ParaphraseRequest, ParaphraseResult};
use providers::ProviderStatus;

/// What the UI needs to know about the account: whether one is connected, and a
/// hint that is *not* the key. The frontend never receives the secret itself.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub connected: bool,
    pub hint: Option<String>,
}

#[tauri::command]
fn key_status(app: AppHandle) -> KeyStatus {
    store::active_status(&app)
}

#[tauri::command]
fn save_key(key: String) -> Result<KeyStatus, String> {
    let key = key.trim().to_string();
    if !key.starts_with("sk-or-") {
        return Err("That does not look like an OpenRouter key: they start with sk-or-.".into());
    }
    store::save_key(&key)?;
    store::save_key_for("openrouter", &key)?;
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

/// Every zero-priced model with its best availability and speed, scored. The AI
/// Models view calls this; nothing here needs an account.
#[tauri::command]
async fn model_catalogue() -> Result<Vec<catalog::RankedModel>, String> {
    catalog::ranked_models().await
}

#[tauri::command]
async fn paraphrase(app: AppHandle, request: ParaphraseRequest) -> Result<ParaphraseResult, String> {
    openrouter::paraphrase(app.clone(), request).await
}

/// The grammar check. Every span in the reply was found by searching the text for the
/// model's quote, so the frontend can trust the offsets enough to splice with them.
#[tauri::command]
async fn grammar_check(
    app: AppHandle,
    text: String,
    model: Option<String>,
) -> Result<grammar::GrammarResult, String> {
    grammar::check(&app, &text, model).await
}

/* ------------------------------------------------------------------ history */

#[tauri::command]
fn list_runs(app: AppHandle) -> Result<Vec<history::Run>, String> {
    history::list(&app)
}

#[tauri::command]
fn delete_run(app: AppHandle, id: String) -> Result<Vec<history::Run>, String> {
    history::delete(&app, &id)
}

#[tauri::command]
fn clear_history(app: AppHandle) -> Result<Vec<history::Run>, String> {
    history::clear(&app)
}

#[tauri::command]
fn run_stats(app: AppHandle) -> Result<history::Stats, String> {
    history::stats(&app)
}

#[tauri::command]
fn open_data_dir(app: AppHandle) -> Result<(), String> {
    history::open_folder(&app)
}

/// The mode the workspace opens in; `None` means Standard.
#[tauri::command]
fn default_mode(app: AppHandle) -> Option<String> {
    store::read_settings(&app).default_mode
}

#[tauri::command]
fn set_default_mode(app: AppHandle, mode: String) -> Result<(), String> {
    let mut settings = store::read_settings(&app);
    let mode = mode.trim().to_string();
    settings.default_mode = if mode.is_empty() { None } else { Some(mode) };
    store::write_settings(&app, &settings)
}

/* ---------------------------------------------------------------- providers */

#[tauri::command]
fn list_providers(app: AppHandle) -> Vec<ProviderStatus> {
    providers::statuses(&app)
}

#[tauri::command]
fn save_provider_key(app: AppHandle, provider: String, key: String) -> Result<Vec<ProviderStatus>, String> {
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err("Paste the key first.".into());
    }
    if let Some(info) = providers::get(&provider) {
        if let Some(prefix) = info.key_prefix {
            if !key.starts_with(prefix) {
                return Err(format!("{} keys start with “{prefix}”.", info.label));
            }
        }
    }
    store::save_key_for(&provider, &key)?;
    Ok(providers::statuses(&app))
}

#[tauri::command]
fn clear_provider_key(app: AppHandle, provider: String) -> Result<Vec<ProviderStatus>, String> {
    store::clear_key_for(&provider)?;
    Ok(providers::statuses(&app))
}

#[tauri::command]
fn set_active_provider(app: AppHandle, provider: String) -> Result<Vec<ProviderStatus>, String> {
    if providers::get(&provider).is_none() {
        return Err(format!("Unknown provider “{provider}”."));
    }
    let mut settings = store::read_settings(&app);
    settings.active_provider = Some(provider);
    store::write_settings(&app, &settings)?;
    Ok(providers::statuses(&app))
}

#[tauri::command]
fn set_provider_model(app: AppHandle, provider: String, model: String) -> Result<Vec<ProviderStatus>, String> {
    let mut settings = store::read_settings(&app);
    let model = model.trim().to_string();
    if model.is_empty() {
        settings.provider_models.remove(&provider);
    } else {
        settings.provider_models.insert(provider, model);
    }
    store::write_settings(&app, &settings)?;
    Ok(providers::statuses(&app))
}

#[tauri::command]
fn set_custom_endpoint(app: AppHandle, base_url: String, model: String) -> Result<Vec<ProviderStatus>, String> {
    let base_url = base_url.trim().to_string();
    if !base_url.is_empty() && !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
        return Err("The base URL has to start with http:// or https://.".into());
    }

    let mut settings = store::read_settings(&app);
    settings.custom_base_url = if base_url.is_empty() { None } else { Some(base_url) };
    let model = model.trim().to_string();
    if model.is_empty() {
        settings.provider_models.remove("custom");
    } else {
        settings.provider_models.insert("custom".to_string(), model);
    }
    store::write_settings(&app, &settings)?;
    Ok(providers::statuses(&app))
}

/// Opens a provider's documentation in the system browser. Only https, so this cannot
/// be turned into a way to launch arbitrary local handlers from the webview.
#[tauri::command]
fn open_docs(url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("Only https links are opened.".into());
    }
    openrouter::open_browser(&url)
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
            model_catalogue,
            paraphrase,
            grammar_check,
            list_runs,
            delete_run,
            clear_history,
            run_stats,
            open_data_dir,
            default_mode,
            set_default_mode,
            list_providers,
            save_provider_key,
            clear_provider_key,
            set_active_provider,
            set_provider_model,
            set_custom_endpoint,
            open_docs
        ])
        .run(tauri::generate_context!())
        .expect("error while running QuillNot");
}
