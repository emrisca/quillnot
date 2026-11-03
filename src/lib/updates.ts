import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { isDesktop } from "./openrouter";

/**
 * In-app updates.
 *
 * The check and the install both live in Rust: the endpoint is read there, the archive
 * is verified there against the key in `tauri.conf.json`, and the webview only ever
 * sees a version string and a byte count. A browser tab has no updater, so every call
 * here is a no-op that a preview can render without pretending.
 */
export type UpdateInfo = {
  currentVersion: string;
  version: string;
  notes: string | null;
};

export type UpdateProgress = {
  phase: "downloading" | "installing";
  downloaded: number;
  total: number | null;
};

const PROGRESS_EVENT = "update:progress";

/** The version this build is, from the app's own metadata. */
export async function appVersion(): Promise<string | null> {
  if (!isDesktop()) return null;
  return invoke<string>("app_version");
}

/** The newer release, or null when this is the newest one. */
export async function checkUpdate(): Promise<UpdateInfo | null> {
  if (!isDesktop()) return null;
  return invoke<UpdateInfo | null>("check_update");
}

/**
 * Downloads, verifies and installs, then restarts into the new build — so this promise
 * normally never resolves. Progress arrives as events while it runs.
 */
export async function installUpdate(onProgress: (step: UpdateProgress) => void): Promise<void> {
  if (!isDesktop()) throw new Error("Updates install from the desktop app, not a browser tab.");
  const stop = await listen<UpdateProgress>(PROGRESS_EVENT, (event) => onProgress(event.payload));
  try {
    await invoke("install_update");
  } finally {
    stop();
  }
}

/** "1.4 of 8.2 MB", or just the bytes when the server sent no length. */
export function transfer(step: UpdateProgress): string {
  const mb = (bytes: number) => `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (step.phase === "installing") return `Installing ${mb(step.downloaded)}…`;
  if (step.total === null) return `Downloading ${mb(step.downloaded)}…`;
  return `Downloading ${mb(step.downloaded)} of ${mb(step.total)}`;
}
