import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Whether this window is inside Tauri or a plain browser (the Vite preview). */
export const isDesktop = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type KeyStatus = { connected: boolean; hint: string | null };

export type FreeModel = {
  id: string;
  name: string | null;
  context_length: number | null;
  pricing: { prompt: string | null; completion: string | null } | null;
};

export type ModelEndpoint = {
  provider_name: string | null;
  context_length: number | null;
  uptime_last_30m: number | null;
  latency_last_30m: number | null;
  throughput_last_30m: number | null;
};

export type ParaphraseRequest = {
  text: string;
  mode: string;
  strength: number;
  model?: string;
};

export type ParaphraseResult = { text: string; model: string; blocks: number };

export type Delta = { block: number; text: string; model: string };

/** Matches `DELTA_EVENT` in `src-tauri/src/lib.rs`. */
const DELTA_EVENT = "paraphrase:delta";

const notDesktop = (what: string) =>
  new Error(`${what} needs the QuillNot app; this browser preview runs on mock output.`);

export async function keyStatus(): Promise<KeyStatus> {
  if (!isDesktop()) return { connected: false, hint: null };
  return invoke<KeyStatus>("key_status");
}

export async function saveKey(key: string): Promise<KeyStatus> {
  if (!isDesktop()) throw notDesktop("Saving a key");
  return invoke<KeyStatus>("save_key", { key });
}

export async function clearKey(): Promise<KeyStatus> {
  if (!isDesktop()) throw notDesktop("Disconnecting");
  return invoke<KeyStatus>("clear_key");
}

/** Opens the browser for OpenRouter's PKCE flow and settles when the key is stored. */
export async function signIn(): Promise<KeyStatus> {
  if (!isDesktop()) throw notDesktop("Signing in");
  return invoke<KeyStatus>("sign_in");
}

export async function freeModels(): Promise<FreeModel[]> {
  if (!isDesktop()) throw notDesktop("Listing models");
  return invoke<FreeModel[]>("list_free_models");
}

export async function modelEndpoints(id: string): Promise<ModelEndpoint[]> {
  if (!isDesktop()) throw notDesktop("Reading endpoints");
  return invoke<ModelEndpoint[]>("model_endpoints", { id });
}

export async function paraphrase(request: ParaphraseRequest): Promise<ParaphraseResult> {
  if (!isDesktop()) throw notDesktop("Paraphrasing");
  return invoke<ParaphraseResult>("paraphrase", { request });
}

/** Streams each block's tokens in as the model produces them. Returns an unsubscribe. */
export async function onDelta(handler: (delta: Delta) => void): Promise<UnlistenFn> {
  return listen<Delta>(DELTA_EVENT, (event) => handler(event.payload));
}
