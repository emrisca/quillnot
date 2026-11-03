import { invoke } from "@tauri-apps/api/core";
import { isDesktop, type ParaphraseResult } from "./openrouter";

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
 * The house style from this workspace's `unslop` skill, cut down to what a model can act
 * on in one pass. Rule numbers are dropped because the model has no file to look them up
 * in; the substance is kept.
 */
export const UNSLOP_PRESET: SkillPreset = {
  id: "unslop",
  label: "Unslop",
  skill: `Remove the tells that make writing sound machine-generated. Keep the meaning and match the intended tone.

Content
- Delete superficial -ing tails ("highlighting...", "ensuring...", "showcasing...") or replace them with a fact.
- Replace vague attributions ("experts believe", "reports suggest", "critics argue") with a source or delete them.

Language
- Replace AI vocabulary with plain words: additionally, crucial, delve, enhance, foster, garner, intricate, landscape, pivotal, showcase, tapestry, testament, underscore, vibrant.
- Say "is" or "has" instead of "serves as", "stands as", "boasts", "features".
- Drop "not just X, but Y" and state the point once.
- Use the natural number. Never force ideas into groups of three.
- Repeat one word for a thing instead of cycling synonyms.
- Replace false ranges ("from X to Y") with a plain list.

Style
- No em dashes and no en dashes. End the sentence or use a comma.
- Use a colon only before a list or an example, never as a mid-sentence connector.
- Do not bold every name or acronym. No decorative emoji. Straight quotes only.
- Sentence case in headings, not title case.

Filler
- "in order to" becomes "to". "due to the fact that" becomes "because". Delete "it is important to note that".
- Cut hedging to a single "may".
- Replace generic conclusions with a specific fact or plan.
- Prefer the plain word: use, not utilise or leverage; help, not facilitate; many, not numerous; if, not in the event that.
- Prefer the active voice and name the actor.
- Cut adverbs that prop up a weak verb, or use the stronger verb.

Return the edited text only.`,
};

export const SKILL_PRESETS: SkillPreset[] = [DEFAULT_PRESET, UNSLOP_PRESET];

export type HumanizerState = { skill: string | null; preset: string | null };

/** The skill as it was left, or nothing on a first run — the pane then opens on a preset. */
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
