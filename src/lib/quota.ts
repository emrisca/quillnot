import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** What a provider said is left of this window's allowance, after a call. */
export type Quota = {
  provider: string;
  remaining: number | null;
  limit: number | null;
};

/** Matches `QUOTA_EVENT` in `src-tauri/src/openrouter.rs`. */
export const QUOTA_EVENT = "quota:update";

/**
 * Providers disagree about which headers they send, and one that sends none leaves the
 * last reading alone rather than blanking it, so this only fires when there is news.
 */
export async function onQuota(handler: (quota: Quota) => void): Promise<UnlistenFn> {
  return listen<Quota>(QUOTA_EVENT, (event) => handler(event.payload));
}
