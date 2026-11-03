import type { IconName } from "../icons";

export type ToolId =
  | "paraphraser"
  | "grammar"
  | "humanizer"
  | "typer"
  | "translate"
  | "models"
  | "settings";

export type RailItem = {
  key: string;
  label: string;
  icon: IconName;
  /** Where the item goes, when it goes anywhere yet. */
  tool?: ToolId;
  /** Present but not built: it renders dimmed and says so on hover. */
  soon?: boolean;
};

// Only what this app can actually do. Paraphrasing, grammar checking, translating, Typer
// and the model catalogue are real, and summarising is a mode of the paraphraser rather
// than a tool of its own, because it is the same request with a different prompt. The AI
// detector, plagiarism checker and humaniser were removed rather than faked: the first
// two need a trained classifier and a corpus of sources, and the third exists only to
// defeat the first.
export const RAIL: RailItem[] = [
  { key: "projects", label: "Projects", icon: "projects", soon: true },
  { key: "paraphraser", label: "Paraphraser", icon: "paraphrase", tool: "paraphraser" },
  { key: "grammar", label: "Grammar Checker", icon: "grammar", tool: "grammar" },
  // The humanizer is a rewrite whose prompt is a pane rather than a mode: the skill it
  // shows is what the model is sent, so it can be edited and it is remembered.
  { key: "humanizer", label: "AI Humanizer", icon: "humanizer", tool: "humanizer" },
  // Typer mimics human typing (plan §4); AI Models is the scored free-endpoint
  // catalogue (§3).
  { key: "typer", label: "Typer", icon: "typer", tool: "typer" },
  // Translate is a rewrite with a language in place of a mode, so it shares the same
  // command, provider routing and history.
  { key: "translate", label: "Translate", icon: "translate", tool: "translate" },
  { key: "models", label: "AI Models", icon: "models", tool: "models" },
  { key: "more", label: "More", icon: "more", soon: true },
];

export const RAIL_FOOTER: RailItem[] = [
  { key: "settings", label: "Settings", icon: "settings", tool: "settings" },
  { key: "feedback", label: "Feedback", icon: "feedback", soon: true },
];

export const TITLES: Record<ToolId, string> = {
  paraphraser: "Paraphrasing Tool",
  grammar: "Grammar Checker",
  humanizer: "AI Humanizer",
  typer: "Typer",
  translate: "Translate",
  models: "AI Models",
  settings: "Settings",
};
