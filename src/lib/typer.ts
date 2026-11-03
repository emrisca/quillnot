import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isDesktop } from "./openrouter";

export type TyperStatus = "idle" | "typing" | "paused" | "done" | "stopped" | "blocked";

export type TyperState = { status: TyperStatus; typed: number; total: number };

/** What the run will do, worked out from the text and the settings before it starts. */
export type TyperPlan = { cps: number; totalMs: number; steps: number; chars: number };

export type TyperConfig = {
  /** Share of characters mistyped and corrected, 0..0.25. */
  typoRate: number;
  /** Multiplier on the resting pace, 0.25..3. Ignored when a deadline is set. */
  speed: number;
  /** "Finish by" in seconds. Wins over `speed`, because that is the point of it. */
  deadlineSecs: number | null;
};

/** Matches `TYPER_EVENT` in `src-tauri/src/typer.rs`. */
export const TYPER_EVENT = "typer:state";

const notDesktop = () =>
  new Error("Typer types into other apps, so it needs the QuillNot app; this browser preview cannot do it.");

export async function typerPlan(text: string, config: TyperConfig): Promise<TyperPlan> {
  if (!isDesktop()) throw notDesktop();
  return invoke<TyperPlan>("typer_plan", { text, config });
}

export async function typerStart(text: string, config: TyperConfig): Promise<TyperPlan> {
  if (!isDesktop()) throw notDesktop();
  return invoke<TyperPlan>("typer_start", { text, config });
}

export async function typerPause(): Promise<TyperState> {
  if (!isDesktop()) throw notDesktop();
  return invoke<TyperState>("typer_pause");
}

export async function typerResume(): Promise<TyperState> {
  if (!isDesktop()) throw notDesktop();
  return invoke<TyperState>("typer_resume");
}

export async function typerStop(): Promise<TyperState> {
  if (!isDesktop()) throw notDesktop();
  return invoke<TyperState>("typer_stop");
}

export async function typerStatus(): Promise<TyperState> {
  if (!isDesktop()) return { status: "idle", typed: 0, total: 0 };
  return invoke<TyperState>("typer_status");
}

/**
 * Whether this build can type at all. False on Windows for now, and false on macOS
 * until Accessibility has been granted, which is one and the same question to the UI.
 */
export async function typerAccessibility(): Promise<boolean> {
  if (!isDesktop()) return false;
  return invoke<boolean>("typer_accessibility");
}

export async function typerOpenSettings(): Promise<void> {
  if (!isDesktop()) throw notDesktop();
  await invoke("typer_open_settings");
}

export async function onTyperState(handler: (state: TyperState) => void): Promise<UnlistenFn> {
  return listen<TyperState>(TYPER_EVENT, (event) => handler(event.payload));
}
