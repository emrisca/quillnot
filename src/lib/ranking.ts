import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./openrouter";

/** The tasks the score is tuned for. Each keeps its own weights. */
export type RankingTask = "paraphrase" | "grammar";

/** Matches `TaskWeights` in `src-tauri/src/store.rs`. */
export type TaskWeights = {
  /** Reported uptime, falling back to our own success ratio. */
  availability: number;
  /** Median first-token latency and throughput, log-scaled. */
  speed: number;
  /** Kept, undone and rerun verdicts: the signal no provider reports. */
  quality: number;
};

/** Matches `Penalties` in `src-tauri/src/store.rs`, field names included. */
export type Penalties = {
  failure: number;
  rejection: number;
  rate_limit: number;
};

export const DEFAULT_WEIGHTS: TaskWeights = { availability: 0.5, speed: 0.3, quality: 0.2 };

export const DEFAULT_PENALTIES: Penalties = { failure: 6, rejection: 8, rate_limit: 10 };

export const RANKING_TASKS: { id: RankingTask; label: string }[] = [
  { id: "paraphrase", label: "Paraphrase" },
  { id: "grammar", label: "Grammar" },
];

const notDesktop = () => new Error("The ranking is stored by the QuillNot app, not by this browser preview.");

/** The weights one task is scored with right now, defaults included. */
export async function taskWeights(task: RankingTask): Promise<TaskWeights> {
  if (!isDesktop()) return DEFAULT_WEIGHTS;
  return invoke<TaskWeights>("task_weights", { task });
}

/** Saves one task's weights. The Rust side clamps them, so the reply is the stored value. */
export async function setTaskWeights(task: RankingTask, weights: TaskWeights): Promise<TaskWeights> {
  if (!isDesktop()) throw notDesktop();
  return invoke<TaskWeights>("set_task_weights", { task, weights });
}

export async function penalties(): Promise<Penalties> {
  if (!isDesktop()) return DEFAULT_PENALTIES;
  return invoke<Penalties>("penalties");
}

export async function setPenalties(penalties: Penalties): Promise<Penalties> {
  if (!isDesktop()) throw notDesktop();
  return invoke<Penalties>("set_penalties", { penalties });
}
