import { invoke } from "@tauri-apps/api/core";
import { isDesktop, type ParaphraseResult } from "./openrouter";
import { UNSLOP_SKILL } from "./unslop";

/**
 * The skill is the whole of the humanizer's brief.
 *
 * It is not a mode id the backend interprets: the text below is what the model is told,
 * character for character, so making it editable changes what actually happens. Presets
 * are starting points, and the person's edits are what gets saved and sent.
 */
export type SkillPreset = { id: string; label: string; skill: string };

/** What the pane opens on the first time this build runs. */
export const DEFAULT_PRESET: SkillPreset = {
  id: "plain",
  label: "Plain editing",
  skill: `Edit the text so it reads like a person wrote it, not a model.
- Vary sentence length. Keep some sentences short.
- Replace stock phrasing and transition words with plain connectives.
- Keep every fact, name, number and paragraph break.
- Keep the author's voice and register. Do not simplify the ideas.
- Do not add examples, claims or detail that is not already in the text.
- Return the edited text only.`,
};

/**
 * This workspace's own unslop skill, byte for byte, from `skills/unslop.md`.
 *
 * It is longer than a prompt would normally be, and that is the point: the rule numbers
 * and the reasons are part of the skill, and the person can cut it down in the pane.
 */
export const UNSLOP_PRESET: SkillPreset = {
  id: "unslop",
  label: "Unslop",
  skill: UNSLOP_SKILL,
};

export const SKILL_PRESETS: SkillPreset[] = [DEFAULT_PRESET, UNSLOP_PRESET];

export type HumanizerState = { skill: string | null; preset: string | null };

/** The skill as it was left, or nothing on a first run. The pane then opens on a preset. */
export async function loadHumanizer(): Promise<HumanizerState> {
  if (!isDesktop()) return { skill: null, preset: null };
  return invoke<HumanizerState>("humanizer");
}

/** Remembers the text and which preset it came from. Called as the person types. */
export async function saveHumanizer(skill: string, preset: string): Promise<void> {
  if (!isDesktop()) return;
  await invoke("set_humanizer", { skill, preset });
}

/** The same command the paraphraser calls, with the skill bundled in place of a mode. */
export async function humanize(text: string, skill: string): Promise<ParaphraseResult> {
  if (!isDesktop()) {
    throw new Error("Humanizing needs the QuillNot app; this browser preview runs on mock output.");
  }
  return invoke<ParaphraseResult>("paraphrase", { request: { text, skill } });
}
