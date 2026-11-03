import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./openrouter";

export type SpanResult = { text: string; model: string };
export type SynonymResult = { words: string[]; model: string };

/** A sentence with the offsets it occupies in the text it came from. */
export type SentenceSpan = { text: string; start: number; end: number };

const notDesktop = () =>
  new Error("Rewording a sentence needs the QuillNot app; this browser preview runs on mock output.");

/**
 * Sentence boundaries the way the rest of the app understands them: a full stop, question
 * mark or exclamation ends one, and a line break always does. Offsets are kept so the
 * caller can splice a replacement back without re-deriving where the sentence was.
 */
export function splitSentences(text: string): SentenceSpan[] {
  const spans: SentenceSpan[] = [];
  const pattern = /[^.!?\n]+[.!?]*/g;
  let match: RegExpExecArray | null;

  while ((match = pattern.exec(text)) !== null) {
    let start = match.index;
    let end = start + match[0].length;
    while (start < end && /\s/.test(text[start])) start += 1;
    while (end > start && /\s/.test(text[end - 1])) end -= 1;
    if (end > start) spans.push({ text: text.slice(start, end), start, end });
  }

  return spans;
}

/** The sentence that contains `index`, or the nearest one before it. */
export function sentenceAt(text: string, index: number): SentenceSpan | null {
  const spans = splitSentences(text);
  if (spans.length === 0) return null;
  return (
    spans.find((span) => index >= span.start && index < span.end) ??
    [...spans].reverse().find((span) => span.start <= index) ??
    spans[0]
  );
}

export const isSingleWord = (text: string): boolean => /^[\p{L}\p{M}'’-]+$/u.test(text.trim());

/**
 * Rewrites one sentence in the mode the workspace is in, with the rest of the answer as
 * context so the replacement keeps the surrounding tense and voice.
 */
export async function rephraseSentence(
  sentence: string,
  context: string,
  mode: string,
): Promise<SpanResult> {
  if (!isDesktop()) throw notDesktop();
  return invoke<SpanResult>("rephrase_sentence", { sentence, context, mode });
}

/** Alternatives for one word, chosen against the sentence it was selected from. */
export async function wordSynonyms(word: string, sentence: string): Promise<SynonymResult> {
  if (!isDesktop()) throw notDesktop();
  return invoke<SynonymResult>("word_synonyms", { word, sentence });
}
