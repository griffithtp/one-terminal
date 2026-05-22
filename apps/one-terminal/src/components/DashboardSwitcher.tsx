/**
 * DashboardSwitcher
 *
 * Horizontal pill strip rendered in the header. Lets the user switch between
 * named dashboards, create new ones, rename and delete them, and reorder via
 * drag-and-drop.
 *
 * Dialogs (create, rename, unsaved-changes confirm) park panel webviews while
 * open so they're not occluded by content panels sitting above in z-order.
 */

import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { registry } from "../commands/registry";
import type { UseDashboardsResult, DashboardInfo } from "../hooks/useDashboards";
import "./DashboardSwitcher.css";

// ── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  ds: UseDashboardsResult;
}

// ── Confirm dialog ─────────────────────────────────────────────────────────────

interface ConfirmDialogProps {
  activeName: string;
  onSave: () => void;
  onDiscard: () => void;
  onCancel: () => void;
}

function ConfirmDialog({ activeName, onSave, onDiscard, onCancel }: ConfirmDialogProps) {
  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">Unsaved Changes</div>
        <p className="wm-ds-dialog__body">
          <strong>&ldquo;{activeName}&rdquo;</strong> has unsaved layout changes. Save before
          switching or discard them?
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

// ── Create dialog ─────────────────────────────────────────────────────────────

interface CreateDialogProps {
  onConfirm: (name: string) => void;
  onCancel: () => void;
}

function CreateDialog({ onConfirm, onCancel }: CreateDialogProps) {
  const [name, setName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter" && name.trim()) onConfirm(name.trim());
      if (e.key === "Escape") onCancel();
    },
    [name, onConfirm, onCancel]
  );

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">New Dashboard</div>
        <input
          ref={inputRef}
          className="wm-ds-dialog__input"
          placeholder="Dashboard name"
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
            disabled={!name.trim()}
            onClick={() => name.trim() && onConfirm(name.trim())}
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Rename dialog ─────────────────────────────────────────────────────────────

interface RenameDialogProps {
  currentName: string;
  onConfirm: (newName: string) => void;
  onCancel: () => void;
}

function RenameDialog({ currentName, onConfirm, onCancel }: RenameDialogProps) {
  const [name, setName] = useState(currentName);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.select();
  }, []);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter" && name.trim() && name.trim() !== currentName) onConfirm(name.trim());
      if (e.key === "Escape") onCancel();
    },
    [name, currentName, onConfirm, onCancel]
  );

  const canSubmit = name.trim().length > 0 && name.trim() !== currentName;

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">Rename Dashboard</div>
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

// ── Context menu ──────────────────────────────────────────────────────────────

interface CtxMenu {
  name: string;
  x: number;
  y: number;
}

// ── Main component ────────────────────────────────────────────────────────────

export function DashboardSwitcher({ ds }: Props) {
  const {
    dashboards,
    pendingSwitch,
    switchTo,
    confirmSave,
    confirmDiscard,
    cancelSwitch,
    create,
    rename,
    remove,
    reorder,
  } = ds;

  // ── Dialog visibility ─────────────────────────────────────────────────────
  const [creating, setCreating] = useState(false);
  const [renaming, setRenaming] = useState<string | null>(null); // dashboard name being renamed
  const [ctxMenu, setCtxMenu] = useState<CtxMenu | null>(null);

  // ── Park panels while any dialog is open ─────────────────────────────────
  const dialogOpen = creating || renaming !== null || pendingSwitch !== null;
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

  // ── Close context menu on outside click ───────────────────────────────────
  useEffect(() => {
    if (!ctxMenu) return;
    const close = () => setCtxMenu(null);
    window.addEventListener("pointerdown", close, { capture: true });
    return () => window.removeEventListener("pointerdown", close, { capture: true });
  }, [ctxMenu]);

  // ── Drag-to-reorder ───────────────────────────────────────────────────────
  const dragSourceRef = useRef<string | null>(null);
  const [dragOverName, setDragOverName] = useState<string | null>(null);

  const handleDragStart = useCallback((name: string) => {
    dragSourceRef.current = name;
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent, name: string) => {
    e.preventDefault();
    if (dragSourceRef.current !== name) setDragOverName(name);
  }, []);

  const handleDrop = useCallback(
    (e: React.DragEvent, targetName: string) => {
      e.preventDefault();
      setDragOverName(null);
      const source = dragSourceRef.current;
      dragSourceRef.current = null;
      if (!source || source === targetName) return;

      const names = dashboards.map((d) => d.name);
      const fromIdx = names.indexOf(source);
      const toIdx = names.indexOf(targetName);
      if (fromIdx === -1 || toIdx === -1) return;

      const next = [...names];
      next.splice(fromIdx, 1);
      next.splice(toIdx, 0, source);
      reorder(next).catch(console.error);
    },
    [dashboards, reorder]
  );

  const handleDragEnd = useCallback(() => {
    dragSourceRef.current = null;
    setDragOverName(null);
  }, []);

  // ── Context menu ──────────────────────────────────────────────────────────
  const handleContextMenu = useCallback((e: React.MouseEvent, name: string) => {
    e.preventDefault();
    setCtxMenu({ name, x: e.clientX, y: e.clientY });
  }, []);

  // ── Create ────────────────────────────────────────────────────────────────
  const handleCreate = useCallback(
    (name: string) => {
      setCreating(false);
      create(name)
        .then(() => switchTo(name))
        .catch(console.error);
    },
    [create, switchTo]
  );

  // ── Rename ────────────────────────────────────────────────────────────────
  const handleRename = useCallback(
    (newName: string) => {
      const old = renaming;
      setRenaming(null);
      if (!old) return;
      rename(old, newName).catch(console.error);
    },
    [renaming, rename]
  );

  // ── Delete ────────────────────────────────────────────────────────────────
  const handleDelete = useCallback(
    (name: string) => {
      setCtxMenu(null);
      remove(name).catch(console.error);
    },
    [remove]
  );

  // ── Command palette entries that need component state ─────────────────────
  const setCreatingRef = useRef(setCreating);
  const setRenamingRef = useRef(setRenaming);
  const dashboardsRef = useRef(dashboards);
  setCreatingRef.current = setCreating;
  setRenamingRef.current = setRenaming;
  dashboardsRef.current = dashboards;

  useEffect(() => {
    registry.register({
      id: "dashboard:create",
      label: "Create dashboard",
      keywords: ["create", "new", "dashboard", "add"],
      group: "navigation",
      action: () => setCreatingRef.current(true),
    });
    registry.register({
      id: "dashboard:rename",
      label: "Rename current dashboard",
      keywords: ["rename", "dashboard"],
      group: "navigation",
      isAvailable: () => dashboardsRef.current.length > 0,
      action: () => {
        const active = dashboardsRef.current.find((d) => d.active);
        if (active) setRenamingRef.current(active.name);
      },
    });
    return () => {
      registry.unregister("dashboard:create");
      registry.unregister("dashboard:rename");
    };
  }, []);

  // ── Active dashboard name (for confirm dialog body text) ──────────────────
  const activeName = dashboards.find((d) => d.active)?.name ?? "";

  return (
    <>
      <div className="wm-ds" data-tauri-drag-region>
        <div className="wm-ds__strip" data-tauri-drag-region>
          {dashboards.length === 0 ? (
            <span className="wm-ds__empty-hint" data-tauri-drag-region>
              No dashboards
            </span>
          ) : (
            dashboards.map((d) => (
              <Pill
                key={d.name}
                info={d}
                dragOver={dragOverName === d.name}
                onClick={() => switchTo(d.name).catch(console.error)}
                onClose={() => handleDelete(d.name)}
                onContextMenu={(e) => handleContextMenu(e, d.name)}
                onDragStart={() => handleDragStart(d.name)}
                onDragOver={(e) => handleDragOver(e, d.name)}
                onDrop={(e) => handleDrop(e, d.name)}
                onDragEnd={handleDragEnd}
              />
            ))
          )}
        </div>
        <button
          type="button"
          className="wm-ds__add"
          title="New dashboard"
          aria-label="New dashboard"
          onClick={() => setCreating(true)}
        >
          +
        </button>
      </div>

      {/* Unsaved-changes confirmation */}
      {pendingSwitch !== null && (
        <ConfirmDialog
          activeName={activeName}
          onSave={() => confirmSave().catch(console.error)}
          onDiscard={() => confirmDiscard().catch(console.error)}
          onCancel={cancelSwitch}
        />
      )}

      {/* New dashboard dialog */}
      {creating && <CreateDialog onConfirm={handleCreate} onCancel={() => setCreating(false)} />}

      {/* Rename dialog */}
      {renaming !== null && (
        <RenameDialog
          currentName={renaming}
          onConfirm={handleRename}
          onCancel={() => setRenaming(null)}
        />
      )}

      {/* Right-click context menu */}
      {ctxMenu && (
        <div className="wm-ds-ctx" style={{ top: ctxMenu.y, left: ctxMenu.x }}>
          <button
            type="button"
            className="wm-ds-ctx__item"
            onClick={() => {
              setRenaming(ctxMenu.name);
              setCtxMenu(null);
            }}
          >
            Rename
          </button>
          <button
            type="button"
            className="wm-ds-ctx__item wm-ds-ctx__item--danger"
            onClick={() => handleDelete(ctxMenu.name)}
          >
            Delete
          </button>
        </div>
      )}
    </>
  );
}

// ── Pill sub-component ────────────────────────────────────────────────────────

interface PillProps {
  info: DashboardInfo;
  dragOver: boolean;
  onClick: () => void;
  onClose: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
  onDragStart: () => void;
  onDragOver: (e: React.DragEvent) => void;
  onDrop: (e: React.DragEvent) => void;
  onDragEnd: () => void;
}

function Pill({
  info,
  dragOver,
  onClick,
  onClose,
  onContextMenu,
  onDragStart,
  onDragOver,
  onDrop,
  onDragEnd,
}: PillProps) {
  const classes = [
    "wm-ds__pill",
    info.active ? "wm-ds__pill--active" : "",
    dragOver ? "wm-ds__pill--drag-over" : "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <span
      className={classes}
      draggable
      onContextMenu={onContextMenu}
      onDragStart={onDragStart}
      onDragOver={onDragOver}
      onDrop={onDrop}
      onDragEnd={onDragEnd}
      title={info.dirty ? `${info.name} (unsaved changes)` : info.name}
    >
      {info.dirty && <span className="wm-ds__dirty" aria-label="unsaved changes" />}
      <button
        type="button"
        className="wm-ds__pill-label"
        onClick={onClick}
        aria-current={info.active ? "page" : undefined}
      >
        {info.name}
      </button>
      <button
        type="button"
        className="wm-ds__pill-close"
        aria-label={`Close ${info.name}`}
        onClick={(e) => { e.stopPropagation(); onClose(); }}
      >
        ✕
      </button>
    </span>
  );
}
