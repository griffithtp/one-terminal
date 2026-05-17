import { listen } from "@tauri-apps/api/event";
import { registry } from "./registry";

// ── Key combo normalisation ───────────────────────────────────────────────────

function normalise(e: KeyboardEvent): string {
  const mods: string[] = [];
  // metaKey = Cmd on macOS, ctrlKey = Ctrl on Windows/Linux — both map to CmdOrCtrl.
  if (e.metaKey || e.ctrlKey) mods.push("CmdOrCtrl");
  if (e.shiftKey) mods.push("Shift");
  if (e.altKey) mods.push("Alt");

  let key = e.key;
  // Single printable characters come in lowercase when no Shift is held;
  // upper-case them so "Cmd+k" matches the registered "CmdOrCtrl+K".
  if (key.length === 1) key = key.toUpperCase();
  // Map physical key names to the normalised identifiers used in keybindings.
  if (key === "=") key = "Equal";
  if (key === "-") key = "Minus";

  return [...mods, key].join("+");
}

// ── In-process listener (fires when the chrome webview has keyboard focus) ────

export function initKeyboardListener(): void {
  document.addEventListener("keydown", (e) => {
    const combo = normalise(e);
    const cmd = registry.findByKeybinding(combo);
    if (!cmd || cmd.isAvailable?.() === false) return;
    e.preventDefault();
    registry.execute(cmd.id).catch(console.error);
  });
}

// ── Global shortcut listener (fires from the Rust-side OS registration) ───────
//
// Rust registers CmdOrCtrl+K via tauri-plugin-global-shortcut and emits
// "global-shortcut" when it fires. This reaches the chrome webview even when
// a panel webview currently holds OS keyboard focus.

export async function initGlobalShortcutListener(): Promise<void> {
  await listen("global-shortcut", () => {
    registry.execute("palette.open").catch(console.error);
  });
}
