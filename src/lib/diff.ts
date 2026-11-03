export type Piece = { text: string; changed: boolean };

/** A word and the whitespace that followed it, so joining pieces restores the text exactly. */
const TOKEN = /\S+\s*/g;

const tokenize = (text: string): string[] => text.match(TOKEN) ?? [];
const keyOf = (token: string): string => token.trim().toLowerCase();

// Past this many tokens per side the quadratic table stops being worth it on the
// main thread; the whole text is reported as changed instead, which is honest if
// less precise.
const LIMIT = 900;

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

  const n = a.length;
  const m = b.length;
  const width = m + 1;
  const table = new Int32Array((n + 1) * width);
  const ka = a.map(keyOf);
  const kb = b.map(keyOf);

  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      table[i * width + j] =
        ka[i] === kb[j]
          ? table[(i + 1) * width + j + 1] + 1
          : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
    }
  }

  const beforeInLcs = new Uint8Array(n);
  const afterInLcs = new Uint8Array(m);
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
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
