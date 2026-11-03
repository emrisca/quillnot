import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isDesktop } from "./openrouter";

export type TyperStatus =
  | "idle"
  | "typing"
  | "paused"
  | "done"
  | "stopped"
  | "blocked"
  /** Stopped because QuillNot's own window had focus: the keystroke would have landed here. */
  | "unfocused"
  /** Stopped itself: nothing was accepted for the configured idle seconds. */
  | "stalled";

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
  /** Stop after this many seconds with no keystroke accepted. Null never stops on time. */
  idleSecs: number | null;
};

/** What a generated run produced: the finished text, what the model added, and the plan. */
export type TyperGenerated = { text: string; added: number; plan: TyperPlan };

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

/**
 * Hands the chord the text it should type. The chord is one button with two halves: it
 * stops a live run, and it starts this text when nothing is running. The view calls this
 * whenever the text or settings change, so the chord can never type an old draft.
 */
export async function typerArm(text: string, config: TyperConfig): Promise<void> {
  if (!isDesktop()) return;
  await invoke("typer_arm", { text, config });
}

/** Forgets the armed text, so the chord cannot start a run from it. */
export async function typerDisarm(): Promise<void> {
  if (!isDesktop()) return;
  await invoke("typer_disarm");
}

/**
 * "Finish by" with the model writing the ending: it continues the text to the length the
 * deadline leaves room for, and starts typing the joined result.
 */
export async function typerGenerate(text: string, config: TyperConfig): Promise<TyperGenerated> {
  if (!isDesktop()) throw notDesktop();
  return invoke<TyperGenerated>("typer_generate", { text, config });
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
 * Three separate answers, because they lead to three different menus: this platform may
 * have no injector at all (X11, for now), it may have one that needs no permission
 * (Windows), or it may have one that is waiting on one (macOS Accessibility). `platform`
 * names the OS so the copy can match it.
 */
export type TyperAccess = {
  supported: boolean;
  granted: boolean;
  platform: string;
};

export async function typerAccessibility(): Promise<TyperAccess> {
  if (!isDesktop()) return { supported: false, granted: false, platform: "browser" };
  return invoke<TyperAccess>("typer_accessibility");
}

export async function typerOpenSettings(): Promise<void> {
  if (!isDesktop()) throw notDesktop();
  await invoke("typer_open_settings");
}

/**
 * The stop chord registered with the OS, in the plugin's spelling: "Control+Alt+Shift+KeyS".
 * Empty in a browser, where there is nothing to register.
 */
export async function typerShortcut(): Promise<string> {
  if (!isDesktop()) return "";
  return invoke<string>("typer_shortcut");
}

/**
 * Takes a chord the person pressed and makes it the live one. The answer is the chord the
 * OS actually accepted, which is not always the one asked for: another app may own it,
 * and the command then fails with the old chord still in place.
 */
export async function setTyperShortcut(shortcut: string): Promise<string> {
  if (!isDesktop()) throw notDesktop();
  return invoke<string>("set_typer_shortcut", { shortcut });
}

export async function onTyperState(handler: (state: TyperState) => void): Promise<UnlistenFn> {
  return listen<TyperState>(TYPER_EVENT, (event) => handler(event.payload));
}
