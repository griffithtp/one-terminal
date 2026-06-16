/**
 * App Directory settings store
 *
 * Per-machine override for the App Directory endpoint the launcher fetches
 * widgets from. Defaults to the built-in endpoint shipped in
 * `terminal.config.json` (`appDirectoryUrl`); the user may override it to any
 * HTTP(S) endpoint from the App Menu → App Directory section.
 *
 * Backed by localStorage with a tiny pub/sub — mirrors `userSettingsStore` so
 * catalog consumers (useAppLaunch / useAppDirectory) can subscribe and re-fetch
 * live when the override changes, no restart required.
 *
 * Storage key: `one-terminal:app-directory` — same prefix as `user-settings` /
 * `theme` / `keybindings`.
 */

const STORAGE_KEY = "one-terminal:app-directory";

export interface AppDirectorySettings {
  /** User override for the App Directory URL. Empty string = use the default. */
  urlOverride: string;
}

export const DEFAULT_APP_DIRECTORY_SETTINGS: AppDirectorySettings = {
  urlOverride: "",
};

/** Coerces an unknown blob into a valid AppDirectorySettings. */
function sanitise(raw: unknown): AppDirectorySettings {
  const obj = (typeof raw === "object" && raw !== null ? raw : {}) as Record<string, unknown>;
  return {
    urlOverride: typeof obj.urlOverride === "string" ? obj.urlOverride : "",
  };
}

// ── Cached in-memory state ──────────────────────────────────────────────────

let cache: AppDirectorySettings | null = null;

function readFromStorage(): AppDirectorySettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_APP_DIRECTORY_SETTINGS };
    return sanitise(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_APP_DIRECTORY_SETTINGS };
  }
}

function writeToStorage(settings: AppDirectorySettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    /* localStorage may be unavailable in private browsing — silent */
  }
}

export function loadAppDir(): AppDirectorySettings {
  if (!cache) cache = readFromStorage();
  return cache;
}

export function saveAppDir(patch: Partial<AppDirectorySettings>): AppDirectorySettings {
  const next = sanitise({ ...loadAppDir(), ...patch });
  cache = next;
  writeToStorage(next);
  notify(next);
  return next;
}

/**
 * Resolve the effective App Directory URL: the user override (trimmed) when
 * set, otherwise the supplied built-in default from the terminal config.
 */
export function resolveEffectiveUrl(defaultUrl: string): string {
  const override = loadAppDir().urlOverride.trim();
  return override || defaultUrl;
}

// ── Subscription ────────────────────────────────────────────────────────────

type Listener = (s: AppDirectorySettings) => void;
const listeners = new Set<Listener>();

export function subscribe(cb: Listener): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

function notify(s: AppDirectorySettings): void {
  for (const l of listeners) l(s);
}
