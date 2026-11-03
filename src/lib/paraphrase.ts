/**
 * The eight rewrite modes, and the transport behind them.
 *
 * `mockParaphrase` is the only thing here that is not real: it rewrites text with
 * a small dictionary so the workspace can be judged before an API exists. M2
 * replaces its body with `invoke("paraphrase", { text, mode, strength })`, keeps
 * the signature, and streams the reply in through events — every component that
 * calls it stays as it is. The mode labels, hints and the shape of `MODES` are
 * final and are what the Rust prompt builder is keyed on.
 */
export type Mode =
  | "standard"
  | "fluency"
  | "formal"
  | "simple"
  | "creative"
  | "expand"
  | "shorten"
  | "summarize"
  | "custom";

export const MODES: { id: Mode; label: string; hint: string }[] = [
  { id: "standard", label: "Standard", hint: "Balanced rewording that keeps your meaning and your length." },
  { id: "fluency", label: "Fluency", hint: "Smooths awkward phrasing and joins sentences that should be one." },
  { id: "formal", label: "Formal", hint: "Academic register: no contractions, heavier vocabulary." },
  { id: "simple", label: "Simple", hint: "Short sentences and plain words, for a reader in a hurry." },
  { id: "creative", label: "Creative", hint: "Livelier verbs and fresher comparisons, same argument." },
  { id: "expand", label: "Expand", hint: "Adds detail and connective tissue to grow the text." },
  { id: "shorten", label: "Shorten", hint: "Cuts filler so the point arrives sooner." },
  { id: "summarize", label: "Summarize", hint: "The point in at most a third of the length, using only what is there." },
  { id: "custom", label: "Custom", hint: "Follow your own written instruction — wired up in M2." },
];

export const modeLabel = (mode: Mode): string => MODES.find((m) => m.id === mode)?.label ?? mode;

export const modeHint = (mode: Mode): string => MODES.find((m) => m.id === mode)?.hint ?? "";

/** [find, replace] pairs, applied once each, first occurrence, case preserved. */
const PAIRS: Record<Mode, [string, string][]> = {
  standard: [
    ["in order to", "to"],
    ["a lot of", "plenty of"],
    ["utilise", "use"],
    ["utilize", "use"],
    ["very important", "essential"],
    ["helps", "assists"],
    ["shows", "demonstrates"],
    ["now rendered", "drawn"],
    ["compatible with", "in line with"],
    ["can be", "may be"],
    ["also", "as well"],
  ],
  fluency: [
    ["which is compatible with", "built for"],
    ["are now", "are now"],
    ["and they are", "and are"],
    ["in order to", "to"],
    ["due to the fact that", "because"],
    ["at this point in time", "now"],
    ["the reason why is that", "because"],
  ],
  formal: [
    ["a lot of", "a considerable number of"],
    ["get", "acquire"],
    ["big", "substantial"],
    ["show", "demonstrate"],
    ["help", "facilitate"],
    ["use", "employ"],
    ["but", "however"],
    ["so", "therefore"],
    ["really", "markedly"],
    ["don't", "do not"],
    ["can't", "cannot"],
    ["it's", "it is"],
  ],
  simple: [
    ["utilise", "use"],
    ["utilize", "use"],
    ["demonstrate", "show"],
    ["approximately", "about"],
    ["commence", "start"],
    ["terminate", "end"],
    ["facilitate", "help"],
    ["numerous", "many"],
    ["obtain", "get"],
    ["sufficient", "enough"],
    ["additional", "more"],
    ["purchase", "buy"],
  ],
  creative: [
    ["big", "colossal"],
    ["fast", "lightning-quick"],
    ["important", "pivotal"],
    ["shows", "paints a picture of"],
    ["idea", "notion"],
    ["problem", "snag"],
    ["improves", "sharpens"],
  ],
  expand: [
    ["shows", "clearly demonstrates"],
    ["important", "of real importance"],
    ["because", "for the simple reason that"],
    ["helps", "goes a long way towards helping"],
    ["new", "brand new"],
  ],
  shorten: [
    ["in order to", "to"],
    ["a lot of", "many"],
    ["at this point in time", "now"],
    ["due to the fact that", "because"],
    ["in the event that", "if"],
    ["make a decision", "decide"],
    ["has the ability to", "can"],
    ["is able to", "can"],
  ],
  summarize: [],
  custom: [],
};

const escapeRegExp = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

function matchCase(source: string, replacement: string): string {
  if (source === source.toUpperCase() && source.length > 1) return replacement.toUpperCase();
  if (source[0] === source[0]?.toUpperCase()) {
    return replacement[0]?.toUpperCase() + replacement.slice(1);
  }
  return replacement;
}

function swap(text: string, find: string, replace: string): string {
  const pattern = new RegExp(`\\b${escapeRegExp(find)}\\b`, "i");
  return text.replace(pattern, (hit) => matchCase(hit, replace));
}

const tidy = (text: string): string =>
  text
    .replace(/[ \t]{2,}/g, " ")
    .replace(/ +\n/g, "\n")
    .replace(/(^|[.!?]\s+)([a-z])/g, (_all, lead: string, letter: string) => `${lead}${letter.toUpperCase()}`);

function expand(text: string): string {
  return text
    .split(/(?<=[.!?])\s+/)
    .map((sentence, index) => (index % 2 === 1 && sentence.length > 40 ? `In practice, ${sentence[0]?.toLowerCase()}${sentence.slice(1)}` : sentence))
    .join(" ");
}

function shorten(text: string): string {
  return text
    .replace(/\b(?:very|really|actually|basically|simply|quite|rather)\s+/gi, "")
    .replace(/\s+/g, " ")
    .trim();
}

function joinShortSentences(text: string): string {
  const sentences = text.split(/(?<=[.!?])\s+/);
  const out: string[] = [];
  for (const sentence of sentences) {
    const previous = out[out.length - 1];
    if (previous && previous.length < 60 && sentence.length < 90 && !/[!?]$/.test(previous)) {
      out[out.length - 1] = `${previous.replace(/\.$/, "")}, and ${sentence[0]?.toLowerCase()}${sentence.slice(1)}`;
    } else {
      out.push(sentence);
    }
  }
  return out.join(" ");
}

function summarize(text: string): string {
  const sentences = text.split(/(?<=[.!?])\s+/).filter((sentence) => sentence.trim().length > 0);
  if (sentences.length <= 2) return sentences.join(" ");
  // Two fifths of the sentences, rounded up, so a three-sentence text loses nothing.
  return sentences.slice(0, Math.max(1, Math.ceil(sentences.length * 0.4))).join(" ");
}

/**
 * M1's stand-in for the model. Deterministic, offline, and clearly labelled as a
 * mock in the UI — the `Mock` badge in the header exists so this never passes for
 * a real rewrite.
 */
export async function mockParaphrase(text: string, mode: Mode, strength: number): Promise<string> {
  await new Promise((resolve) => setTimeout(resolve, 380 + Math.min(text.length, 500)));

  const pairs = PAIRS[mode];
  const take = mode === "custom" ? 0 : Math.min(pairs.length, Math.max(2, Math.round(pairs.length * (0.35 + strength * 0.18))));

  let output = text;
  for (const [find, replace] of pairs.slice(0, take)) output = swap(output, find, replace);

  if (mode === "expand") output = expand(output);
  if (mode === "shorten") output = shorten(output);
  if (mode === "fluency") output = joinShortSentences(output);
  if (mode === "summarize") output = summarize(output);

  return tidy(output);
}
