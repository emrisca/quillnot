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

// The rail is QuillBot's list, minus the tools that need a model we do not have
// (an AI detector without a detector is a lie). Everything marked `soon` is
// reachable later on the same client — summarise and humanise are the same
// request with a different prompt.
export const RAIL: RailItem[] = [
  { key: "projects", label: "Projects", icon: "projects", soon: true },
  { key: "paraphraser", label: "Paraphraser", icon: "paraphrase", tool: "paraphraser" },
  { key: "grammar", label: "Grammar Checker", icon: "grammar", tool: "grammar" },
  // Visible before it exists on purpose: the rail is the map of the app, and a
  // planned feature nobody can see is one nobody asks for. Typer mimics human
  // typing (plan §4); AI Models is the scored free-endpoint catalogue (§3).
  { key: "typer", label: "Typer", icon: "typer", soon: true },
  { key: "detector", label: "AI Detector", icon: "detector", soon: true },
  { key: "plagiarism", label: "Plagiarism Checker", icon: "plagiarism", soon: true },
  { key: "humanizer", label: "AI Humanizer", icon: "humanize", soon: true },
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
