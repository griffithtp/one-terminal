/**
 * User settings store
 *
 * Per-machine user preferences (date / time format etc.). Backed by
 * localStorage with a tiny pub/sub so consumers can re-render when settings
 * change.
 *
 * Storage key: `one-terminal:user-settings` — same prefix as `theme` /
 * `keybindings`.
 *
 * If a future setting needs to be readable from Rust (e.g. to format
 * timestamps server-side), migrate it to a Rust-backed store at that point.
 * Until then localStorage keeps the surface small and zero-IPC.
 */

const STORAGE_KEY = "one-terminal:user-settings";

export type DateFormat = "iso" | "us" | "eu" | "long";
export type TimeFormat = "12h" | "24h";
export type FirstDayOfWeek = "sunday" | "monday";

export interface UserSettings {
  dateFormat: DateFormat;
  timeFormat: TimeFormat;
  firstDayOfWeek: FirstDayOfWeek;
}

export const DEFAULT_SETTINGS: UserSettings = {
  dateFormat: "iso",
  timeFormat: "24h",
  firstDayOfWeek: "monday",
};

const VALID_DATE: readonly DateFormat[] = ["iso", "us", "eu", "long"];
const VALID_TIME: readonly TimeFormat[] = ["12h", "24h"];
const VALID_FDOW: readonly FirstDayOfWeek[] = ["sunday", "monday"];

function isInList<T extends string>(v: unknown, list: readonly T[]): v is T {
  return typeof v === "string" && (list as readonly string[]).includes(v);
}

/** Coerces an unknown blob into a valid UserSettings, falling back per-field. */
function sanitise(raw: unknown): UserSettings {
  const obj = (typeof raw === "object" && raw !== null ? raw : {}) as Record<string, unknown>;
  return {
    dateFormat: isInList(obj.dateFormat, VALID_DATE) ? obj.dateFormat : DEFAULT_SETTINGS.dateFormat,
    timeFormat: isInList(obj.timeFormat, VALID_TIME) ? obj.timeFormat : DEFAULT_SETTINGS.timeFormat,
    firstDayOfWeek: isInList(obj.firstDayOfWeek, VALID_FDOW)
      ? obj.firstDayOfWeek
      : DEFAULT_SETTINGS.firstDayOfWeek,
  };
}

// ── Cached in-memory state ──────────────────────────────────────────────────
//
// Loaded lazily on first access. Mutations write through to localStorage and
// fan out to subscribers. We avoid `storage` event handling: OneTerminal is
// single-window, and the chrome webview is the only consumer.

let cache: UserSettings | null = null;

function readFromStorage(): UserSettings {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...DEFAULT_SETTINGS };
    return sanitise(JSON.parse(raw));
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

function writeToStorage(settings: UserSettings): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
  } catch {
    /* localStorage may be unavailable in private browsing — silent */
  }
}

export function loadSettings(): UserSettings {
  if (!cache) cache = readFromStorage();
  return cache;
}

export function saveSettings(patch: Partial<UserSettings>): UserSettings {
  const next = sanitise({ ...loadSettings(), ...patch });
  cache = next;
  writeToStorage(next);
  notify(next);
  return next;
}

// ── Subscription ────────────────────────────────────────────────────────────

type Listener = (s: UserSettings) => void;
const listeners = new Set<Listener>();

export function subscribe(cb: Listener): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

function notify(s: UserSettings): void {
  for (const l of listeners) l(s);
}

// ── Formatters ──────────────────────────────────────────────────────────────
//
// Tiny pure formatters — predictable across locales (Intl.DateTimeFormat can
// produce surprises depending on the user's OS locale) and zero-dep. Exposed
// here so any consumer (UserSettingsSection's live preview, future widgets
// that render timestamps in the chrome) renders the same way.

const MONTH_NAMES = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

const DAY_NAMES = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

const pad2 = (n: number): string => String(n).padStart(2, "0");

export function formatDate(d: Date, format: DateFormat): string {
  const y = d.getFullYear();
  const m = d.getMonth() + 1;
  const day = d.getDate();
  switch (format) {
    case "iso":
      return `${y}-${pad2(m)}-${pad2(day)}`;
    case "us":
      return `${pad2(m)}/${pad2(day)}/${y}`;
    case "eu":
      return `${pad2(day)}/${pad2(m)}/${y}`;
    case "long":
      return `${MONTH_NAMES[d.getMonth()]} ${day}, ${y}`;
  }
}

export function formatTime(d: Date, format: TimeFormat): string {
  const h24 = d.getHours();
  const m = d.getMinutes();
  if (format === "24h") return `${pad2(h24)}:${pad2(m)}`;
  const h12 = h24 === 0 ? 12 : h24 > 12 ? h24 - 12 : h24;
  const suffix = h24 >= 12 ? "PM" : "AM";
  return `${h12}:${pad2(m)} ${suffix}`;
}

export function dayName(day: FirstDayOfWeek): string {
  return day === "sunday" ? DAY_NAMES[0] : DAY_NAMES[1];
}
