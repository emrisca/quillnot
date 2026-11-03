/**
 * The grammar checker's data model, and a local rule pass that fills it.
 *
 * The important part is the shape: every issue carries a quote, a replacement
 * and a *verified* position in the text. M3 asks a free model for
 * `{original, replacement, message}` triples and does exactly what
 * `findIssues` does here — searches for the quote itself rather than trusting a
 * model's character offsets, which are wrong almost every time. Anything that
 * cannot be located unambiguously is reported without a position instead of
 * being spliced into the wrong sentence.
 */
export type IssueKind = "spelling" | "grammar" | "punctuation" | "style";

export type Issue = {
  id: string;
  kind: IssueKind;
  message: string;
  quote: string;
  suggestion: string;
  start: number;
  end: number;
};

type Rule = {
  kind: IssueKind;
  message: string;
  pattern: RegExp;
  replace: (hit: RegExpExecArray) => string;
};

const CONTRACTIONS: Record<string, string> = {
  dont: "don't",
  doesnt: "doesn't",
  cant: "can't",
  wont: "won't",
  isnt: "isn't",
  arent: "aren't",
  didnt: "didn't",
  wouldnt: "wouldn't",
  couldnt: "couldn't",
  shouldnt: "shouldn't",
  its: "it's",
};

const RULES: Rule[] = [
  { kind: "spelling", message: "Misspelling of “the”.", pattern: /\bteh\b/gi, replace: () => "the" },
  { kind: "spelling", message: "“a lot” is two words.", pattern: /\balot\b/gi, replace: () => "a lot" },
  {
    kind: "spelling",
    message: "This contraction is missing its apostrophe.",
    pattern: /\b(dont|doesnt|cant|wont|isnt|arent|didnt|wouldnt|couldnt|shouldnt)\b/gi,
    replace: (hit) => CONTRACTIONS[hit[1].toLowerCase()] ?? hit[1],
  },
  {
    kind: "grammar",
    message: "The same word appears twice in a row.",
    pattern: /\b(\w+)\s+\1\b/gi,
    replace: (hit) => hit[1],
  },
  { kind: "punctuation", message: "Two spaces where one belongs.", pattern: / {2,}/g, replace: () => " " },
  {
    kind: "punctuation",
    message: "No space after the comma.",
    pattern: /,(?=[A-Za-z])/g,
    replace: () => ", ",
  },
  {
    kind: "punctuation",
    message: "A space sits before the punctuation mark.",
    pattern: / +([,.;:!?])/g,
    replace: (hit) => hit[1],
  },
  {
    kind: "style",
    message: "The pronoun “I” is always capitalised.",
    pattern: /\bi\b/g,
    replace: () => "I",
  },
  {
    kind: "style",
    message: "“very” makes the adjective work harder than it needs to.",
    pattern: /\bvery\s+(\w+)/gi,
    replace: (hit) => hit[1],
  },
];

/**
 * Runs every rule, keeps non-overlapping matches and caps the list so one
 * document cannot paint a thousand cards. Positions are the ones the regex
 * actually matched, which is the property the model path has to reproduce.
 */
export function findIssues(text: string, limit = 24): Issue[] {
  const found: Issue[] = [];

  for (const rule of RULES) {
    const pattern = new RegExp(rule.pattern.source, rule.pattern.flags);
    let hit = pattern.exec(text);
    while (hit) {
      const quote = hit[0];
      const suggestion = rule.replace(hit);
      if (suggestion !== quote) {
        found.push({
          id: `${rule.kind}:${hit.index}:${quote}`,
          kind: rule.kind,
          message: rule.message,
          quote,
          suggestion,
          start: hit.index,
          end: hit.index + quote.length,
        });
      }
      if (rule.pattern.global === false) break;
      hit = pattern.exec(text);
    }
  }

  found.sort((a, b) => a.start - b.start);

  const kept: Issue[] = [];
  let reach = -1;
  for (const issue of found) {
    if (issue.start >= reach) {
      kept.push(issue);
      reach = issue.end;
    }
    if (kept.length >= limit) break;
  }
  return kept;
}

/** M1's stand-in for the model call; M3 keeps the signature and swaps the body. */
export async function mockCheck(text: string): Promise<Issue[]> {
  await new Promise((resolve) => setTimeout(resolve, 320 + Math.min(text.length, 400)));
  return findIssues(text);
}

export function applyIssue(text: string, issue: Issue): string {
  return text.slice(0, issue.start) + issue.suggestion + text.slice(issue.end);
}
