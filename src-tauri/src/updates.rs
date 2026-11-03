//! In-app updates, through `tauri-plugin-updater`.
//!
//! The release workflow publishes `latest.json` beside the installers and signs each
//! updater archive with a minisign key whose public half is in `tauri.conf.json`. The
//! endpoint is only JSON, so the signature is the whole of the trust: a build that was
//! tampered with, or one signed by another key, is refused before anything on disk is
//! replaced.
//!
//! The `Update` the check returns is kept here until the person says yes, because the
//! download it points at is the one that gets installed — checking and installing are
//! two halves of the same object, not two requests.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Where the download is up to. `src/lib/updates.ts` listens for exactly this string.
pub const PROGRESS_EVENT: &str = "update:progress";

/// What a check found. The `Update` itself cannot cross the boundary — it hands out the
/// download — so only these three fields go to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub version: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    phase: &'static str,
    downloaded: u64,
    total: Option<u64>,
}

/// The update the last check found, waiting for a person to accept it.
#[derive(Default)]
pub struct Pending(Mutex<Option<Update>>);

/// The version this build was compiled as, read from the app's own metadata rather
/// than from the webview, so the two cannot disagree about what is installed.
#[tauri::command]
pub fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Asks the endpoint in `tauri.conf.json` whether there is anything newer. `Ok(None)`
/// means this is the newest release, which is not an error and not worth a banner.
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    pending: State<'_, Pending>,
) -> Result<Option<UpdateInfo>, String> {
    let update = app
        .updater()
        .map_err(|error| format!("The updater is not configured in this build: {error}"))?
        .check()
        .await
        .map_err(|error| format!("Could not reach the release feed: {error}"))?;

    match update {
        Some(update) => {
            let info = UpdateInfo {
                current_version: update.current_version.clone(),
                version: update.version.clone(),
                notes: update.body.clone(),
            };
            *pending.0.lock().unwrap() = Some(update);
            Ok(Some(info))
        }
        None => {
            *pending.0.lock().unwrap() = None;
            Ok(None)
        }
    }
}

/// Downloads, verifies and installs the update the check found, then restarts into it.
///
/// The guard is taken and dropped before the first `await`: a `MutexGuard` is not
/// `Send`, and this command's future has to be.
#[tauri::command]
pub async fn install_update(app: AppHandle, pending: State<'_, Pending>) -> Result<(), String> {
    let awaiting = pending.0.lock().unwrap().take();
    let Some(update) = awaiting else {
        return Err("There is nothing to install. Check for updates first.".into());
    };

    let downloaded = Arc::new(AtomicU64::new(0));
    let counter = downloaded.clone();
    let ticker = app.clone();
    let finisher = app.clone();

    update
        .download_and_install(
            move |chunk, total| {
                let bytes = counter.fetch_add(chunk as u64, Ordering::Relaxed) + chunk as u64;
                let _ = ticker.emit(
                    PROGRESS_EVENT,
                    Progress { phase: "downloading", downloaded: bytes, total },
                );
            },
            move || {
                let _ = finisher.emit(
                    PROGRESS_EVENT,
                    Progress {
                        phase: "installing",
                        downloaded: downloaded.load(Ordering::Relaxed),
                        total: None,
                    },
                );
            },
        )
        .await
        .map_err(|error| format!("The update could not be installed: {error}"))?;

    // On Windows the installer has already exited the process by this point; on the
    // other platforms the new bundle is in place and only needs to be opened. `restart`
    // never returns, which is why this is the tail of the function rather than followed
    // by an `Ok(())` nobody could reach.
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_crosses_the_boundary_in_camel_case() {
        let info = UpdateInfo {
            current_version: "0.1.0".into(),
            version: "0.1.1".into(),
            notes: None,
        };
        let json = serde_json::to_value(&info).expect("serialize");
        assert_eq!(json["currentVersion"], "0.1.0");
        assert_eq!(json["version"], "0.1.1");
        assert!(json["notes"].is_null());
        assert!(json.get("current_version").is_none());
    }

    #[test]
    fn progress_reports_bytes_and_a_phase() {
        let json = serde_json::to_value(Progress {
            phase: "downloading",
            downloaded: 1024,
            total: Some(4096),
        })
        .expect("serialize");
        assert_eq!(json["phase"], "downloading");
        assert_eq!(json["downloaded"], 1024);
        assert_eq!(json["total"], 4096);
    }
}
