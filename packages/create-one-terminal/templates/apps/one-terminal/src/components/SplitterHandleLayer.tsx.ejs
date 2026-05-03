import { useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { SplitterHandle } from "../types";

/**
 * Renders one 4 px handle per gap inside a `Splitter`. Position comes
 * straight from `wm:host-layout`, so handles sit exactly in the gaps that
 * `reflow_layout` carved out.
 *
 * On pointer drag, we stream window-space `(clientX, clientY)` to Rust via
 * `wm_splitter_drag`. Rust mutates the two adjacent child weights in place
 * and re-emits the host layout, so the handle itself is repainted on the
 * next render with the new position — no local optimistic state needed.
 */
export function SplitterHandleLayer({ splitters }: { splitters: SplitterHandle[] }) {
  // One drag at a time across the whole layer.
  const dragRef = useRef<{ path: number[]; childIndex: number } | null>(null);

  return (
    <>
      {splitters.map((s) => (
        <div
          key={`split-${s.path.join("-")}-${s.childIndex}`}
          className={`wm-splitter wm-splitter--${s.direction}`}
          style={{
            position: "absolute",
            left: s.x,
            top: s.y,
            width: s.width,
            height: s.height,
            cursor: s.direction === "horizontal" ? "ew-resize" : "ns-resize",
            touchAction: "none",
          }}
          data-path={s.path.join(".")}
          data-child-index={s.childIndex}
          onPointerDown={(e) => {
            dragRef.current = { path: s.path, childIndex: s.childIndex };
            (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
            e.preventDefault();
          }}
          onPointerMove={(e) => {
            const drag = dragRef.current;
            if (!drag) return;
            invoke("wm_splitter_drag", {
              path: drag.path,
              childIndex: drag.childIndex,
              positionX: e.clientX,
              positionY: e.clientY,
            }).catch(console.error);
          }}
          onPointerUp={(e) => {
            dragRef.current = null;
            try {
              (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
            } catch {
              // pointer may have already been released
            }
          }}
          onPointerCancel={() => {
            dragRef.current = null;
          }}
        />
      ))}
    </>
  );
}
