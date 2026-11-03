/**
 * Tone, read from the words on the page.
 *
 * No model call. A rewrite is a couple of hundred words, and a scored reading of the
 * signals a reader actually notices — contractions, hedges, boosters, formal
 * connectives, warmth — is instant, free and repeatable, where a request per edit
 * would be slow and would change its mind. It is a heuristic and the rail says so:
 * the label is what the text leans towards, not a verdict on it.
 */
import { countWords } from "./text";

export type ToneId = "formal" | "confident" | "tentative" | "casual" | "warm" | "neutral";

export type ToneReading = {
  id: ToneId;
  label: string;
  note: string;
  /** What was counted, in the writer's words. At most two, most telling first. */
  signals: string[];
  /** 0–100: how much of the evidence pointed at this tone rather than another. */
  certainty: number;
};

const LABELS: Record<ToneId, string> = {
  formal: "Formal",
  confident: "Confident",
  tentative: "Tentative",
  casual: "Casual",
  warm: "Warm",
  neutral: "Neutral",
};

const NOTES: Record<ToneId, string> = {
  formal: "Long words and no contractions, so this reads like a document.",
  confident: "The text commits to what it says instead of softening it.",
  tentative: "Hedges leave the writer room to be wrong.",
  casual: "Contractions and everyday words. This reads like speech.",
  warm: "It thanks and includes the reader rather than reporting at them.",
  neutral: "Nothing strongly marked either way: plain, even prose.",
};

const CONTRACTION = /\b[a-z]+'(?:s|t|re|ll|ve|d|m)\b/gi;
const CASUAL = /\b(?:gonna|wanna|kinda|sorta|stuff|guys|folks|awesome|cool|super|totally|pretty much|a bit|anyway|okay|ok|yeah|yep|nope|huge|nice|bit|lots)\b/gi;
const HEDGE = /\b(?:maybe|perhaps|possibly|probably|seems?|appears?|might|somewhat|fairly|arguably|i think|i guess|sort of|kind of|tends? to|generally|often)\b/gi;
const BOOSTER = /\b(?:definitely|certainly|clearly|obviously|must|always|never|will|proven|guarantee[sd]?|absolutely|simply|exactly)\b/gi;
const WARM = /\b(?:please|thanks?|thank|happy|glad|hope|appreciate|together|welcome|kind|care|enjoy|sorry)\b/gi;
const FORMAL = /\b(?:furthermore|moreover|therefore|thus|hence|however|regarding|pursuant|hereby|shall|accordingly|consequently|notwithstanding|respectively|thereby|wherein)\b/gi;
const LONG_WORD = /\b[a-z]{9,}\b/gi;

const count = (text: string, pattern: RegExp): number => text.match(pattern)?.length ?? 0;

/** A reading of the tone, or null when there is too little text to read one from. */
export function readTone(text: string): ToneReading | null {
  const words = countWords(text);
  if (words < 8) return null;

  // Per hundred words, so a long paragraph does not read as "more" of everything.
  const rate = (n: number): number => (n / words) * 100;

  const contractions = count(text, CONTRACTION);
  const casual = rate(count(text, CASUAL));
  const hedge = rate(count(text, HEDGE));
  const booster = rate(count(text, BOOSTER));
  const warm = rate(count(text, WARM));
  const formal = rate(count(text, FORMAL));
  const longWords = rate(count(text, LONG_WORD));
  const exclaim = count(text, /!/g);
  // "we" and "our" belong to warmth, but a report written in the first person plural
  // is not warm by itself, so they are weighted low rather than counted as evidence.
  const plural = rate(count(text, /\b(?:we|our|us)\b/gi));

  const candidates: { id: ToneId; score: number; signals: string[] }[] = [
    {
      id: "formal",
      score: formal * 1.6 + longWords * 0.55 + (contractions === 0 ? 0.9 : 0),
      signals: [
        formal > 0 ? pluralise(count(text, FORMAL), "formal connective") : "",
        longWords > 0 ? pluralise(count(text, LONG_WORD), "long word") : "",
      ].filter(Boolean),
    },
    {
      id: "casual",
      score: casual * 1.4 + contractions * 0.9 + exclaim * 0.5,
      signals: [
        contractions > 0 ? pluralise(contractions, "contraction") : "",
        casual > 0 ? pluralise(count(text, CASUAL), "everyday word") : "",
      ].filter(Boolean),
    },
    {
      id: "confident",
      score: booster * 1.5,
      signals: [booster > 0 ? pluralise(count(text, BOOSTER), "booster") : ""].filter(Boolean),
    },
    {
      id: "tentative",
      score: hedge * 1.6,
      signals: [hedge > 0 ? pluralise(count(text, HEDGE), "hedge") : ""].filter(Boolean),
    },
    {
      id: "warm",
      score: warm * 1.5 + plural * 0.35,
      signals: [
        warm > 0 ? pluralise(count(text, WARM), "warm word") : "",
        plural > 0 ? pluralise(count(text, /\b(?:we|our|us)\b/gi), "first-person plural") : "",
      ].filter(Boolean),
    },
  ];

  candidates.sort((a, b) => b.score - a.score);
  const [first, second] = candidates;
  const total = first.score + (second?.score ?? 0);

  // Nothing counted means nothing to say; a couple of weak signals is a lean, not a
  // finding, so the label stays Neutral until one tone owns the text.
  if (!first || first.score < 1.2 || total === 0) {
    return { id: "neutral", label: LABELS.neutral, note: NOTES.neutral, signals: [], certainty: 0 };
  }

  const certainty = Math.max(35, Math.min(95, Math.round((first.score / total) * 100)));
  return {
    id: first.id,
    label: LABELS[first.id],
    note: NOTES[first.id],
    signals: first.signals.slice(0, 2),
    certainty,
  };
}

const pluralise = (n: number, noun: string): string => `${n} ${noun}${n === 1 ? "" : "s"}`;
