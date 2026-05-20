import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { registry } from "../commands/registry";
import type { TerminalListItem } from "../types";

// ── Error discriminator ───────────────────────────────────────────────────────

type TerminalError =
  | { code: "needsConfirm" }
  | { code: "lastTerminal" }
  | { code: "other"; message: string };

function isNeedsConfirm(e: unknown): boolean {
  return (
    typeof e === "object" &&
    e !== null &&
    (e as TerminalError).code === "needsConfirm"
  );
}

// ── Public types ──────────────────────────────────────────────────────────────

export interface UseTerminalsResult {
  terminals: TerminalListItem[];
  /** The Terminal whose chrome webview is rendering this component. */
  current: TerminalListItem | null;
  openNew: (name?: string) => Promise<void>;
  focus: (id: string) => Promise<void>;
  /** Initiates close. Sets `pendingClose` if the Terminal has unsaved dashboards. */
  close: (id: string) => Promise<void>;
  rename: (id: string, name: string) => Promise<void>;
  resetPosition: (id?: string) => Promise<void>;
  /** Non-null when a close was blocked by NeedsConfirm — render a dialog */
  pendingClose: string | null;
  confirmSave: () => Promise<void>;
  confirmDiscard: () => Promise<void>;
  cancelClose: () => void;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useTerminals(): UseTerminalsResult {
  const [terminals, setTerminals] = useState<TerminalListItem[]>([]);
  const [currentId, setCurrentId] = useState<string>("");
  const [pendingClose, setPendingClose] = useState<string | null>(null);

  // The parent window label is the Terminal ID (e.g. "terminal-main").
  useEffect(() => {
    setCurrentId(getCurrentWindow().label);
  }, []);

  // ── Initial fetch + live subscription ──────────────────────────────────────
  useEffect(() => {
    invoke<TerminalListItem[]>("wm_list_terminals").then(setTerminals).catch(console.error);

    const unlisten = listen<TerminalListItem[]>("wm:terminals", (e) => {
      setTerminals(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // ── Actions ───────────────────────────────────────────────────────────────

  const openNew = useCallback(async (name?: string) => {
    await invoke("wm_new_terminal", { name });
  }, []);

  const focus = useCallback(async (id: string) => {
    await invoke("wm_focus_terminal", { id });
  }, []);

  const close = useCallback(async (id: string) => {
    try {
      await invoke("wm_close_terminal", { id });
    } catch (e) {
      if (isNeedsConfirm(e)) {
        setPendingClose(id);
      } else {
        console.error("[terminals] close:", e);
      }
    }
  }, []);

  const rename = useCallback(async (id: string, name: string) => {
    await invoke("wm_rename_terminal", { id, name });
  }, []);

  const resetPosition = useCallback(async (id?: string) => {
    await invoke("wm_reset_terminal_position", id !== undefined ? { id } : {});
  }, []);

  // After NeedsConfirm: save/discard the active dashboard (which makes the
  // Terminal clean), then retry close. This works when closing the current
  // Terminal; for cross-terminal close the save/discard targets the terminal
  // whose chrome is rendering this hook, which is the common case.
  const confirmSave = useCallback(async () => {
    const target = pendingClose;
    if (!target) return;
    setPendingClose(null);
    try {
      await invoke("wm_save_dashboard");
      await invoke("wm_close_terminal", { id: target });
    } catch (e) {
      console.error("[terminals] confirmSave:", e);
    }
  }, [pendingClose]);

  const confirmDiscard = useCallback(async () => {
    const target = pendingClose;
    if (!target) return;
    setPendingClose(null);
    try {
      await invoke("wm_discard_dashboard");
      await invoke("wm_close_terminal", { id: target });
    } catch (e) {
      console.error("[terminals] confirmDiscard:", e);
    }
  }, [pendingClose]);

  const cancelClose = useCallback(() => setPendingClose(null), []);

  // ── Stable refs for palette command actions ───────────────────────────────

  const resetPositionRef = useRef(resetPosition);
  const currentIdRef = useRef(currentId);
  resetPositionRef.current = resetPosition;
  currentIdRef.current = currentId;

  // ── Command palette — static commands ─────────────────────────────────────

  useEffect(() => {
    registry.register({
      id: "terminal:reset-position",
      label: "Reset terminal window position",
      keywords: ["reset", "terminal", "window", "position", "move", "center", "recentre"],
      group: "navigation",
      action: () => resetPositionRef.current().catch(console.error),
    });
    return () => {
      registry.unregister("terminal:reset-position");
    };
  }, []);

  // ── Command palette — dynamic switch commands ─────────────────────────────

  const focusRef = useRef(focus);
  focusRef.current = focus;

  useEffect(() => {
    if (!terminals.length) return;
    const ids: string[] = [];
    for (const t of terminals) {
      if (t.id === currentIdRef.current) continue;
      const id = `terminal:switch:${t.id}`;
      ids.push(id);
      registry.register({
        id,
        label: `Switch to "${t.name}"`,
        keywords: ["switch", "terminal", "focus", t.name.toLowerCase()],
        group: "navigation",
        action: () => focusRef.current(t.id).catch(console.error),
      });
    }
    return () => {
      ids.forEach((id) => registry.unregister(id));
    };
  }, [terminals]);

  // ── Derived state ─────────────────────────────────────────────────────────

  const current = terminals.find((t) => t.id === currentId) ?? null;

  return {
    terminals,
    current,
    openNew,
    focus,
    close,
    rename,
    resetPosition,
    pendingClose,
    confirmSave,
    confirmDiscard,
    cancelClose,
  };
}
