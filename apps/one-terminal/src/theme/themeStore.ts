/**
 * Theme store
 *
 * Persists the user's theme choice (light / dark / system) to localStorage
 * and applies it to <html> via the `data-theme` attribute. CSS in wm.css
 * defines colour tokens on `:root` (dark) and `[data-theme="light"]`.
 *
 * "system" tracks `prefers-color-scheme` and re-applies on OS theme flip.
 *
 * Call `applyTheme(loadTheme())` in main.tsx *before* the first React render
 * so the first paint matches the persisted choice (no flash).
 */

const STORAGE_KEY = "one-terminal:theme";

export type ThemeChoice = "light" | "dark" | "system";
export type EffectiveTheme = "light" | "dark";

const VALID: readonly ThemeChoice[] = ["light", "dark", "system"];

function isThemeChoice(v: unknown): v is ThemeChoice {
  return typeof v === "string" && (VALID as readonly string[]).includes(v);
}

export function loadTheme(): ThemeChoice {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return isThemeChoice(raw) ? raw : "system";
  } catch {
    return "system";
  }
}

export function saveTheme(choice: ThemeChoice): void {
  try {
    localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    /* localStorage may be unavailable in private browsing — ignore */
  }
}

function resolveEffective(choice: ThemeChoice): EffectiveTheme {
  if (choice === "light" || choice === "dark") return choice;
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

// Tracks the currently-attached matchMedia listener so we can detach it when
// the user switches away from "system".
let mqlListener: ((e: MediaQueryListEvent) => void) | null = null;
let mql: MediaQueryList | null = null;

function detachSystemListener() {
  if (mql && mqlListener) {
    mql.removeEventListener("change", mqlListener);
  }
  mql = null;
  mqlListener = null;
}

export function applyTheme(choice: ThemeChoice): void {
  detachSystemListener();
  document.documentElement.dataset.theme = resolveEffective(choice);

  if (choice === "system" && typeof window.matchMedia === "function") {
    mql = window.matchMedia("(prefers-color-scheme: dark)");
    mqlListener = (e) => {
      document.documentElement.dataset.theme = e.matches ? "dark" : "light";
    };
    mql.addEventListener("change", mqlListener);
  }
}

/**
 * Subscribe to changes in the effective theme. Fires on `applyTheme()` calls
 * and on OS theme flips while in "system" mode. Returns an unsubscribe fn.
 *
 * Implemented via a MutationObserver on <html>'s data-theme so consumers
 * don't need to know about the matchMedia plumbing.
 */
export function subscribeEffectiveTheme(cb: (theme: EffectiveTheme) => void): () => void {
  const observer = new MutationObserver(() => {
    const t = document.documentElement.dataset.theme;
    if (t === "light" || t === "dark") cb(t);
  });
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
  return () => observer.disconnect();
}
