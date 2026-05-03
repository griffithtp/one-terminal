import type { TabDragState } from "../hooks/useTabDrag";

/**
 * Floating preview that follows the cursor during a tab drag. Visible only
 * inside the chrome webview — fine here because panels are parked offscreen
 * during the drag (see `wm_begin_tab_drag`).
 */
export function GhostLayer({ drag }: { drag: TabDragState | null }) {
  if (!drag) return null;
  return (
    <div
      className="wm-ghost"
      style={{
        position: "absolute",
        left: drag.pointerX + 12,
        top: drag.pointerY + 12,
        pointerEvents: "none",
      }}
    >
      <span className="wm-ghost__label">{shortLabel(drag.title)}</span>
    </div>
  );
}

function shortLabel(label: string) {
  return label.replace(/^panel-/, "");
}
