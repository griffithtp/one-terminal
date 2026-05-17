import { registry } from "./registry";

const STORAGE_KEY = "one-terminal:keybindings";

// Snapshot of each command's original (pre-override) keybinding.
// Populated the first time applyKeybindingOverrides() runs.
const defaultKeybindings = new Map<string, string | undefined>();

/** Returns the original default keybinding for a command id, or undefined if
 *  the command has no default or was never seen before overrides ran. */
export function getDefaultKeybinding(id: string): string | undefined {
  return defaultKeybindings.get(id);
}

/** Returns the saved command-id → keybinding overrides map, or {} on error. */
export function loadKeybindings(): Record<string, string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as Record<string, string>) : {};
  } catch {
    return {};
  }
}

/** Persists the full overrides map (command-id → keybinding). */
export function saveKeybindings(bindings: Record<string, string>): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(bindings));
}

/** Re-registers every command that has a saved keybinding override, replacing
 *  the default keybinding. Snapshots defaults first so Reset can restore them.
 *  Call this after all static commands are registered. */
export function applyKeybindingOverrides(): void {
  // Snapshot defaults before touching anything.
  for (const cmd of registry.getAll()) {
    if (!defaultKeybindings.has(cmd.id)) {
      defaultKeybindings.set(cmd.id, cmd.keybinding);
    }
  }
  const overrides = loadKeybindings();
  if (Object.keys(overrides).length === 0) return;
  for (const cmd of registry.getAll()) {
    const override = overrides[cmd.id];
    if (override !== undefined) {
      registry.register({ ...cmd, keybinding: override });
    }
  }
}
