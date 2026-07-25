import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { registry } from "./registry";
import type { HostLayout, LayoutSnapshot, PanelBounds } from "../types";

// ── Live panel state (updated from wm:layout + wm:host-layout) ───────────────

const panelState = new Map<string, PanelBounds>();
const labelToStackPath = new Map<string, number[]>();

function syncFromSnapshot(snapshot: LayoutSnapshot | null): void {
  panelState.clear();
  for (const p of snapshot?.panels ?? []) panelState.set(p.id, p);
  rebuildInstanceCommands();
}

function syncFromHostLayout(host: HostLayout): void {
  labelToStackPath.clear();
  for (const stack of host.stacks) {
    for (const tab of stack.tabs) {
      labelToStackPath.set(tab.label, stack.path);
    }
  }
  // Re-register so maximise actions have fresh paths.
  rebuildInstanceCommands();
}

// ── Instance command builders ─────────────────────────────────────────────────

function displayTitle(p: PanelBounds): string {
  return p.displayName ?? p.title ?? p.id;
}

function rebuildInstanceCommands(): void {
  // Remove stale instance commands before re-registering.
  for (const cmd of registry.getAll()) {
    if (cmd.group === "widget-instances") registry.unregister(cmd.id);
  }

  for (const p of panelState.values()) {
    const name = displayTitle(p);
    const nameKw = name.toLowerCase();
    const appKw = p.appId.toLowerCase();

    registry.register({
      id: `widget-instance.close-${p.id}`,
      label: `Close "${name}"`,
      keywords: ["close", nameKw, appKw],
      group: "widget-instances",
      widgetLabel: p.id,
      action: () => void invoke("wm_close_leaf", { label: p.id }).catch(console.error),
    });

    registry.register({
      id: `widget-instance.rename-${p.id}`,
      label: `Rename "${name}"`,
      keywords: ["rename", "title", nameKw, appKw],
      group: "widget-instances",
      widgetLabel: p.id,
      action: () => void invoke("wm_request_rename", { label: p.id }).catch(console.error),
    });

    registry.register({
      id: `widget-instance.zoom-in-${p.id}`,
      label: `Zoom in "${name}"`,
      keywords: ["zoom", "in", "bigger", nameKw, appKw],
      group: "widget-instances",
      widgetLabel: p.id,
      action: () => {
        const current = panelState.get(p.id)?.zoomFactor ?? 1.0;
        const zoomFactor = Math.min(2.0, current + 0.1);
        invoke("wm_set_zoom", { label: p.id, zoomFactor }).catch(console.error);
      },
    });

    registry.register({
      id: `widget-instance.zoom-out-${p.id}`,
      label: `Zoom out "${name}"`,
      keywords: ["zoom", "out", "smaller", nameKw, appKw],
      group: "widget-instances",
      widgetLabel: p.id,
      action: () => {
        const current = panelState.get(p.id)?.zoomFactor ?? 1.0;
        const zoomFactor = Math.max(0.5, current - 0.1);
        invoke("wm_set_zoom", { label: p.id, zoomFactor }).catch(console.error);
      },
    });

    registry.register({
      id: `widget-instance.zoom-reset-${p.id}`,
      label: `Reset zoom "${name}"`,
      keywords: ["zoom", "reset", "100", nameKw, appKw],
      group: "widget-instances",
      widgetLabel: p.id,
      action: () =>
        void invoke("wm_set_zoom", { label: p.id, zoomFactor: 1.0 }).catch(console.error),
    });

    registry.register({
      id: `widget-instance.maximise-${p.id}`,
      label: `Maximise "${name}"`,
      keywords: ["maximise", "maximize", "restore", "fullscreen", nameKw, appKw],
      group: "widget-instances",
      widgetLabel: p.id,
      action: () => {
        const path = labelToStackPath.get(p.id);
        if (path) invoke("wm_toggle_maximize_stack", { path }).catch(console.error);
      },
      isAvailable: () => labelToStackPath.has(p.id),
    });

    registry.register({
      id: `widget-instance.keep-alive-${p.id}`,
      label: `${p.keepAlive ? "Stop" : "Keep"} "${name}" running in background`,
      keywords: ["keep", "alive", "background", "dashboard", nameKw, appKw],
      group: "widget-instances",
      widgetLabel: p.id,
      action: () => {
        const current = panelState.get(p.id)?.keepAlive ?? false;
        invoke("wm_set_panel_keep_alive", { label: p.id, keepAlive: !current }).catch(
          console.error
        );
      },
    });

    registry.register({
      id: `widget-instance.reload-${p.id}`,
      label: `Reload "${name}"`,
      keywords: ["reload", "refresh", nameKw, appKw],
      group: "widget-instances",
      widgetLabel: p.id,
      // TODO: wm_reload_panel is not yet implemented in Rust
      action: () => console.log(`TODO: reload panel ${p.id}`),
    });
  }
}

// ── Initialisation ────────────────────────────────────────────────────────────

export async function initWidgetInstanceCommands(): Promise<void> {
  // Seed from current snapshot so commands are available immediately.
  const snap = await invoke<LayoutSnapshot | null>("wm_snapshot");
  syncFromSnapshot(snap);

  await listen<LayoutSnapshot | null>("wm:layout", (e) => syncFromSnapshot(e.payload));
  await listen<HostLayout>("wm:host-layout", (e) => syncFromHostLayout(e.payload));
}
