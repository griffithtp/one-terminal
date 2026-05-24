/**
 * DashboardSwitcher
 *
 * Horizontal pill strip rendered in the header. Click a pill to switch.
 * Right-click opens a "Manage…" entry that deep-links the App Menu drawer's
 * Dashboards section, where full CRUD (rename, delete, reorder, auto-save
 * toggle) lives. The header keeps a "+" for quick create and the unsaved-
 * changes confirm dialog because both are triggered by header interactions.
 *
 * Drag-to-reorder also lives in the drawer (↑ / ↓ buttons) — the header
 * pill strip stays read-mostly so accidental drags don't reorder layouts
 * while the user is just switching.
 */

import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { registry } from "../commands/registry";
import type { UseDashboardsResult, DashboardInfo } from "../hooks/useDashboards";
import { popPark, pushPark } from "../lib/parkPanels";
import "./DashboardSwitcher.css";

// ── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  ds: UseDashboardsResult;
  /**
   * Deep-link into the App Menu drawer's Dashboards section. Called from
   * the right-click context menu's "Manage…" item and from the
   * `dashboard:rename` command palette entry.
   */
  onManageDashboards: () => void;
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

// ── Context menu ──────────────────────────────────────────────────────────────

interface CtxMenu {
  name: string;
  x: number;
  y: number;
}

// ── Main component ────────────────────────────────────────────────────────────

export function DashboardSwitcher({ ds, onManageDashboards }: Props) {
  const { dashboards, pendingSwitch, switchTo, confirmSave, confirmDiscard, cancelSwitch, create } =
    ds;

  // ── Dialog visibility ─────────────────────────────────────────────────────
  const [creating, setCreating] = useState(false);
  const [ctxMenu, setCtxMenu] = useState<CtxMenu | null>(null);

  // ── Park panels while any dialog is open (refcounted via parkPanels) ─────
  const dialogOpen = creating || pendingSwitch !== null;
  useEffect(() => {
    if (!dialogOpen) return;
    pushPark();
    return () => popPark();
  }, [dialogOpen]);

  // ── Close context menu on outside click ───────────────────────────────────
  useEffect(() => {
    if (!ctxMenu) return;
    const close = () => setCtxMenu(null);
    window.addEventListener("pointerdown", close, { capture: true });
    return () => window.removeEventListener("pointerdown", close, { capture: true });
  }, [ctxMenu]);

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

  // ── Command palette entries that need component state ─────────────────────
  // `dashboard:rename` now deep-links the drawer's Dashboards section rather
  // than opening an inline rename dialog — rename UI moved to DashboardsSection.
  const setCreatingRef = useRef(setCreating);
  const onManageRef = useRef(onManageDashboards);
  setCreatingRef.current = setCreating;
  onManageRef.current = onManageDashboards;

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
      keywords: ["rename", "dashboard", "manage"],
      group: "navigation",
      action: () => onManageRef.current(),
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
                onClick={() => switchTo(d.name).catch(console.error)}
                onContextMenu={(e) => handleContextMenu(e, d.name)}
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

      {/* Right-click context menu — single "Manage…" item that deep-links
          the drawer's Dashboards section for rename / delete / reorder. */}
      {ctxMenu && (
        <div className="wm-ds-ctx" style={{ top: ctxMenu.y, left: ctxMenu.x }}>
          <button
            type="button"
            className="wm-ds-ctx__item"
            onClick={() => {
              setCtxMenu(null);
              onManageDashboards();
            }}
          >
            Manage…
          </button>
        </div>
      )}
    </>
  );
}

// ── Pill sub-component ────────────────────────────────────────────────────────

interface PillProps {
  info: DashboardInfo;
  onClick: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
}

function Pill({ info, onClick, onContextMenu }: PillProps) {
  const classes = ["wm-ds__pill", info.active ? "wm-ds__pill--active" : ""]
    .filter(Boolean)
    .join(" ");

  return (
    <span
      className={classes}
      onContextMenu={onContextMenu}
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
    </span>
  );
}
