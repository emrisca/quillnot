import { useSyncExternalStore } from "react";

/**
 * Light and dark, chosen by hand and remembered.
 *
 * The window opens in whatever the OS is set to, and follows the OS until someone
 * presses the switch — at that moment the choice is stored and the OS stops being
 * consulted. `index.html` applies the same rule before the first paint, so the
 * window never flashes the wrong theme on launch.
 */
export type Theme = "light" | "dark";

const STORAGE_KEY = "quillnot:theme";
const listeners = new Set<() => void>();

function currentFromDocument(): Theme {
  if (typeof document === "undefined") return "light";
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
}

let current: Theme = currentFromDocument();

function apply(theme: Theme) {
  current = theme;
  if (typeof document !== "undefined") document.documentElement.dataset.theme = theme;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The stored choice, or null when the person has never pressed the switch. */
export function storedTheme(): Theme | null {
  try {
    const value = localStorage.getItem(STORAGE_KEY);
    return value === "dark" || value === "light" ? value : null;
  } catch {
    return null;
  }
}

export function setTheme(theme: Theme) {
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // A blocked store only costs the choice at next launch.
  }
  apply(theme);
}

export function toggleTheme() {
  setTheme(current === "dark" ? "light" : "dark");
}

export function useTheme(): Theme {
  return useSyncExternalStore(
    subscribe,
    () => current,
    () => current,
  );
}

// Follow the system while no choice has been made, and stop the moment one is.
if (typeof window !== "undefined" && typeof window.matchMedia === "function") {
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  media.addEventListener("change", (event) => {
    if (!storedTheme()) apply(event.matches ? "dark" : "light");
  });
}
