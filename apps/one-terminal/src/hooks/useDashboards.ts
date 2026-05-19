import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { registry } from "../commands/registry";

// ── Tauri payload type ────────────────────────────────────────────────────────

interface DashboardsPayload {
  active: string;
  autoSave: boolean;
  dirty: boolean;
  dashboards: string[];
}

type DashboardError =
  | { code: "needsConfirm" }
  | { code: "notFound" }
  | { code: "other"; message: string };

function isNeedsConfirm(e: unknown): boolean {
  return typeof e === "object" && e !== null && (e as DashboardError).code === "needsConfirm";
}

// ── Public types ──────────────────────────────────────────────────────────────

export interface DashboardInfo {
  name: string;
  active: boolean;
  /** true only on the active dashboard when auto-save is off and layout has changed */
  dirty: boolean;
}

export interface UseDashboardsResult {
  dashboards: DashboardInfo[];
  autoSave: boolean;
  /** Non-null when a switch was blocked by NeedsConfirm — render a dialog */
  pendingSwitch: string | null;
  switchTo: (name: string) => Promise<void>;
  /** Save the live layout then complete the pending switch */
  confirmSave: () => Promise<void>;
  /** Discard live changes then complete the pending switch */
  confirmDiscard: () => Promise<void>;
  cancelSwitch: () => void;
  create: (name: string) => Promise<void>;
  save: () => Promise<void>;
  discard: () => Promise<void>;
  rename: (oldName: string, newName: string) => Promise<void>;
  remove: (name: string) => Promise<void>;
  reorder: (names: string[]) => Promise<void>;
  setAutoSave: (enabled: boolean) => Promise<void>;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useDashboards(): UseDashboardsResult {
  const [payload, setPayload] = useState<DashboardsPayload | null>(null);
  const [pendingSwitch, setPendingSwitch] = useState<string | null>(null);

  // ── Initial fetch + live subscription ────────────────────────────────────
  useEffect(() => {
    invoke<DashboardsPayload>("wm_list_dashboards").then(setPayload).catch(console.error);

    const unlisten = listen<DashboardsPayload>("wm:dashboards", (e) => {
      setPayload(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // ── Window title ──────────────────────────────────────────────────────────
  useEffect(() => {
    if (!payload) return;
    const suffix = payload.dirty && !payload.autoSave ? " *" : "";
    getCurrentWindow().setTitle(`OneTerminal — ${payload.active}${suffix}`).catch(console.error);
  }, [payload]);

  // ── Actions ───────────────────────────────────────────────────────────────
  const switchTo = useCallback(async (name: string) => {
    try {
      await invoke("wm_switch_dashboard", { name });
    } catch (e) {
      if (isNeedsConfirm(e)) {
        setPendingSwitch(name);
      } else {
        console.error("[dashboards] switch:", e);
      }
    }
  }, []);

  const confirmSave = useCallback(async () => {
    const target = pendingSwitch;
    if (!target) return;
    setPendingSwitch(null);
    try {
      await invoke("wm_save_dashboard");
      await invoke("wm_switch_dashboard", { name: target });
    } catch (e) {
      console.error("[dashboards] confirmSave:", e);
    }
  }, [pendingSwitch]);

  const confirmDiscard = useCallback(async () => {
    const target = pendingSwitch;
    if (!target) return;
    setPendingSwitch(null);
    try {
      await invoke("wm_discard_dashboard");
      await invoke("wm_switch_dashboard", { name: target });
    } catch (e) {
      console.error("[dashboards] confirmDiscard:", e);
    }
  }, [pendingSwitch]);

  const cancelSwitch = useCallback(() => setPendingSwitch(null), []);

  const create = useCallback(async (name: string) => {
    await invoke("wm_create_dashboard", { name });
  }, []);

  const save = useCallback(async () => {
    await invoke("wm_save_dashboard");
  }, []);

  const discard = useCallback(async () => {
    await invoke("wm_discard_dashboard");
  }, []);

  const rename = useCallback(async (oldName: string, newName: string) => {
    await invoke("wm_rename_dashboard", { oldName, newName });
  }, []);

  const remove = useCallback(async (name: string) => {
    await invoke("wm_delete_dashboard", { name });
  }, []);

  const reorder = useCallback(async (names: string[]) => {
    await invoke("wm_reorder_dashboards", { order: names });
  }, []);

  const setAutoSave = useCallback(async (enabled: boolean) => {
    await invoke("wm_set_auto_save", { enabled });
  }, []);

  // ── Command palette — stable static commands ──────────────────────────────
  const saveRef = useRef(save);
  const discardRef = useRef(discard);
  const setAutoSaveRef = useRef(setAutoSave);
  const payloadRef = useRef(payload);

  saveRef.current = save;
  discardRef.current = discard;
  setAutoSaveRef.current = setAutoSave;
  payloadRef.current = payload;

  useEffect(() => {
    registry.register({
      id: "dashboard:save",
      label: "Save dashboard",
      keywords: ["save", "dashboard", "snapshot"],
      group: "navigation",
      action: () => saveRef.current().catch(console.error),
    });
    registry.register({
      id: "dashboard:discard",
      label: "Discard dashboard changes",
      keywords: ["discard", "revert", "dashboard", "changes"],
      group: "navigation",
      isAvailable: () =>
        payloadRef.current?.dirty === true && payloadRef.current?.autoSave === false,
      action: () => discardRef.current().catch(console.error),
    });
    registry.register({
      id: "dashboard:toggle-autosave",
      label: "Toggle dashboard auto-save",
      keywords: ["auto", "save", "dashboard", "toggle", "autosave"],
      group: "settings",
      action: () => setAutoSaveRef.current(!payloadRef.current?.autoSave).catch(console.error),
    });
    return () => {
      registry.unregister("dashboard:save");
      registry.unregister("dashboard:discard");
      registry.unregister("dashboard:toggle-autosave");
    };
  }, []);

  // ── Command palette — dynamic switch commands per dashboard ───────────────
  const switchToRef = useRef(switchTo);
  switchToRef.current = switchTo;

  useEffect(() => {
    if (!payload) return;

    const ids: string[] = [];
    for (const name of payload.dashboards) {
      if (name === payload.active) continue;
      const id = `dashboard:switch:${name}`;
      ids.push(id);
      registry.register({
        id,
        label: `Switch to "${name}"`,
        keywords: ["switch", "dashboard", name.toLowerCase()],
        group: "navigation",
        action: () => switchToRef.current(name).catch(console.error),
      });
    }
    return () => {
      ids.forEach((id) => registry.unregister(id));
    };
  }, [payload]);

  // ── Derived state ─────────────────────────────────────────────────────────
  const dashboards: DashboardInfo[] = payload
    ? payload.dashboards.map((name) => ({
        name,
        active: name === payload.active,
        dirty: name === payload.active && payload.dirty,
      }))
    : [];

  return {
    dashboards,
    autoSave: payload?.autoSave ?? true,
    pendingSwitch,
    switchTo,
    confirmSave,
    confirmDiscard,
    cancelSwitch,
    create,
    save,
    discard,
    rename,
    remove,
    reorder,
    setAutoSave,
  };
}
