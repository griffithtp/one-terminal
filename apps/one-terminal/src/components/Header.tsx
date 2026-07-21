/**
 * Header
 *
 * 40 px toolbar that sits at the top of the chrome webview.
 *
 * After Plan 10, the header is intentionally minimal: brand/menu button on
 * the left, dashboard switcher, optional launch-error banner, then window
 * controls on the right. Widget launching moved into the App Menu drawer's
 * Add Widget section (see AppMenuSidebar). Launch errors still surface here
 * via `errorMessage` / `onClearError` because the header is the
 * always-visible status surface.
 *
 * FDC3 channel selection (Plan 11) is per-widget, not per-Terminal — there is
 * no channel control here anymore. See the tab strip's "Set channel" context
 * menu item (`OverlayApp.tsx`) and `wm_set_panel_fdc3_channel`.
 */

import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { DashboardSwitcher } from "./DashboardSwitcher";
import type { UseDashboardsResult } from "../hooks/useDashboards";
import { getTerminalConfig } from "../lib/terminalConfig";

interface Props {
  /** Most recent launch error from useAppLaunch (null when clean). */
  errorMessage: string | null;
  /** Dismiss the inline error banner. */
  onClearError: () => void;
  /** Dashboard state + actions from the parent's useDashboards() call. */
  dashboards: UseDashboardsResult;
  /** Open / close the App Menu drawer (rendered by the parent). */
  onMenuToggle: () => void;
  /** Deep-link callback: open the drawer at the Dashboards section. */
  onManageDashboards: () => void;
}

export function Header({
  errorMessage,
  onClearError,
  dashboards,
  onMenuToggle,
  onManageDashboards,
}: Props) {
  const [terminalTitle, setTerminalTitle] = useState<string>("OneTerminal");

  // Hydrate terminal title once for the brand label.
  useEffect(() => {
    getTerminalConfig()
      .then((c) => {
        if (c.title) setTerminalTitle(c.title);
      })
      .catch(() => {});
  }, []);

  // Tauri's automatic data-tauri-drag-region handler is only injected into
  // the primary webview, not child webviews. Our chrome is a child webview
  // (win.add_child), so the attribute alone doesn't drag — we have to call
  // startDragging() ourselves. The attribute serves as the opt-in marker.
  const handleHeaderPointerDown = useCallback((e: React.PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    if (!(e.target as HTMLElement).hasAttribute("data-tauri-drag-region")) return;
    getCurrentWindow().startDragging().catch(console.error);
  }, []);

  const handleHeaderDoubleClick = useCallback((e: React.MouseEvent<HTMLElement>) => {
    if (!(e.target as HTMLElement).hasAttribute("data-tauri-drag-region")) return;
    getCurrentWindow().toggleMaximize().catch(console.error);
  }, []);

  const handleMinimize = useCallback(() => {
    getCurrentWindow().minimize().catch(console.error);
  }, []);

  const handleMaximize = useCallback(() => {
    getCurrentWindow().toggleMaximize().catch(console.error);
  }, []);

  const handleClose = useCallback(() => {
    getCurrentWindow().close().catch(console.error);
  }, []);

  return (
    <header
      className="wm-header"
      data-interactive
      data-tauri-drag-region
      onPointerDown={handleHeaderPointerDown}
      onDoubleClick={handleHeaderDoubleClick}
    >
      <button
        type="button"
        className="wm-header__brand-btn"
        onClick={onMenuToggle}
        title="Open menu"
        aria-label="Open menu"
        aria-haspopup="menu"
      >
        <span className="wm-header__brand-icon" aria-hidden>
          ☰
        </span>
        <span className="wm-header__brand-label">{terminalTitle}</span>
      </button>

      <DashboardSwitcher ds={dashboards} onManageDashboards={onManageDashboards} />

      {/* Spacer pushes window controls to the right and absorbs unused
          horizontal space as a drag region. */}
      <div className="wm-header__spacer" data-tauri-drag-region />

      {errorMessage && (
        <div className="wm-header__error" role="alert">
          {errorMessage}
          <button
            type="button"
            className="wm-header__error-close"
            onClick={onClearError}
            aria-label="Dismiss"
          >
            ✕
          </button>
        </div>
      )}

      <div className="wm-header__controls">
        <button
          type="button"
          className="wm-header__control-btn"
          title="Minimize"
          aria-label="Minimize"
          onClick={handleMinimize}
        >
          &#x2012;
        </button>
        <button
          type="button"
          className="wm-header__control-btn"
          title="Maximize"
          aria-label="Maximize"
          onClick={handleMaximize}
        >
          &#x25A2;
        </button>
        <button
          type="button"
          className="wm-header__control-btn wm-header__control-btn--close"
          title="Close"
          aria-label="Close"
          onClick={handleClose}
        >
          &#x2715;
        </button>
      </div>
    </header>
  );
}
