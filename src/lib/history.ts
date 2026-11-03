import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./openrouter";

/**
 * The local run log, as Rust writes it.
 *
 * A grammar run has no result — the checker only suggested — so `result` is null and
 * `note` carries what it found. A browser tab has no data directory, so every call
 * here answers empty rather than pretending to have history.
 */
export type RunTool = "paraphrase" | "grammar";

export type Run = {
  id: string;
  /** Unix seconds. */
  at: number;
  tool: RunTool;
  mode: string;
  model: string;
  original: string;
  result: string | null;
  note: string;
  wordsIn: number;
  wordsOut: number;
  ms: number;
};

export type ModelUse = { model: string; runs: number };

export type Stats = {
  runs: number;
  wordsIn: number;
  wordsOut: number;
  perTool: Record<string, number>;
  perMode: Record<string, number>;
  models: ModelUse[];
  /** Words added (positive) or removed (negative) across rewrites. */
  wordsDelta: number;
};

export async function listRuns(): Promise<Run[]> {
  if (!isDesktop()) return [];
  return invoke<Run[]>("list_runs");
}

export async function deleteRun(id: string): Promise<Run[]> {
  if (!isDesktop()) return [];
  return invoke<Run[]>("delete_run", { id });
}

export async function clearHistory(): Promise<Run[]> {
  if (!isDesktop()) return [];
  return invoke<Run[]>("clear_history");
}

export async function runStats(): Promise<Stats | null> {
  if (!isDesktop()) return null;
  return invoke<Stats>("run_stats");
}

export async function openDataDir(): Promise<void> {
  if (!isDesktop()) return;
  await invoke("open_data_dir");
}

/** The mode the workspace opens in, or null for Standard. */
export async function defaultMode(): Promise<string | null> {
  if (!isDesktop()) return null;
  return invoke<string | null>("default_mode");
}

export async function setDefaultMode(mode: string): Promise<void> {
  if (!isDesktop()) return;
  await invoke("set_default_mode", { mode });
}

/* ------------------------------------------------------------ restoring a run */

export const RESTORE_EVENT = "quillnot:restore";

/**
 * The run waiting to be loaded.
 *
 * The history view is replaced by the tool that has to show the run, so the tool
 * mounts *after* the click. An event alone would arrive before there was anything to
 * receive it, so the run is parked here and the view takes it on mount; the event
 * exists only to tell the shell which tool to open.
 */
let pending: Run | null = null;

export function restoreRun(run: Run) {
  pending = run;
  window.dispatchEvent(new CustomEvent(RESTORE_EVENT, { detail: run }));
}

export function takeRestore(tool: RunTool): Run | null {
  if (pending && pending.tool === tool) {
    const run = pending;
    pending = null;
    return run;
  }
  return null;
}

/** "just now", "12 min ago", "3 h ago", then a date — enough to place a run. */
export function ago(at: number): string {
  const seconds = Math.max(0, Math.floor(Date.now() / 1000 - at));
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} h ago`;
  if (seconds < 7 * 86_400) return `${Math.floor(seconds / 86_400)} d ago`;
  return new Date(at * 1000).toLocaleDateString();
}

/** The first line, short enough for a row. */
export function preview(text: string, limit = 150): string {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > limit ? `${flat.slice(0, limit)}…` : flat;
}
