import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./openrouter";

/**
 * What the person did with a reply. This is the only quality signal the ranking has:
 * a kept reply is the model being right, an undone one is the model being wrong in a
 * way somebody had to fix, and a rerun is "close, try again".
 */
export type Outcome = "kept" | "undone" | "rerun";

export type OutcomeTask = "paraphrase" | "grammar";

/**
 * Records a verdict against the newest reply from this model on this task. Quiet on
 * purpose: a sample is evidence for a future ranking, not something worth interrupting
 * a person over, and a browser preview has no log to write to.
 */
export async function recordOutcome(
  model: string | null,
  task: OutcomeTask,
  outcome: Outcome,
): Promise<void> {
  if (!isDesktop() || !model) return;
  try {
    await invoke<boolean>("record_outcome", { model, task, outcome });
  } catch {
    // Nothing here is worth a dialog.
  }
}
