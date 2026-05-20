/**
 * TerminalSwitcher
 *
 * Trigger button in the header. Clicking it emits "wm:terminal-switcher" to
 * the overlay webview (which sits above all panel webviews), where the actual
 * dropdown is rendered. Actions (close, new) are sent back as
 * "wm:terminal-switcher-action" events which this component handles by opening
 * the appropriate dialog in the chrome.
 *
 * Dialogs park panel webviews while open so they are not occluded by content
 * panels sitting above the chrome in z-order.
 */

import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTerminals } from "../hooks/useTerminals";
import { registry } from "../commands/registry";
import { NewTerminalDialog } from "./NewTerminalDialog";
import "./TerminalSwitcher.css";

// ── Action events emitted from the overlay back to the chrome ─────────────────

type TerminalSwitcherAction = { type: "new" } | { type: "close"; id: string };

// ── Confirm close dialog ──────────────────────────────────────────────────────

interface ConfirmCloseDialogProps {
  terminalName: string;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

function ConfirmCloseDialog({ terminalName, onSave, onDiscard, onCancel }: ConfirmCloseDialogProps) {
  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">Close Terminal</div>
        <p className="wm-ds-dialog__body">
          <strong>&ldquo;{terminalName}&rdquo;</strong> has unsaved dashboard changes. Save before
          closing or discard them?
        </p>
        <div className="wm-ds-dialog__actions">
          <button type="button" className="wm-ds-dialog__btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--danger"
            onClick={onDiscard}
          >
            Discard
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--primary"
            onClick={onSave}
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Rename dialog ─────────────────────────────────────────────────────────────

interface RenameDialogProps {
  currentName: string;
  onConfirm: (name: string) => void;
  onCancel: () => void;
}

function RenameDialog({ currentName, onConfirm, onCancel }: RenameDialogProps) {
  const [name, setName] = useState(currentName);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.select();
  }, []);

  const canSubmit = name.trim().length > 0 && name.trim() !== currentName;

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter" && canSubmit) onConfirm(name.trim());
      if (e.key === "Escape") onCancel();
    },
    [name, canSubmit, onConfirm, onCancel]
  );

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">Rename Terminal</div>
        <input
          ref={inputRef}
          className="wm-ds-dialog__input"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <div className="wm-ds-dialog__actions">
          <button type="button" className="wm-ds-dialog__btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--primary"
            disabled={!canSubmit}
            onClick={() => canSubmit && onConfirm(name.trim())}
          >
            Rename
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

export function TerminalSwitcher() {
  const ts = useTerminals();
  const [creating, setCreating] = useState(false);
  const [renaming, setRenaming] = useState(false);
  const btnRef = useRef<HTMLButtonElement>(null);

  const dialogOpen = creating || renaming || ts.pendingClose !== null;

  // Park panels while any dialog is open.
  const parkedRef = useRef(false);
  useEffect(() => {
    if (dialogOpen && !parkedRef.current) {
      parkedRef.current = true;
      invoke("wm_park_panels").catch(console.error);
    } else if (!dialogOpen && parkedRef.current) {
      parkedRef.current = false;
      invoke("wm_unpark_panels").catch(console.error);
    }
  }, [dialogOpen]);

  // Listen for action events sent back from the overlay dropdown.
  useEffect(() => {
    const unlisten = listen<TerminalSwitcherAction>("wm:terminal-switcher-action", (e) => {
      const action = e.payload;
      if (action.type === "new") {
        setCreating(true);
      } else if (action.type === "close") {
        tsRef.current.close(action.id).catch(console.error);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Open the dropdown in the overlay webview.
  const handleTriggerClick = useCallback(() => {
    if (!btnRef.current) return;
    const rect = btnRef.current.getBoundingClientRect();
    const overlayLabel = `${getCurrentWindow().label}-overlay`;
    emitTo(overlayLabel, "wm:terminal-switcher", {
      x: rect.left,
      y: rect.bottom + 4,
      terminals: ts.terminals,
      currentId: ts.current?.id ?? "",
    }).catch(console.error);
  }, [ts.terminals, ts.current]);

  const handleNew = useCallback(
    (name: string) => {
      setCreating(false);
      ts.openNew(name).catch(console.error);
    },
    [ts]
  );

  const handleRename = useCallback(
    (newName: string) => {
      setRenaming(false);
      if (!ts.current) return;
      ts.rename(ts.current.id, newName).catch(console.error);
    },
    [ts]
  );

  // ── Stable refs for palette command actions ───────────────────────────────

  const setCreatingRef = useRef(setCreating);
  const setRenamingRef = useRef(setRenaming);
  const tsRef = useRef(ts);
  setCreatingRef.current = setCreating;
  setRenamingRef.current = setRenaming;
  tsRef.current = ts;

  // ── Command palette entries ───────────────────────────────────────────────

  useEffect(() => {
    registry.register({
      id: "terminal:new",
      label: "New Terminal",
      keywords: ["new", "terminal", "open", "create", "spawn", "window"],
      group: "navigation",
      action: () => setCreatingRef.current(true),
    });
    registry.register({
      id: "terminal:close",
      label: "Close terminal",
      keywords: ["close", "terminal", "quit", "exit", "window"],
      group: "navigation",
      isAvailable: () => tsRef.current.terminals.length > 1,
      action: () => {
        const id = tsRef.current.current?.id;
        if (id) tsRef.current.close(id).catch(console.error);
      },
    });
    registry.register({
      id: "terminal:rename",
      label: "Rename terminal",
      keywords: ["rename", "terminal", "name", "title"],
      group: "navigation",
      action: () => setRenamingRef.current(true),
    });
    registry.register({
      id: "terminal:set-channel",
      label: "Set terminal FDC3 channel…",
      keywords: ["fdc3", "channel", "terminal", "set", "join"],
      group: "navigation",
      action: () => {
        const channelId = tsRef.current.current?.fdc3Channel ?? null;
        invoke("wm_channel_picker_open", {
          x: window.innerWidth / 2 - 80,
          y: 44,
          channelId,
        }).catch(console.error);
      },
    });
    return () => {
      registry.unregister("terminal:new");
      registry.unregister("terminal:close");
      registry.unregister("terminal:rename");
      registry.unregister("terminal:set-channel");
    };
  }, []);

  const pendingTerminalName =
    ts.terminals.find((t) => t.id === ts.pendingClose)?.name ?? "this terminal";

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        className="wm-ts__trigger"
        onClick={handleTriggerClick}
        title="Terminal switcher"
        aria-haspopup="listbox"
      >
        <span className="wm-ts__current-name">{ts.current?.name ?? "Terminal"}</span>
        <span className="wm-ts__caret" aria-hidden>
          ▾
        </span>
      </button>

      {ts.pendingClose !== null && (
        <ConfirmCloseDialog
          terminalName={pendingTerminalName}
          onSave={() => ts.confirmSave().catch(console.error)}
          onDiscard={() => ts.confirmDiscard().catch(console.error)}
          onCancel={ts.cancelClose}
        />
      )}

      {creating && (
        <NewTerminalDialog onConfirm={handleNew} onCancel={() => setCreating(false)} />
      )}

      {renaming && ts.current && (
        <RenameDialog
          currentName={ts.current.name}
          onConfirm={handleRename}
          onCancel={() => setRenaming(false)}
        />
      )}
    </>
  );
}
