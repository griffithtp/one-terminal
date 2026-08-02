/**
 * App — root of the chrome webview.
 *
 * The chrome webview covers the entire Tauri window with a transparent
 * background.  Panel webviews are added on top (higher z-order) so they
 * receive all mouse events in their own rectangles.  The chrome is only
 * "visible" (and therefore interactive) in the areas not covered by panels:
 *
 *   • The 40 px header bar at y = 0
 *   • The splitter-handle strips between panels
 *   • The tab strips at the top of every Stack
 *
 * No pointer-events toggling or set_ignore_cursor_events is needed — the
 * z-ordering naturally routes events to the correct webview.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useLayout } from "./hooks/useLayout";
import { useHostLayout } from "./hooks/useHostLayout";
import { useTabDrag } from "./hooks/useTabDrag";
import { useAppLaunch } from "./hooks/useAppLaunch";
import { Header } from "./components/Header";
import { TabStripLayer } from "./components/TabStripLayer";
import { SplitterHandleLayer } from "./components/SplitterHandleLayer";
import { GhostLayer } from "./components/GhostLayer";
import { DropZoneLayer } from "./components/DropZoneLayer";
import { PanelHeaderLayer } from "./components/PanelHeaderLayer";
import { OverlayApp } from "./components/OverlayApp";
import { EmptyDashboardPicker } from "./components/EmptyDashboardPicker";
import { popPark, pushPark } from "./lib/parkPanels";
import { registerWidgetCommands, setActivePanelLabel } from "./commands/widgetCommands";
import { initAppCommands } from "./commands/appCommands";
import { applyKeybindingOverrides } from "./commands/keybindingStore";
import { registry } from "./commands/registry";
import { useDashboards } from "./hooks/useDashboards";
import type { AppRecord, EngineBinding, StackHeader } from "./types";
import "./wm.css";

// ── TerminalCloseDialog ───────────────────────────────────────────────────────
//
// Listens for `wm:confirm-close` (emitted by the Rust CloseRequested intercept)
// and shows a confirmation dialog. On confirm it calls `wm_close_terminal` which
// deletes persisted state and force-destroys the window. On cancel it dismisses.

function TerminalCloseDialog() {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    const unlisten = listen("wm:confirm-close", () => {
      pushPark();
      setVisible(true);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleConfirm = useCallback(() => {
    const label = getCurrentWindow().label;
    invoke("wm_close_terminal", { label }).catch(console.error);
    popPark();
    setVisible(false);
  }, []);

  const handleCancel = useCallback(() => {
    popPark();
    setVisible(false);
  }, []);

  if (!visible) return null;

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">Close Terminal</div>
        <p className="wm-ds-dialog__body">
          Close this Terminal? This will permanently remove it and all its dashboards.
        </p>
        <div className="wm-ds-dialog__actions">
          <button type="button" className="wm-ds-dialog__btn" onClick={handleCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--danger"
            onClick={handleConfirm}
          >
            Close Terminal
          </button>
        </div>
      </div>
    </div>
  );
}

export default function App() {
  // The overlay webview loads the same bundle with `#overlay` in the URL.
  // Render only the lightweight overlay UI; skip all chrome setup.
  if (window.location.hash === "#overlay") {
    return <OverlayApp />;
  }
  return <ChromeApp />;
}

function ChromeApp() {
  const { layout, openPanel } = useLayout();
  const { host: hostLayout } = useHostLayout();
  const tabDrag = useTabDrag();
  const dashboards = useDashboards();

  // ── App menu drawer state ─────────────────────────────────────────────────
  // The drawer is rendered inside the overlay webview (above panels in
  // z-order). Chrome only tracks whether it's open so the brand button can
  // toggle correctly; opening invokes `wm_menu_open` which raises the
  // overlay and emits `wm:menu-open` for OverlayMenu to listen to. Closing
  // happens overlay-side via `wm_ctx_menu_close`; OverlayMenu emits
  // `wm:menu-closed` so chrome syncs `menuOpen` back to false.
  const [menuOpen, setMenuOpen] = useState(false);

  const openMenuAt = useCallback((sectionId?: string) => {
    setMenuOpen(true);
    // The Shortcuts section requests its own snapshot on mount via
    // wm:keybindings-request — see KeybindingsSection.
    invoke("wm_menu_open", { initialSectionId: sectionId }).catch(console.error);
  }, []);

  const toggleMenu = useCallback(() => {
    if (menuOpen) {
      // Overlay-side dismiss path is the source of truth; just trigger it.
      invoke("wm_ctx_menu_close").catch(console.error);
      setMenuOpen(false);
    } else {
      openMenuAt();
    }
  }, [menuOpen, openMenuAt]);

  // Sync `menuOpen=false` when the overlay reports a dismiss (Escape /
  // backdrop / close button / app-launch). Without this, the brand button
  // would think the drawer is still open and try to close-then-reopen.
  useEffect(() => {
    const unlisten = listen("wm:menu-closed", () => setMenuOpen(false));
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // ── Command registry bootstrap (runs once per mount) ──────────────────────
  //
  // The `settings.keybindings` command opens the App Menu drawer at the
  // Shortcuts section (overlay-hosted editor). A ref keeps the action
  // pointing at the live setter without requiring re-registration when
  // openMenuAt's identity changes.
  const openMenuAtRef = useRef(openMenuAt);
  openMenuAtRef.current = openMenuAt;

  const commandsRegistered = useRef(false);
  useEffect(() => {
    if (commandsRegistered.current) return;
    commandsRegistered.current = true;
    registerWidgetCommands(() => openMenuAtRef.current("shortcuts"));
    applyKeybindingOverrides();
    initAppCommands((appId, url, title) =>
      openPanel(appId, url, title, null).catch(console.error)
    ).catch(console.error);
  }, [openPanel]);

  // Execute palette commands dispatched from the overlay webview.
  useEffect(() => {
    const unlisten = listen<string>("wm:palette-execute", (e) => {
      registry.execute(e.payload).catch(console.error);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Keep active panel label in sync so generic widget commands know their target.
  useEffect(() => {
    if (!layout || layout.panels.length === 0) {
      setActivePanelLabel(null);
      return;
    }
    // When a new panel appears set it as active; useLayout resolves the last opened.
    const last = layout.panels[layout.panels.length - 1];
    setActivePanelLabel(last.id);
  }, [layout]);

  // Show a status banner while the Electron binary is being auto-installed.
  const [electronStatus, setElectronStatus] = useState<"idle" | "installing" | "error">("idle");
  useEffect(() => {
    let errorTimer: ReturnType<typeof setTimeout>;
    const unlistenInstalling = listen("wm:electron-installing", () => {
      setElectronStatus("installing");
    });
    const unlistenReady = listen("wm:electron-ready", () => {
      setElectronStatus("idle");
    });
    const unlistenFailed = listen("wm:electron-install-failed", () => {
      setElectronStatus("error");
      errorTimer = setTimeout(() => setElectronStatus("idle"), 6000);
    });
    return () => {
      clearTimeout(errorTimer);
      unlistenInstalling.then((fn) => fn());
      unlistenReady.then((fn) => fn());
      unlistenFailed.then((fn) => fn());
    };
  }, []);

  const handleTabPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>, stack: StackHeader, tabIndex: number) => {
      // Only the primary (left) button arms a drag. Right-click opens the
      // tab strip's context menu; middle-click (and any others) are ignored
      // so they don't get disambiguated into a click → set_active_tab.
      if (e.button !== 0) return;
      const tab = stack.tabs[tabIndex];
      if (!tab) return;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      e.preventDefault();
      tabDrag.arm(tab.label, stack.path, tabIndex, tab.label, e.clientX, e.clientY);
    },
    [tabDrag]
  );

  const handleTabPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      tabDrag.update(e.clientX, e.clientY, hostLayout ?? null);
    },
    [tabDrag, hostLayout]
  );

  const handleTabPointerUp = useCallback(
    (_e: React.PointerEvent<HTMLDivElement>) => {
      // Browser auto-releases pointer capture on pointerup — don't call
      // releasePointerCapture here (.wm-chrome isn't the captor).
      const result = tabDrag.end();
      if (result.kind === "click") {
        invoke("wm_set_active_tab", {
          path: result.stackPath,
          tabIndex: result.tabIndex,
        }).catch(console.error);
        // Track the clicked tab as the active panel for generic widget commands.
        const stack = hostLayout?.stacks.find((s) => s.path.join() === result.stackPath.join());
        const tab = stack?.tabs[result.tabIndex];
        if (tab) setActivePanelLabel(tab.label);
      }
    },
    [tabDrag, hostLayout]
  );

  const handleOpenTab = useCallback(
    (
      appId: string,
      url: string,
      title: string,
      engineBinding: EngineBinding | null,
      target?: string | null
    ) => {
      // New panel joins `target`'s Stack when set (used by per-widget /
      // group kebab "Add Widget" + "Duplicate"), otherwise the active
      // panel's Stack (drawer / palette / empty-dashboard picker). Stack
      // auto-wraps the leaf on first grouping. Creating splits happens only
      // via tab drag-and-drop. When the user picks an engine that doesn't
      // match this WM's engine, Rust pops the launch out as a stand-alone
      // window and the tab list stays the same.
      openPanel(appId, url, title, engineBinding, target ?? undefined).catch(console.error);
    },
    [openPanel]
  );

  // Widget launch state machine — owns the app directory fetch, engine
  // picker, and download confirmation. Picker/download dialogs are returned
  // as ReactNodes and mounted at the root of the chrome so they survive
  // sibling subtrees (e.g. drawer, header) being torn down mid-flow.
  const launch = useAppLaunch({ onOpenTab: handleOpenTab });

  // Bridge for app launches initiated from the overlay drawer or kebab
  // menus. The overlay emits `wm:launch-app-from-menu` with the full
  // AppRecord (+ optional target label so per-widget / group kebab
  // "Add Widget" lands inside the source's Stack). Chrome runs the
  // launch through useAppLaunch so the picker / download dialogs render
  // in chrome (parked correctly via existing chrome parking).
  const launchAppRef = useRef(launch.launchApp);
  launchAppRef.current = launch.launchApp;
  // Duplicate fires from the per-widget kebab — the overlay only knows
  // the source's appId + label, so chrome resolves the AppRecord from
  // `launch.apps` and reuses the standard launch flow (fresh session,
  // engine picker re-prompt when multi-engine).
  const launchAppsRef = useRef(launch.apps);
  launchAppsRef.current = launch.apps;
  useEffect(() => {
    const unlistenLaunch = listen<{ app: AppRecord; target?: string | null }>(
      "wm:launch-app-from-menu",
      (e) => {
        launchAppRef.current(e.payload.app, e.payload.target ?? null);
      }
    );
    const unlistenDuplicate = listen<{ appId: string; target: string }>(
      "wm:duplicate-from-menu",
      (e) => {
        const app = launchAppsRef.current.find((a) => a.appId === e.payload.appId);
        if (!app) return;
        launchAppRef.current(app, e.payload.target);
      }
    );
    return () => {
      unlistenLaunch.then((fn) => fn());
      unlistenDuplicate.then((fn) => fn());
    };
  }, []);

  return (
    <div
      className="wm-chrome"
      onPointerMove={handleTabPointerMove}
      onPointerUp={handleTabPointerUp}
      onPointerCancel={handleTabPointerUp}
    >
      <Header
        errorMessage={launch.errorMessage ?? dashboards.lockMessage}
        onClearError={launch.errorMessage ? launch.clearError : dashboards.clearLockMessage}
        dashboards={dashboards}
        onMenuToggle={toggleMenu}
        onManageDashboards={() => openMenuAt("dashboards")}
      />

      {/* Drawer + engine picker render inside the overlay webview — see
          OverlayMenu and the EnginePickerDialog block in OverlayApp. The
          download prompt stays in chrome because it carries long-running
          progress UI; parking is acceptable while a download runs. */}

      {launch.downloadNode}

      {/* Per-panel headers — drag region + title + close button, painted in
          the top slice of every non-Stack leaf's rect. Stack members get
          their headers from the tab strip instead. */}
      {layout && <PanelHeaderLayer panels={layout.panels} />}

      {/* Host shell — tab strips + splitter handles driven by the N-ary tree */}
      {hostLayout && (
        <>
          <TabStripLayer stacks={hostLayout.stacks} onTabPointerDown={handleTabPointerDown} />
          <SplitterHandleLayer splitters={hostLayout.splitters} />
        </>
      )}

      {/* Tab-drag overlays — drop indicator + cursor-following ghost */}
      <DropZoneLayer target={tabDrag.state?.target ?? null} />
      <GhostLayer drag={tabDrag.state} />

      <TerminalCloseDialog />

      {/* Empty-state surfaces: no dashboards yet → CTA copy; empty
          dashboard → inline app picker so the first widget can be added
          without opening the drawer. */}
      {(!layout || layout.panels.length === 0) &&
        (!hostLayout || hostLayout.stacks.length === 0) &&
        (dashboards.dashboards.length === 0 ? (
          <div className="wm-empty">
            <p>Create a dashboard to get started.</p>
          </div>
        ) : (
          <EmptyDashboardPicker
            apps={launch.apps}
            enginesFor={launch.enginesFor}
            onSelect={launch.launchApp}
          />
        ))}

      {/* Electron auto-install status banner */}
      {electronStatus === "installing" && (
        <div className="wm-electron-status">
          Installing Electron… this may take a moment on first launch
        </div>
      )}
      {electronStatus === "error" && (
        <div className="wm-electron-status wm-electron-status--error">
          Electron install failed — check the terminal for details
        </div>
      )}
    </div>
  );
}
