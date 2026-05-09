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

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useLayout } from "./hooks/useLayout";
import { useHostLayout } from "./hooks/useHostLayout";
import { useTabDrag } from "./hooks/useTabDrag";
import { Header } from "./components/Header";
import { TabStripLayer } from "./components/TabStripLayer";
import { SplitterHandleLayer } from "./components/SplitterHandleLayer";
import { GhostLayer } from "./components/GhostLayer";
import { DropZoneLayer } from "./components/DropZoneLayer";
import { PanelHeaderLayer } from "./components/PanelHeaderLayer";
import { OverlayApp } from "./components/OverlayApp";
import type { EngineBinding, StackHeader } from "./types";
import "./wm.css";

export default function App() {
  // The overlay webview loads the same bundle with `#overlay` in the URL.
  // Render only the lightweight overlay UI; skip all chrome setup.
  if (window.location.hash === "#overlay") {
    return <OverlayApp />;
  }
  return <ChromeApp />;
}

function ChromeApp() {
  const { layout, openPanel, closePanel } = useLayout();
  const { host: hostLayout, removeTab } = useHostLayout();
  const tabDrag = useTabDrag();

  // Show a status banner while the Electron binary is being auto-installed.
  const [electronStatus, setElectronStatus] = useState<
    "idle" | "installing" | "error"
  >("idle");
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
      }
    },
    [tabDrag]
  );

  const handleTabClose = useCallback(
    (stack: StackHeader, tabIndex: number) => {
      const tab = stack.tabs[tabIndex];
      if (!tab) return;
      // Optimistic: drop the tab from local state immediately so it disappears
      // on the same frame as the click. Rust's next `wm:host-layout` emit
      // (after close_tab → reflow → emit_host) overwrites this with the truth.
      removeTab(tab.label);
      invoke("close_tab", { label: tab.label }).catch(console.error);
    },
    [removeTab]
  );

  const handleOpenTab = useCallback(
    (appId: string, url: string, title: string, engineBinding: EngineBinding | null) => {
      // New panel joins the active panel's Stack (auto-wrapping the active
      // leaf into a Stack on first grouping). Creating splits happens only
      // via tab drag-and-drop. When the user picks an engine that doesn't
      // match this WM's engine, Rust pops the launch out as a stand-alone
      // window and the tab list stays the same.
      openPanel(appId, url, title, engineBinding).catch(console.error);
    },
    [openPanel]
  );

  const handleClose = useCallback(
    (panelId: string) => {
      closePanel(panelId).catch(console.error);
    },
    [closePanel]
  );

  return (
    <div
      className="wm-chrome"
      onPointerMove={handleTabPointerMove}
      onPointerUp={handleTabPointerUp}
      onPointerCancel={handleTabPointerUp}
    >
      <Header onOpenTab={handleOpenTab} />

      {/* Per-panel headers — drag region + title + close button, painted in
          the top slice of every non-Stack leaf's rect. Stack members get
          their headers from the tab strip instead. */}
      {layout && <PanelHeaderLayer panels={layout.panels} onClose={handleClose} />}

      {/* Host shell — tab strips + splitter handles driven by the N-ary tree */}
      {hostLayout && (
        <>
          <TabStripLayer
            stacks={hostLayout.stacks}
            onTabPointerDown={handleTabPointerDown}
            onTabClose={handleTabClose}
          />
          <SplitterHandleLayer splitters={hostLayout.splitters} />
        </>
      )}

      {/* Tab-drag overlays — drop indicator + cursor-following ghost */}
      <DropZoneLayer target={tabDrag.state?.target ?? null} />
      <GhostLayer drag={tabDrag.state} />

      {/* Empty-state hint when no panels are open */}
      {(!layout || layout.panels.length === 0) &&
        (!hostLayout || hostLayout.stacks.length === 0) && (
          <div className="wm-empty">
            <p>No panels open — launch an app from the header.</p>
          </div>
        )}

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
