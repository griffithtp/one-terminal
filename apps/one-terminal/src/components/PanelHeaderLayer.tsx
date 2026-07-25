import { useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PanelBounds } from "../types";
import { headerContentFor } from "./panelHeaders";
import { FDC3_CHANNELS } from "./fdc3Channels";
import { PanelAddressBar } from "./PanelAddressBar";
import { ADDRESS_BAR_HEIGHT, GENERIC_WEB_WIDGET_APP_ID } from "../lib/genericWebWidget";

/** Height of the chrome-drawn per-panel header. Must match
 *  `PANEL_HEADER_HEIGHT` in src-tauri/src/layout/mod.rs. */
export const PANEL_HEADER_HEIGHT = 28;

interface Props {
  panels: PanelBounds[];
}

/**
 * Renders one header per open panel in the top `PANEL_HEADER_HEIGHT` slice of
 * each panel's rect. The chrome webview covers the full window, so pointer
 * `clientX/Y` already speak window-local coordinates — no translation needed.
 *
 * The per-panel `×` close button has been replaced by a `⋮` kebab that opens
 * the unified ctx menu in the overlay (Add Widget · Duplicate · Rename ·
 * Zoom · Close tab). Right-clicking the header still opens the same menu.
 */
export function PanelHeaderLayer({ panels }: Props) {
  return (
    <>
      {panels.map((p) => (
        <PanelHeader key={p.id} panel={p} />
      ))}
    </>
  );
}

function PanelHeader({ panel }: { panel: PanelBounds }) {
  const dragging = useRef(false);

  const handlePointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    // Kebab handles its own pointerdown — don't start a drag.
    if ((e.target as HTMLElement).closest("[data-panel-menu]")) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging.current = true;
    e.preventDefault();
  }, []);

  const handlePointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging.current) return;
      invoke("wm_drag_move", {
        panelId: panel.id,
        windowX: e.clientX,
        windowY: e.clientY,
      }).catch(console.error);
    },
    [panel.id]
  );

  const handlePointerUp = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!dragging.current) return;
    dragging.current = false;
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {}
  }, []);

  // Open the per-widget ctx menu anchored under the kebab. Standalone panels
  // aren't members of a Stack, so `stackPath` is empty + `nTabs` is 1. The
  // overlay only uses these for the "Close group" footer (suppressed for
  // standalone panels by kind === "tab" + empty stackPath path).
  // Anchor the menu's RIGHT edge at the kebab's right edge so the menu
  // drops down-left from the trigger. Items inside the menu remain
  // left-aligned (LTR text); only the box position differs.
  const openMenu = useCallback(
    (anchorRect: DOMRect) => {
      invoke("wm_ctx_menu_open", {
        x: anchorRect.right,
        y: anchorRect.bottom,
        stackPath: [],
        nTabs: 1,
        tabLabel: panel.id,
        appId: panel.appId,
        displayName: null,
        zoomFactor: null,
        fdc3Channel: panel.fdc3Channel ?? null,
        keepAlive: panel.keepAlive,
        showAddressBar: panel.showAddressBar,
        kind: "tab",
        maximized: false,
        anchor: "right",
      }).catch(console.error);
    },
    [panel.id, panel.appId, panel.fdc3Channel, panel.keepAlive, panel.showAddressBar]
  );

  const Content = useMemo(() => headerContentFor(panel.appId), [panel.appId]);
  const showAddressBar =
    panel.appId === GENERIC_WEB_WIDGET_APP_ID && panel.showAddressBar && !!panel.url;

  return (
    <>
      <div
        className="wm-panel-header"
        style={{
          position: "absolute",
          left: panel.x,
          top: panel.y,
          width: panel.width,
          height: PANEL_HEADER_HEIGHT,
          touchAction: "none",
        }}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        onContextMenu={(e) => {
          e.preventDefault();
          invoke("wm_ctx_menu_open", {
            x: e.clientX,
            y: e.clientY,
            stackPath: [],
            nTabs: 1,
            tabLabel: panel.id,
            appId: panel.appId,
            displayName: null,
            zoomFactor: null,
            fdc3Channel: panel.fdc3Channel ?? null,
            keepAlive: panel.keepAlive,
            showAddressBar: panel.showAddressBar,
            kind: "tab",
            maximized: false,
          }).catch(console.error);
        }}
      >
        {panel.fdc3Channel && (
          <span
            className="wm-panel-header__channel-dot"
            style={{
              background:
                FDC3_CHANNELS.find((c) => c.id === panel.fdc3Channel)?.color ?? "transparent",
            }}
            title={FDC3_CHANNELS.find((c) => c.id === panel.fdc3Channel)?.name}
            aria-hidden
          />
        )}
        {panel.keepAlive && (
          <span
            className="wm-panel-header__keep-alive-badge"
            title="Keeps running in the background across dashboard switches"
            aria-label="Keeps running in the background"
          >
            ⏺
          </span>
        )}
        <Content appId={panel.appId} title={panel.title} />
        <button
          type="button"
          data-panel-menu
          className="wm-panel-header__menu"
          aria-label={`${panel.title} menu`}
          title="More actions"
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            openMenu(e.currentTarget.getBoundingClientRect());
          }}
        >
          ⋮
        </button>
      </div>
      {showAddressBar && (
        <div
          style={{
            position: "absolute",
            left: panel.x,
            top: panel.y + PANEL_HEADER_HEIGHT,
            width: panel.width,
            height: ADDRESS_BAR_HEIGHT,
            zIndex: 6,
          }}
        >
          <PanelAddressBar url={panel.url} />
        </div>
      )}
    </>
  );
}
