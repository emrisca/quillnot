export type Piece = { text: string; changed: boolean };

/** A word and the whitespace that followed it, so joining pieces restores the text exactly. */
const TOKEN = /\S+\s*/g;

const tokenize = (text: string): string[] => text.match(TOKEN) ?? [];
const keyOf = (token: string): string => token.trim().toLowerCase();

// Past this many tokens per side the quadratic table stops being worth it on the
// main thread; the whole text is reported as changed instead, which is honest if
// less precise.
const LIMIT = 900;

/**
 * Longest-common-subsequence lengths, indexed from the end so the walk can run
 * forwards. Shared by the word diff and the sentence comparison: both are the same
 * question asked at different sizes.
 */
function lcsTable(a: string[], b: string[]): Int32Array {
  const width = b.length + 1;
  const table = new Int32Array((a.length + 1) * width);

  for (let i = a.length - 1; i >= 0; i -= 1) {
    for (let j = b.length - 1; j >= 0; j -= 1) {
      table[i * width + j] =
        a[i] === b[j]
          ? table[(i + 1) * width + j + 1] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }
  return table;
}

function group(tokens: string[], inLcs: Uint8Array): Piece[] {
  const pieces: Piece[] = [];
  tokens.forEach((token, index) => {
    // Whitespace never carries a highlight of its own: doing so paints a green
    // block over the line break and the paragraph looks broken rather than edited.
    const changed = inLcs[index] === 0 && token.trim().length > 0;
    const last = pieces[pieces.length - 1];
    if (last && last.changed === changed) last.text += token;
    else pieces.push({ text: token, changed });
  });
  return pieces;
}

/**
 * Word-level longest-common-subsequence between the original and the rewrite.
 *
 * Only words present on both sides are unchanged, so a phrase the model replaced
 * ("shows" → "demonstrates") reads as one highlighted span rather than a trail of
 * single letters. Comparison is case- and whitespace-insensitive after
 * tokenising, but the pieces keep the original text.
 */
export function diffWords(before: string, after: string): { before: Piece[]; after: Piece[] } {
  const a = tokenize(before);
  const b = tokenize(after);

  if (a.length === 0 || b.length === 0 || a.length > LIMIT || b.length > LIMIT) {
    return {
      before: [{ text: before, changed: before.trim().length > 0 }],
      after: [{ text: after, changed: after.trim().length > 0 }],
    };
  }

  const ka = a.map(keyOf);
  const kb = b.map(keyOf);
  const width = b.length + 1;
  const table = lcsTable(ka, kb);

  const beforeInLcs = new Uint8Array(a.length);
  const afterInLcs = new Uint8Array(b.length);
  let i = 0;
  let j = 0;
  while (i < a.length && j < b.length) {
    if (ka[i] === kb[j]) {
      beforeInLcs[i] = 1;
      afterInLcs[j] = 1;
      i += 1;
      j += 1;
    } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
      i += 1;
    } else {
      j += 1;
    }
  }

  return { before: group(a, beforeInLcs), after: group(b, afterInLcs) };
}

/* ------------------------------------------------------- comparing sentences */

export type SentencePair = {
  /** The sentence in the original, or null when the rewrite added one. */
  before: string | null;
  /** The sentence in the rewrite, or null when it was dropped. */
  after: string | null;
  changed: boolean;
};

/**
 * A terminator only ends a sentence when whitespace follows it, so a version number
 * like 12.1.0 stays one sentence instead of splitting into three.
 */
const sentences = (text: string): string[] =>
  text
    .split("\n")
    .flatMap((line) => line.split(/(?<=[.!?])(?=\s)/))
    .map((sentence) => sentence.replace(/\s+/g, " ").trim())
    .filter(Boolean);

/** Ignores case and punctuation, so "The cat sat." and "the cat sat" are one sentence. */
const sentenceKey = (sentence: string): string =>
  sentence
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s]/gu, "")
    .replace(/\s+/g, " ")
    .trim();

/**
 * The original and the rewrite, sentence by sentence.
 *
 * A word diff answers "what changed in this sentence"; this answers "which sentences
 * changed" — the question a writer actually asks before deciding whether a rewrite is
 * worth keeping. Sentences that survive intact are paired, and a run of replacements
 * is paired in order, so a reworded paragraph reads as a paragraph rather than as
 * deletions followed by additions.
 */
export function compareSentences(before: string, after: string): SentencePair[] {
  const a = sentences(before);
  const b = sentences(after);

  if (a.length === 0 || b.length === 0 || a.length > LIMIT || b.length > LIMIT) {
    return [
      {
        before: before.trim() || null,
        after: after.trim() || null,
        changed: true,
      },
    ];
  }

  const ka = a.map(sentenceKey);
  const kb = b.map(sentenceKey);
  const width = b.length + 1;
  const table = lcsTable(ka, kb);

  const pairs: SentencePair[] = [];
  let i = 0;
  let j = 0;

  while (i < a.length || j < b.length) {
    if (i >= a.length || j >= b.length) {
      // One side ran out: whatever is left was added or dropped wholesale.
      for (; i < a.length; i += 1) pairs.push({ before: a[i], after: null, changed: true });
      for (; j < b.length; j += 1) pairs.push({ before: null, after: b[j], changed: true });
      break;
    }

    if (ka[i] === kb[j]) {
      pairs.push({ before: a[i], after: b[j], changed: false });
      i += 1;
      j += 1;
      continue;
    }

    // Walk to the next sentence that survived on both sides, then pair the run.
    const startI = i;
    const startJ = j;
    while (i < a.length && j < b.length && ka[i] !== kb[j]) {
      if (table[(i + 1) * width + j] >= table[i * width + j + 1]) i += 1;
      else j += 1;
    }

    const removed = a.slice(startI, i);
    const added = b.slice(startJ, j);
    for (let k = 0; k < Math.max(removed.length, added.length); k += 1) {
      pairs.push({ before: removed[k] ?? null, after: added[k] ?? null, changed: true });
    }
  }

  return pairs;
}
