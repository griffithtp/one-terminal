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
  /** Names of dashboards closed via `close()` — hidden but not deleted. */
  closedDashboards: string[];
  /** Total panels across all dashboards parked off-screen (kept alive)
   *  instead of closed, because their owning dashboard isn't active. */
  parkedCount: number;
  /**
   * Dashboard name → display name of the *other* Terminal window currently
   * holding it active (Issue 15-D's exclusivity lock). Never has an entry
   * for this window's own active dashboard — only dashboards locked
   * elsewhere get a badge.
   */
  lockedBy: Record<string, string>;
}

type DashboardError =
  | { code: "needsConfirm" }
  | { code: "notFound" }
  | { code: "lockedElsewhere"; terminalName: string }
  | { code: "other"; message: string };

function isNeedsConfirm(e: unknown): boolean {
  return typeof e === "object" && e !== null && (e as DashboardError).code === "needsConfirm";
}

function asLockedElsewhere(e: unknown): string | null {
  if (typeof e !== "object" || e === null) return null;
  const err = e as DashboardError;
  return err.code === "lockedElsewhere" ? err.terminalName : null;
}

// ── Public types ──────────────────────────────────────────────────────────────

/**
 * Surfaced when a switch/close/delete was blocked because the dashboard is
 * active in another Terminal window (Issue 15-D). `duplicate`/`moveHere`
 * (Issue 15-G) are the two ways forward — reachable from wherever this is
 * rendered, not just an explanation with no next step.
 */
export interface LockConflict {
  name: string;
  terminalName: string;
  message: string;
}

export interface DashboardInfo {
  name: string;
  active: boolean;
  /** true only on the active dashboard when auto-save is off and layout has changed */
  dirty: boolean;
  /**
   * Display name of the *other* Terminal window currently holding this
   * dashboard active (Issue 15-D), or `null` if it isn't locked elsewhere.
   * Never set for this window's own active dashboard.
   */
  lockedBy: string | null;
}

export interface UseDashboardsResult {
  dashboards: DashboardInfo[];
  /** Names of dashboards closed (hidden, not deleted) — see `close`/`reopen`. */
  closedDashboards: string[];
  autoSave: boolean;
  /** Total panels currently parked (kept alive) in background dashboards. */
  parkedCount: number;
  /**
   * Try to switch to `name`. On NeedsConfirm (auto-save off + dirty active
   * layout) the hook invokes `wm_dashboard_confirm_open` so the overlay
   * shows the Save / Discard / Cancel dialog. The dialog completes the
   * switch itself; no chrome-side confirm state is needed. On
   * LockedElsewhere (Issue 15-D — `name` is active in another window),
   * surfaces `lockConflict` instead of switching.
   */
  switchTo: (name: string) => Promise<void>;
  create: (name: string) => Promise<void>;
  save: () => Promise<void>;
  discard: () => Promise<void>;
  rename: (oldName: string, newName: string) => Promise<void>;
  /** Permanently delete `name` — irreversible. See `close` for the reopenable alternative. */
  remove: (name: string) => Promise<void>;
  /** Hide `name` from the switcher/drawer and stop its background widgets, without deleting it. */
  close: (name: string) => Promise<void>;
  /** Bring a closed dashboard back into the switcher/drawer, unchanged. Does not switch to it. */
  reopen: (name: string) => Promise<void>;
  reorder: (names: string[]) => Promise<void>;
  setAutoSave: (enabled: boolean) => Promise<void>;
  /**
   * Set when a switch/close/delete was blocked because the target is
   * active in another window (Issue 15-D). `null` when there's nothing to
   * show. Pair with `clearLockConflict`, `duplicateHere`, `moveHere`.
   */
  lockConflict: LockConflict | null;
  clearLockConflict: () => void;
  /** Create an independent copy of `name` in this window and switch to it — sidesteps the lock entirely (Issue 15-G). */
  duplicateHere: (name: string) => Promise<void>;
  /**
   * Take `name` away from the window that has it active and make it active
   * here instead (Issue 15-G). If the owning window has unsaved edits, this
   * confirms with the user (their edits are saved, never discarded) before
   * completing the move.
   */
  moveHere: (name: string) => Promise<void>;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useDashboards(): UseDashboardsResult {
  const [payload, setPayload] = useState<DashboardsPayload | null>(null);
  const [lockConflict, setLockConflict] = useState<LockConflict | null>(null);
  const clearLockConflict = useCallback(() => setLockConflict(null), []);

  const reportLockConflict = useCallback((name: string, terminalName: string, message: string) => {
    setLockConflict({ name, terminalName, message });
  }, []);

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
    if (!payload?.active) return;
    const suffix = payload.dirty && !payload.autoSave ? " *" : "";
    getCurrentWindow().setTitle(`OneTerminal — ${payload.active}${suffix}`).catch(console.error);
  }, [payload]);

  // ── Actions ───────────────────────────────────────────────────────────────
  // Keep a ref to the latest active name so switchTo can pass it to the
  // overlay's confirm dialog without rebinding the callback on every update.
  const payloadRef = useRef(payload);
  payloadRef.current = payload;

  const switchTo = useCallback(async (name: string) => {
    try {
      await invoke("wm_switch_dashboard", { name });
    } catch (e) {
      const owner = asLockedElsewhere(e);
      if (owner) {
        reportLockConflict(name, owner, `"${name}" is open in ${owner}`);
      } else if (isNeedsConfirm(e)) {
        const activeName = payloadRef.current?.active ?? "";
        invoke("wm_dashboard_confirm_open", {
          activeName,
          pendingName: name,
        }).catch(console.error);
      } else {
        console.error("[dashboards] switch:", e);
      }
    }
  }, []);

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
    try {
      await invoke("wm_delete_dashboard", { name, force: false });
    } catch (e) {
      const owner = asLockedElsewhere(e);
      if (owner) {
        reportLockConflict(
          name,
          owner,
          `"${name}" is open in ${owner} — switch away from it there first`
        );
      } else if (isNeedsConfirm(e)) {
        invoke("wm_dashboard_confirm_delete_open", { name }).catch(console.error);
      } else {
        console.error("[dashboards] remove:", e);
      }
    }
  }, []);

  const close = useCallback(async (name: string) => {
    try {
      await invoke("wm_close_dashboard", { name, force: false });
    } catch (e) {
      const owner = asLockedElsewhere(e);
      if (owner) {
        reportLockConflict(
          name,
          owner,
          `"${name}" is open in ${owner} — switch away from it there first`
        );
      } else if (isNeedsConfirm(e)) {
        invoke("wm_dashboard_confirm_close_open", { name }).catch(console.error);
      } else {
        console.error("[dashboards] close:", e);
      }
    }
  }, []);

  // ── Duplicate / Move here (Issue 15-G) ────────────────────────────────────
  // Both are reachable from wherever `lockConflict` is rendered — a blocked
  // switch/close/delete isn't a dead end.

  const duplicateHere = useCallback(
    async (name: string) => {
      try {
        const newName = await invoke<string>("wm_duplicate_dashboard", { name });
        setLockConflict(null);
        await switchTo(newName);
      } catch (e) {
        console.error("[dashboards] duplicate:", e);
      }
    },
    [switchTo]
  );

  const performMove = useCallback(async (name: string, forceDiscard: boolean) => {
    try {
      await invoke("wm_move_dashboard", { name, forceDiscard });
      setLockConflict(null);
    } catch (e) {
      if (isNeedsConfirm(e) && !forceDiscard) {
        // The owning window has unsaved edits — confirm before proceeding.
        // Its edits are saved as part of the move, never discarded.
        const ok = window.confirm(
          `"${name}" has unsaved changes in another window. Moving it here will save ` +
            "those changes first, not lose them. Continue?"
        );
        if (ok) {
          await performMoveRef.current(name, true);
        }
        return;
      }
      const owner = asLockedElsewhere(e);
      if (owner) {
        reportLockConflict(name, owner, `"${name}" is open in ${owner}`);
      } else {
        console.error("[dashboards] move:", e);
      }
    }
  }, []);
  const performMoveRef = useRef(performMove);
  performMoveRef.current = performMove;

  const moveHere = useCallback((name: string) => performMove(name, false), [performMove]);

  const reopen = useCallback(async (name: string) => {
    await invoke("wm_reopen_dashboard", { name });
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

  saveRef.current = save;
  discardRef.current = discard;
  setAutoSaveRef.current = setAutoSave;

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
        lockedBy: payload.lockedBy[name] ?? null,
      }))
    : [];

  return {
    dashboards,
    closedDashboards: payload?.closedDashboards ?? [],
    autoSave: payload?.autoSave ?? true,
    parkedCount: payload?.parkedCount ?? 0,
    switchTo,
    create,
    save,
    discard,
    rename,
    remove,
    close,
    reopen,
    reorder,
    setAutoSave,
    lockConflict,
    clearLockConflict,
    duplicateHere,
    moveHere,
  };
}
