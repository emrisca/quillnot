import type { IconName } from "../icons";

export type ToolId = "paraphraser" | "grammar" | "settings";

export type RailItem = {
  key: string;
  label: string;
  icon: IconName;
  /** Where the item goes, when it goes anywhere yet. */
  tool?: ToolId;
  /** Present but not built: it renders dimmed and says so on hover. */
  soon?: boolean;
};

// Only what a text model can actually do. Paraphrasing and grammar checking are real;
// everything marked `soon` is planned on the same client — Typer (§4), the model
// catalogue (§3), summarising and translation, which are the same request with a
// different prompt. The AI detector, plagiarism checker and humaniser were removed
// rather than faked: the first two need a trained classifier and a corpus of sources,
// and the third exists only to defeat the first.
export const RAIL: RailItem[] = [
  { key: "projects", label: "Projects", icon: "projects", soon: true },
  { key: "paraphraser", label: "Paraphraser", icon: "paraphrase", tool: "paraphraser" },
  { key: "grammar", label: "Grammar Checker", icon: "grammar", tool: "grammar" },
  // Visible before it exists on purpose: the rail is the map of the app, and a
  // planned feature nobody can see is one nobody asks for. Typer mimics human
  // typing (plan §4); AI Models is the scored free-endpoint catalogue (§3).
  { key: "typer", label: "Typer", icon: "typer", soon: true },
  { key: "summarizer", label: "Summarizer", icon: "summarize", soon: true },
  { key: "translate", label: "Translate", icon: "translate", soon: true },
  { key: "models", label: "AI Models", icon: "models", soon: true },
  { key: "more", label: "More", icon: "more", soon: true },
];

export const RAIL_FOOTER: RailItem[] = [
  { key: "settings", label: "Settings", icon: "settings", tool: "settings" },
  { key: "feedback", label: "Feedback", icon: "feedback", soon: true },
];

export const TITLES: Record<ToolId, string> = {
  paraphraser: "Paraphrasing Tool",
  grammar: "Grammar Checker",
  settings: "Settings",
};
