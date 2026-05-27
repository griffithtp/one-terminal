/**
 * DashboardSwitcher
 *
 * Horizontal pill strip rendered in the header. Click a pill to switch.
 * The "+" button opens the "New dashboard" prompt in the overlay (so
 * widgets stay visible behind it). Right-click opens "Manage…" which
 * deep-links the App Menu drawer's Dashboards section for rename / delete
 * / reorder.
 *
 * The unsaved-changes confirm and the new-dashboard prompt both live in
 * the overlay webview (see OverlayConfirmDashboardSwitch / OverlayCreate
 * Dashboard); chrome triggers them via Rust commands.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { registry } from "../commands/registry";
import type { UseDashboardsResult, DashboardInfo } from "../hooks/useDashboards";
import "./DashboardSwitcher.css";

// ── Props ─────────────────────────────────────────────────────────────────────

interface Props {
  ds: UseDashboardsResult;
  /**
   * Deep-link into the App Menu drawer's Dashboards section. Called from
   * the right-click context menu, the "+" quick-create button, and from
   * the `dashboard:create` / `dashboard:rename` palette commands.
   */
  onManageDashboards: () => void;
}

// ── Context menu ──────────────────────────────────────────────────────────────

interface CtxMenu {
  name: string;
  x: number;
  y: number;
}

// ── Main component ────────────────────────────────────────────────────────────

export function DashboardSwitcher({ ds, onManageDashboards }: Props) {
  const { dashboards, switchTo } = ds;

  const [ctxMenu, setCtxMenu] = useState<CtxMenu | null>(null);

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

  // ── Command palette entries ───────────────────────────────────────────────
  // `dashboard:create` opens the small overlay prompt; `dashboard:rename`
  // deep-links the drawer's Dashboards section (per-row rename lives there).
  const onManageRef = useRef(onManageDashboards);
  onManageRef.current = onManageDashboards;

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
      action: () => onManageRef.current(),
    });
    return () => {
      registry.unregister("dashboard:create");
      registry.unregister("dashboard:rename");
    };
  }, []);

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
          onClick={openCreatePrompt}
        >
          +
        </button>
      </div>

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
