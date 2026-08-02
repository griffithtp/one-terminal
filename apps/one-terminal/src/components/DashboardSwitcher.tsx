/**
 * DashboardSwitcher
 *
 * Horizontal pill strip rendered in the header. Click a pill to switch.
 * The "+" button opens the "New dashboard" prompt in the overlay (so
 * widgets stay visible behind it).
 *
 * Right-click (or the pill's kebab button) opens the dashboard tab's
 * context menu — Add widget, Duplicate, Rename, Set default channel…, Set
 * background running, Close dashboard, Manage… That menu is rendered in the
 * OVERLAY webview (`OverlayDashboardTabMenu.tsx`), not here: panel webviews
 * sit above the chrome webview in OS z-order, so a menu rendered from this
 * chrome-resident component would be visually clipped behind whatever
 * panels occupy that screen region. This component only fires the trigger
 * (`wm_dashboard_ctx_menu_open`) with the click position; the same pattern
 * the per-tab context menu already uses (see TabStripLayer.tsx / `wm_ctx_menu_open`).
 *
 * Inline rename (double-click a pill's label, or "Rename" from the overlay
 * menu via `wm:dashboard-request-rename`) stays local to this component —
 * the pill's own `<input>` lives in chrome regardless of where the menu
 * that triggered it renders.
 *
 * The unsaved-changes confirm and the new-dashboard prompt both live in
 * the overlay webview (see OverlayConfirmDashboardSwitch / OverlayCreate
 * Dashboard); chrome triggers them via Rust commands.
 */

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { registry } from "../commands/registry";
import type { UseDashboardsResult, DashboardInfo } from "../hooks/useDashboards";
import "./DashboardSwitcher.css";

// ── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  ds: UseDashboardsResult;
  /**
   * Deep-link into the App Menu drawer's Dashboards section. Called from
   * the "+" quick-create button and the `dashboard:rename` palette command
   * (the overlay menu's own "Manage…" item calls `wm_menu_open` directly).
   */
  onManageDashboards: () => void;
}

// ── Main component ────────────────────────────────────────────────────────────

export function DashboardSwitcher({ ds, onManageDashboards }: Props) {
  const { dashboards, switchTo, parkedCount, rename } = ds;

  const [renamingName, setRenamingName] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");

  const openCtxMenu = useCallback((name: string, x: number, y: number) => {
    invoke("wm_dashboard_ctx_menu_open", { name, x, y }).catch(console.error);
  }, []);

  const handleContextMenu = useCallback(
    (e: React.MouseEvent, name: string) => {
      e.preventDefault();
      openCtxMenu(name, e.clientX, e.clientY);
    },
    [openCtxMenu]
  );

  const handleKebabClick = useCallback(
    (e: React.MouseEvent, name: string) => {
      e.stopPropagation();
      const rect = e.currentTarget.getBoundingClientRect();
      openCtxMenu(name, rect.left, rect.bottom);
    },
    [openCtxMenu]
  );

  // ── Inline rename ──────────────────────────────────────────────────────────
  const startRename = useCallback((name: string) => {
    setRenamingName(name);
    setRenameValue(name);
  }, []);

  const commitRename = useCallback(() => {
    const oldName = renamingName;
    const newName = renameValue.trim();
    setRenamingName(null);
    if (!oldName || !newName || newName === oldName) return;
    rename(oldName, newName).catch(console.error);
  }, [renamingName, renameValue, rename]);

  const cancelRename = useCallback(() => {
    setRenamingName(null);
  }, []);

  // "Rename" in the overlay's dashboard context menu can't render its own
  // input (the pill lives here, in chrome) — it round-trips through this
  // event instead, mirroring `wm:request-rename` for tabs.
  useEffect(() => {
    const promise = listen<{ name: string }>("wm:dashboard-request-rename", (e) => {
      startRename(e.payload.name);
    });
    return () => {
      promise.then((fn) => fn()).catch(() => {});
    };
  }, [startRename]);

  // ── Command palette entries ───────────────────────────────────────────────
  // `dashboard:create` opens the small overlay prompt; `dashboard:rename`
  // deep-links the drawer's Dashboards section (per-row rename lives there).
  const openCreatePrompt = useCallback(() => {
    invoke("wm_dashboard_create_open").catch(console.error);
  }, []);

  useEffect(() => {
    registry.register({
      id: "dashboard:create",
      label: "Create dashboard",
      keywords: ["create", "new", "dashboard", "add"],
      group: "navigation",
      action: () => {
        invoke("wm_dashboard_create_open").catch(console.error);
      },
    });
    registry.register({
      id: "dashboard:rename",
      label: "Rename current dashboard",
      keywords: ["rename", "dashboard", "manage"],
      group: "navigation",
      action: onManageDashboards,
    });
    return () => {
      registry.unregister("dashboard:create");
      registry.unregister("dashboard:rename");
    };
  }, [onManageDashboards]);

  return (
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
              editing={renamingName === d.name}
              editValue={renameValue}
              onEditValueChange={setRenameValue}
              onCommitRename={commitRename}
              onCancelRename={cancelRename}
              onStartRename={() => startRename(d.name)}
              onClick={() => switchTo(d.name).catch(console.error)}
              onContextMenu={(e) => handleContextMenu(e, d.name)}
              onKebabClick={(e) => handleKebabClick(e, d.name)}
            />
          ))
        )}
      </div>
      {parkedCount > 0 && (
        <span
          className="wm-ds__parked"
          title={`${parkedCount} widget${parkedCount === 1 ? "" : "s"} running in the background`}
        >
          {parkedCount} running
        </span>
      )}
      <button
        type="button"
        className="wm-ds__add"
        title="New dashboard"
        aria-label="New dashboard"
        onClick={openCreatePrompt}
      >
        +
      </button>
    </div>
  );
}

// ── Pill sub-component ────────────────────────────────────────────────────────

interface PillProps {
  info: DashboardInfo;
  editing: boolean;
  editValue: string;
  onEditValueChange: (value: string) => void;
  onCommitRename: () => void;
  onCancelRename: () => void;
  onStartRename: () => void;
  onClick: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
  onKebabClick: (e: React.MouseEvent<HTMLButtonElement>) => void;
}

function Pill({
  info,
  editing,
  editValue,
  onEditValueChange,
  onCommitRename,
  onCancelRename,
  onStartRename,
  onClick,
  onContextMenu,
  onKebabClick,
}: PillProps) {
  const classes = ["wm-ds__pill", info.active ? "wm-ds__pill--active" : ""]
    .filter(Boolean)
    .join(" ");

  if (editing) {
    return (
      <span className={classes} title={info.name}>
        {info.dirty && <span className="wm-ds__dirty" aria-label="unsaved changes" />}
        <input
          autoFocus
          className="wm-ds__pill-input"
          value={editValue}
          onChange={(e) => onEditValueChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onCommitRename();
            else if (e.key === "Escape") onCancelRename();
          }}
          onBlur={onCommitRename}
          onClick={(e) => e.stopPropagation()}
        />
      </span>
    );
  }

  return (
    <span
      className={classes}
      onContextMenu={onContextMenu}
      title={info.dirty ? `${info.name} (unsaved changes)` : info.name}
    >
      {info.dirty && <span className="wm-ds__dirty" aria-label="unsaved changes" />}
      {info.lockedBy && (
        <span
          className="wm-ds__lock"
          title={`Open in ${info.lockedBy}`}
          aria-label={`Open in ${info.lockedBy}`}
        >
          🔒
        </span>
      )}
      <button
        type="button"
        className="wm-ds__pill-label"
        onClick={onClick}
        onDoubleClick={(e) => {
          e.stopPropagation();
          onStartRename();
        }}
        aria-current={info.active ? "page" : undefined}
      >
        {info.name}
      </button>
      <button
        type="button"
        className="wm-ds__pill-kebab"
        title="Dashboard menu"
        aria-label="Dashboard menu"
        onClick={onKebabClick}
      >
        ⋮
      </button>
    </span>
  );
}
