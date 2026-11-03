import { invoke } from "@tauri-apps/api/core";
import { isDesktop, type ParaphraseResult } from "./openrouter";

/**
 * Target languages, in the names a person picks them by rather than in ISO codes. The
 * model reads whatever string it is given, so the list is a menu and not a constraint.
 */
export const LANGUAGES = [
  "Arabic",
  "Brazilian Portuguese",
  "Chinese (Simplified)",
  "Chinese (Traditional)",
  "Czech",
  "Danish",
  "Dutch",
  "English (UK)",
  "English (US)",
  "Finnish",
  "French",
  "German",
  "Greek",
  "Hebrew",
  "Hindi",
  "Hungarian",
  "Indonesian",
  "Italian",
  "Japanese",
  "Korean",
  "Norwegian",
  "Polish",
  "Portuguese",
  "Romanian",
  "Russian",
  "Spanish",
  "Swedish",
  "Thai",
  "Turkish",
  "Ukrainian",
  "Vietnamese",
] as const;

export type Language = (typeof LANGUAGES)[number];

/** The registers the prompt understands. Anything else falls back to Neutral. */
export const FORMALITY = ["Neutral", "Formal", "Informal"] as const;

export type Formality = (typeof FORMALITY)[number];

/**
 * Translation rides the paraphrase command: same streaming, provider routing, block
 * splitting and history, with a target language instead of a mode. The model answers from
 * its own weights, so nothing but the text leaves the machine.
 */
export async function translate(
  text: string,
  target: string,
  formality: Formality,
): Promise<ParaphraseResult> {
  if (!isDesktop()) {
    throw new Error("Translation needs the QuillNot app; this browser preview is a mock.");
  }
  return invoke<ParaphraseResult>("paraphrase", {
    request: { text, target, formality },
  });
}
