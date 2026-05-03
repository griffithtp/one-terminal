import { useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { PanelBounds } from "../types";
import { headerContentFor } from "./panelHeaders";

/** Height of the chrome-drawn per-panel header. Must match
 *  `PANEL_HEADER_HEIGHT` in src-tauri/src/layout/mod.rs. */
export const PANEL_HEADER_HEIGHT = 28;

interface Props {
  panels: PanelBounds[];
  onClose: (panelId: string) => void;
}

/**
 * Renders one header per open panel in the top `PANEL_HEADER_HEIGHT` slice of
 * each panel's rect. The chrome webview covers the full window, so pointer
 * `clientX/Y` already speak window-local coordinates — no translation needed.
 */
export function PanelHeaderLayer({ panels, onClose }: Props) {
  return (
    <>
      {panels.map((p) => (
        <PanelHeader key={p.id} panel={p} onClose={onClose} />
      ))}
    </>
  );
}

function PanelHeader({
  panel,
  onClose,
}: {
  panel: PanelBounds;
  onClose: (panelId: string) => void;
}) {
  const dragging = useRef(false);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (e.button !== 0) return;
      // Close button handles its own pointerdown — don't start a drag.
      if ((e.target as HTMLElement).closest("[data-panel-close]")) return;
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      dragging.current = true;
      e.preventDefault();
    },
    [],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging.current) return;
      invoke("wm_drag_move", {
        panelId: panel.id,
        windowX: e.clientX,
        windowY: e.clientY,
      }).catch(console.error);
    },
    [panel.id],
  );

  const handlePointerUp = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!dragging.current) return;
      dragging.current = false;
      try {
        (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
      } catch {}
    },
    [],
  );

  const Content = headerContentFor(panel.appId);

  return (
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
    >
      <Content appId={panel.appId} title={panel.title} />
      <button
        type="button"
        data-panel-close
        className="wm-panel-header__close"
        aria-label={`Close ${panel.title}`}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          onClose(panel.id);
        }}
      >
        ×
      </button>
    </div>
  );
}
