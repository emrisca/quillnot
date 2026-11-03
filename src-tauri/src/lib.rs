//! The Rust half of QuillNot.
//!
//! M1 is the UI shell, so this is deliberately empty. M2 adds the module that
//! matters: `openrouter`, which owns the API key (OS keychain), the model list
//! fetched from `/api/v1/models` and filtered to zero-priced prompt models, and
//! the streaming completion with backoff and a fallback chain. Keeping the key
//! and the network on this side means the webview never holds a secret.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running QuillNot");
}
