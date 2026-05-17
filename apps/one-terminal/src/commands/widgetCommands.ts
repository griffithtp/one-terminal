import { invoke } from "@tauri-apps/api/core";
import { registry } from "./registry";
import type { LayoutSnapshot } from "../types";

// ── Active panel tracking ─────────────────────────────────────────────────────
//
// Call setActivePanelLabel from App.tsx whenever the focused panel changes
// (tab click, new panel opened). Generic widget commands use this label.

let activePanelLabel: string | null = null;

export function setActivePanelLabel(label: string | null): void {
  activePanelLabel = label;
}

async function zoomActive(delta: number): Promise<void> {
  const label = activePanelLabel;
  if (!label) return;
  const snap = await invoke<LayoutSnapshot | null>("wm_snapshot");
  const panel = snap?.panels.find((p) => p.id === label);
  if (!panel) return;
  const zoomFactor = Math.min(2.0, Math.max(0.5, panel.zoomFactor + delta));
  invoke("wm_set_zoom", { label, zoomFactor }).catch(console.error);
}

// ── Registration ──────────────────────────────────────────────────────────────

export function registerWidgetCommands(
  onOpenPalette: () => void,
  onOpenSettings: () => void,
): void {
  registry.register({
    id: "palette.open",
    label: "Open command palette",
    keywords: ["palette", "command", "search", "find"],
    shortcut: "⌘K",
    keybinding: "CmdOrCtrl+K",
    group: "navigation",
    action: onOpenPalette,
  });

  registry.register({
    id: "widget.close",
    label: "Close current widget",
    keywords: ["close", "widget", "panel", "tab", "quit"],
    shortcut: "⌘W",
    keybinding: "CmdOrCtrl+W",
    group: "widgets",
    action: () => {
      const label = activePanelLabel;
      if (label) invoke("wm_close_leaf", { label }).catch(console.error);
    },
    isAvailable: () => activePanelLabel !== null,
  });

  registry.register({
    id: "widget.rename",
    label: "Rename current widget",
    keywords: ["rename", "widget", "panel", "tab", "title", "name"],
    group: "widgets",
    action: () => {
      const label = activePanelLabel;
      if (label) invoke("wm_request_rename", { label }).catch(console.error);
    },
    isAvailable: () => activePanelLabel !== null,
  });

  registry.register({
    id: "widget.zoom-in",
    label: "Zoom in",
    keywords: ["zoom", "in", "bigger", "larger", "increase"],
    shortcut: "⌘+",
    keybinding: "CmdOrCtrl+Equal",
    group: "widgets",
    action: () => zoomActive(0.1),
    isAvailable: () => activePanelLabel !== null,
  });

  registry.register({
    id: "widget.zoom-out",
    label: "Zoom out",
    keywords: ["zoom", "out", "smaller", "decrease"],
    shortcut: "⌘−",
    keybinding: "CmdOrCtrl+Minus",
    group: "widgets",
    action: () => zoomActive(-0.1),
    isAvailable: () => activePanelLabel !== null,
  });

  registry.register({
    id: "widget.zoom-reset",
    label: "Reset zoom",
    keywords: ["zoom", "reset", "100", "default", "normal"],
    shortcut: "⌘0",
    keybinding: "CmdOrCtrl+0",
    group: "widgets",
    action: () => {
      const label = activePanelLabel;
      if (label) invoke("wm_set_zoom", { label, zoomFactor: 1.0 }).catch(console.error);
    },
    isAvailable: () => activePanelLabel !== null,
  });

  registry.register({
    id: "widget.split-h",
    label: "Split horizontal",
    keywords: ["split", "horizontal", "side", "left", "right", "column"],
    group: "widgets",
    // TODO: split requires a dedicated Tauri command (not yet implemented)
    action: () => console.log("TODO: widget.split-h"),
  });

  registry.register({
    id: "widget.split-v",
    label: "Split vertical",
    keywords: ["split", "vertical", "top", "bottom", "row"],
    group: "widgets",
    // TODO: split requires a dedicated Tauri command (not yet implemented)
    action: () => console.log("TODO: widget.split-v"),
  });

  registry.register({
    id: "widget.maximise",
    label: "Maximise / restore",
    keywords: ["maximise", "maximize", "restore", "fullscreen", "expand"],
    group: "widgets",
    // TODO: needs stack path for the active panel (requires active stack tracking)
    action: () => console.log("TODO: widget.maximise — needs active panel stack path"),
  });

  registry.register({
    id: "widget.reload",
    label: "Reload widget",
    keywords: ["reload", "refresh", "restart"],
    group: "widgets",
    // TODO: wm_reload_panel is not yet implemented in Rust
    action: () => console.log("TODO: widget.reload — wm_reload_panel not implemented"),
  });

  registry.register({
    id: "nav.focus-next",
    label: "Focus next widget",
    keywords: ["focus", "next", "widget", "panel", "tab", "switch"],
    shortcut: "⌃Tab",
    keybinding: "Ctrl+Tab",
    group: "navigation",
    // TODO: needs focus cycling logic
    action: () => console.log("TODO: nav.focus-next"),
  });

  registry.register({
    id: "nav.focus-prev",
    label: "Focus previous widget",
    keywords: ["focus", "previous", "prev", "widget", "panel", "tab", "back"],
    shortcut: "⌃⇧Tab",
    keybinding: "Ctrl+Shift+Tab",
    group: "navigation",
    // TODO: needs focus cycling logic
    action: () => console.log("TODO: nav.focus-prev"),
  });

  registry.register({
    id: "nav.app-directory",
    label: "Open App Directory",
    keywords: ["app", "directory", "appd", "open", "launch", "store"],
    group: "navigation",
    // TODO: wire to Header's open-panel flow
    action: () => console.log("TODO: nav.app-directory"),
  });

  registry.register({
    id: "settings.keybindings",
    label: "Edit keyboard shortcuts",
    keywords: ["settings", "keybindings", "shortcuts", "keyboard", "hotkeys", "remap"],
    group: "settings",
    action: onOpenSettings,
  });
}
