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

/**
 * The rest of the menu: the registers people actually need, each one a brief rather than a
 * mode id. They are starting points on purpose, and the pane is where they get edited.
 */
export const ACADEMIC_PRESET: SkillPreset = {
  id: "academic",
  label: "Academic",
  skill: `Edit the text so it reads as a person's academic writing, not a model's.
- Keep every claim, citation and hedge exactly as it is. Never invent a source.
- Vary sentence length. Let a long analytical sentence follow a short one.
- Replace stock academic filler ("delve into", "it is worth noting", "plays a crucial role") with plain verbs.
- Use "this" and "the study" only where the referent is clear.
- Keep the register formal: no contractions, no slang, no rhetorical questions.
- Return the edited text only.`,
};

export const CONVERSATIONAL_PRESET: SkillPreset = {
  id: "conversational",
  label: "Conversational",
  skill: `Rewrite the text the way you would say it out loud to one person.
- Use contractions and ordinary words.
- Vary sentence length. A fragment is fine now and then.
- Cut throat-clearing openers and summary closers.
- Keep every fact, name and number, and the author's opinion where there is one.
- Do not add jokes, asides or detail that is not in the text.
- Return the rewritten text only.`,
};

export const BUSINESS_PRESET: SkillPreset = {
  id: "business",
  label: "Business",
  skill: `Edit the text for a reader at work who is short on time.
- Lead with the point: the ask or the result comes first.
- Prefer active voice and name who does what.
- Replace corporate filler ("leverage", "synergy", "circle back") with the plain verb it stands for.
- Keep numbers, dates and owners exactly as written.
- Cut anything that does not change what the reader does next.
- Return the edited text only.`,
};

export const PLAIN_ENGLISH_PRESET: SkillPreset = {
  id: "plain-english",
  label: "Plain English",
  skill: `Rewrite the text so a smart reader who is not a specialist understands it once.
- Prefer the short word: "use" not "utilise", "help" not "facilitate".
- One idea to a sentence. Split anything carrying more than one "and" or "which".
- Active voice unless the actor is genuinely unknown.
- Keep the technical terms the reader needs to learn, and drop the rest.
- Keep every fact and number.
- Return the rewritten text only.`,
};

export const TIGHTEN_PRESET: SkillPreset = {
  id: "tighten",
  label: "Tighten",
  skill: `Cut the text to its shortest honest form.
- Remove about a third of the words without losing a fact.
- Delete hedges, restatements, and sentences that only announce what comes next.
- Keep names, numbers, dates and quotes intact.
- Keep the author's tone. Shorter is not the same as colder.
- Return the shortened text only.`,
};

export const SKILL_PRESETS: SkillPreset[] = [
  DEFAULT_PRESET,
  UNSLOP_PRESET,
  ACADEMIC_PRESET,
  CONVERSATIONAL_PRESET,
  BUSINESS_PRESET,
  PLAIN_ENGLISH_PRESET,
  TIGHTEN_PRESET,
];

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
