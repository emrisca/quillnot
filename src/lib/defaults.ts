import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./openrouter";

/**
 * The model every run starts with, or null for whatever the provider picks. The router in
 * Rust has read this field all along; these two calls are what finally write it.
 */
export async function defaultModel(): Promise<string | null> {
  if (!isDesktop()) return null;
  return invoke<string | null>("default_model");
}

/** An empty string clears the setting rather than pinning a model called "". */
export async function setDefaultModel(model: string): Promise<void> {
  if (!isDesktop()) return;
  await invoke("set_default_model", { model });
}
