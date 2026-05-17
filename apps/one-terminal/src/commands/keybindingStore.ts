import { registry } from "./registry";

const STORAGE_KEY = "one-terminal:keybindings";

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
 *  the default keybinding. Call this after all commands are registered. */
export function applyKeybindingOverrides(): void {
  const overrides = loadKeybindings();
  if (Object.keys(overrides).length === 0) return;
  for (const cmd of registry.getAll()) {
    const override = overrides[cmd.id];
    if (override !== undefined) {
      registry.register({ ...cmd, keybinding: override });
    }
  }
}
