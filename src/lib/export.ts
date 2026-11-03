import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./openrouter";
import { modeLabel, type Mode } from "./paraphrase";
import { countWords } from "./text";

/**
 * A run as a Markdown file.
 *
 * Both texts go in, original first and rewrite second, so the file is worth keeping
 * even if the reader never opens QuillNot again — a rewrite on its own loses the
 * only thing that gives it meaning, which is what it was before.
 */
export type ExportInput = {
  original: string;
  result: string;
  mode: Mode;
  model: string | null;
  /** Unix seconds; defaults to now. */
  at?: number;
};

export function markdown({ original, result, mode, model, at }: ExportInput): string {
  const when = new Date((at ?? Date.now() / 1000) * 1000);
  const date = when.toISOString().slice(0, 10);
  const inWords = countWords(original);
  const outWords = countWords(result);

  return [
    `# ${modeLabel(mode)} rewrite`,
    "",
    `_${inWords} words → ${outWords} words · ${model ?? "mock output"} · ${date}_`,
    "",
    "## Rewritten",
    "",
    result.trim(),
    "",
    "## Original",
    "",
    original.trim(),
    "",
    "---",
    "",
    "Exported from QuillNot. Rewritten by a free model on your own OpenRouter account.",
    "",
  ].join("\n");
}

/** `quillnot-formal-20261103-1015`, which `export.rs` turns into a file name. */
export function fileStem(mode: Mode, at = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, "0");
  const stamp = `${at.getFullYear()}${pad(at.getMonth() + 1)}${pad(at.getDate())}-${pad(at.getHours())}${pad(at.getMinutes())}`;
  return `quillnot-${mode}-${stamp}`;
}

/**
 * Writes the file and answers with its path, or null when the browser took the
 * download — a preview has no Downloads folder to report, but it can still save.
 */
export async function exportMarkdown(name: string, body: string): Promise<string | null> {
  if (isDesktop()) return invoke<string>("save_markdown", { name, body });

  const url = URL.createObjectURL(new Blob([body], { type: "text/markdown" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name.endsWith(".md") ? name : `${name}.md`;
  link.click();
  URL.revokeObjectURL(url);
  return null;
}

/** The last path segment, for a message that should not wrap. */
export const fileName = (path: string): string => path.split(/[\\/]/).pop() ?? path;
