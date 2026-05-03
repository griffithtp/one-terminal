import type { DropTarget } from "../types";

/**
 * Blue translucent highlight over the region a drop would fill. Fed by
 * `useTabDrag.state.target`, so it tracks the cursor live as the user moves
 * across stacks and their zones. When `insertIndex` is set, the target
 * represents a tab-slot insertion; the rect is a thin vertical bar and we
 * render it as a solid indicator rather than a translucent fill.
 */
export function DropZoneLayer({ target }: { target: DropTarget | null }) {
  if (!target) return null;
  const { rect, zone, insertIndex } = target;
  const isInsertBar = insertIndex !== undefined;
  const className = isInsertBar
    ? "wm-drop-zone wm-drop-zone--insert-bar"
    : `wm-drop-zone wm-drop-zone--${zone}`;
  return (
    <div
      className={className}
      style={{
        position: "absolute",
        left: rect.x,
        top: rect.y,
        width: rect.width,
        height: rect.height,
        pointerEvents: "none",
      }}
    />
  );
}
