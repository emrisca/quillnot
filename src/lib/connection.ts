import { useSyncExternalStore } from "react";
import { isDesktop, keyStatus, type KeyStatus } from "./openrouter";

/**
 * One shared copy of the account status.
 *
 * The header badge and the settings card both need it, and both must agree: a
 * second source would let the badge say Live while Settings says otherwise. Mutable
 * module state plus `useSyncExternalStore` is the smallest thing that does that
 * without a provider or a store library.
 */
let status: KeyStatus = { connected: false, hint: null };
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function setStatus(next: KeyStatus) {
  status = next;
  emit();
}

/** Asks the app for the current status. A browser preview simply stays disconnected. */
export async function refreshStatus() {
  if (!isDesktop()) return;
  try {
    setStatus(await keyStatus());
  } catch {
    // Leave the last known value: a failed probe is not proof of a disconnect.
  }
}

export function useConnection(): KeyStatus {
  return useSyncExternalStore(
    subscribe,
    () => status,
    () => status,
  );
}
