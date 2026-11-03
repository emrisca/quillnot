import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./openrouter";

/**
 * One provider with a free tier, as the Rust registry describes it plus the local
 * state: whether a key is stored, whether it is the one answering rewrites, and which
 * model it will use.
 */
export type ProviderInfo = {
  id: string;
  label: string;
  baseUrl: string;
  freeTier: string;
  docs: string;
  /** Every key from this provider starts with this; null means unknown. */
  keyPrefix: string | null;
  supportsOAuth: boolean;
  defaultModel: string;
  /** Whether the provider is useless without a key — a local server is not. */
  needsKey: boolean;
  connected: boolean;
  hint: string | null;
  active: boolean;
  model: string;
};

const notDesktop = (what: string) =>
  new Error(`${what} needs the QuillNot app; this browser preview cannot reach the keychain.`);

export async function listProviders(): Promise<ProviderInfo[]> {
  if (!isDesktop()) return [];
  return invoke<ProviderInfo[]>("list_providers");
}

export async function saveProviderKey(provider: string, key: string): Promise<ProviderInfo[]> {
  if (!isDesktop()) throw notDesktop("Saving a key");
  return invoke<ProviderInfo[]>("save_provider_key", { provider, key });
}

export async function clearProviderKey(provider: string): Promise<ProviderInfo[]> {
  if (!isDesktop()) throw notDesktop("Removing a key");
  return invoke<ProviderInfo[]>("clear_provider_key", { provider });
}

export async function setActiveProvider(provider: string): Promise<ProviderInfo[]> {
  if (!isDesktop()) throw notDesktop("Choosing a provider");
  return invoke<ProviderInfo[]>("set_active_provider", { provider });
}

export async function setProviderModel(provider: string, model: string): Promise<ProviderInfo[]> {
  if (!isDesktop()) throw notDesktop("Choosing a model");
  return invoke<ProviderInfo[]>("set_provider_model", { provider, model });
}

export async function setCustomEndpoint(baseUrl: string, model: string): Promise<ProviderInfo[]> {
  if (!isDesktop()) throw notDesktop("Saving an endpoint");
  return invoke<ProviderInfo[]>("set_custom_endpoint", { baseUrl, model });
}

/** Opens a provider's docs in the system browser, never inside the app window. */
export async function openDocs(url: string): Promise<void> {
  if (!isDesktop()) return;
  await invoke("open_docs", { url });
}
