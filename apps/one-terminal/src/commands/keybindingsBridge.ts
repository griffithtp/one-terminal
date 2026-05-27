/**
 * Keybindings IPC bridge (chrome side)
 *
 * The command registry is a per-webview singleton. The keybindings editor
 * lives in the overlay webview's drawer (so it sits above panels in
 * z-order) but the registry lives in chrome. This module is the chrome-
 * side bridge:
 *
 *   - `publishKeybindingsSnapshot()` serialises the registry + defaults and
 *     emits `wm:keybindings-snapshot`. The overlay receives this and feeds
 *     it to `KeybindingsEditor` via props.
 *
 *   - `initKeybindingsBridge()` listens for the overlay's edit events
 *     (`wm:keybinding-set`, `wm:keybinding-reset`, `wm:keybinding-reset-all`)
 *     and applies them to the registry + persistent store, then re-publishes
 *     the fresh snapshot so the editor re-renders.
 *
 * Call `initKeybindingsBridge()` once in `main.tsx` after static commands
 * are registered and `applyKeybindingOverrides()` has populated defaults.
 */

import { emit, listen } from "@tauri-apps/api/event";
import { registry, type Command } from "./registry";
import { getDefaultKeybinding, loadKeybindings, saveKeybindings } from "./keybindingStore";

export interface KeybindingsSnapshotPayload {
  commands: Array<{
    id: string;
    label: string;
    keywords: string[];
    group: string;
    keybinding?: string;
  }>;
  /** id → default keybinding (or null if the command has none). */
  defaults: Record<string, string | null>;
}

const SNAPSHOT_EVENT = "wm:keybindings-snapshot";
const REQUEST_EVENT = "wm:keybindings-request";
const SET_EVENT = "wm:keybinding-set";
const RESET_EVENT = "wm:keybinding-reset";
const RESET_ALL_EVENT = "wm:keybinding-reset-all";

function buildSnapshot(): KeybindingsSnapshotPayload {
  const commands = registry.getAll().map((cmd) => ({
    id: cmd.id,
    label: cmd.label,
    keywords: cmd.keywords,
    group: cmd.group,
    keybinding: cmd.keybinding,
  }));
  const defaults: Record<string, string | null> = {};
  for (const cmd of registry.getAll()) {
    defaults[cmd.id] = getDefaultKeybinding(cmd.id) ?? null;
  }
  return { commands, defaults };
}

export function publishKeybindingsSnapshot(): void {
  emit(SNAPSHOT_EVENT, buildSnapshot()).catch(console.error);
}

function findByKeybinding(combo: string): Command | undefined {
  return registry.getAll().find((c) => c.keybinding === combo);
}

function applySet(id: string, combo: string): void {
  // If another command already uses this combo, unassign it first so we
  // don't leave a duplicate registration (mirrors the original chrome
  // editor's behaviour).
  const conflicting = findByKeybinding(combo);
  if (conflicting && conflicting.id !== id) {
    registry.register({ ...conflicting, keybinding: undefined });
    const overrides = loadKeybindings();
    if (getDefaultKeybinding(conflicting.id) === undefined) {
      delete overrides[conflicting.id];
    } else {
      // Mark as explicitly unbound so applyKeybindingOverrides doesn't
      // restore the default at next startup.
      overrides[conflicting.id] = "";
    }
    saveKeybindings(overrides);
  }

  const target = registry.getAll().find((c) => c.id === id);
  if (target) registry.register({ ...target, keybinding: combo });

  const overrides = loadKeybindings();
  overrides[id] = combo;
  saveKeybindings(overrides);
}

function applyReset(id: string): void {
  const defaultKb = getDefaultKeybinding(id);
  const target = registry.getAll().find((c) => c.id === id);
  if (target) registry.register({ ...target, keybinding: defaultKb });

  const overrides = loadKeybindings();
  delete overrides[id];
  saveKeybindings(overrides);
}

function applyResetAll(): void {
  for (const cmd of registry.getAll()) {
    const defaultKb = getDefaultKeybinding(cmd.id);
    registry.register({ ...cmd, keybinding: defaultKb });
  }
  saveKeybindings({});
}

export function initKeybindingsBridge(): void {
  // Respond to snapshot requests (overlay's KeybindingsSection emits one
  // on mount so it doesn't miss a snapshot published before its listener
  // was attached).
  listen(REQUEST_EVENT, () => {
    publishKeybindingsSnapshot();
  }).catch(console.error);

  listen<{ id: string; combo: string }>(SET_EVENT, (e) => {
    applySet(e.payload.id, e.payload.combo);
    publishKeybindingsSnapshot();
  }).catch(console.error);

  listen<{ id: string }>(RESET_EVENT, (e) => {
    applyReset(e.payload.id);
    publishKeybindingsSnapshot();
  }).catch(console.error);

  listen(RESET_ALL_EVENT, () => {
    applyResetAll();
    publishKeybindingsSnapshot();
  }).catch(console.error);
}

export const KEYBINDINGS_SNAPSHOT_EVENT = SNAPSHOT_EVENT;
export const KEYBINDINGS_REQUEST_EVENT = REQUEST_EVENT;
export const KEYBINDING_SET_EVENT = SET_EVENT;
export const KEYBINDING_RESET_EVENT = RESET_EVENT;
export const KEYBINDING_RESET_ALL_EVENT = RESET_ALL_EVENT;
