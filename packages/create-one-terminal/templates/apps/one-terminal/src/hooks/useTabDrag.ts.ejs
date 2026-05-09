import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DropTarget, DropZone, HostLayout, StackHeader } from "../types";

/** Matches `HEADER_HEIGHT` in `src-tauri/src/layout/mod.rs`. Chrome header
 *  reserved at the top of the window; content area starts below it. */
const HEADER_HEIGHT = 40;

/** Width of the outer band along the window edge that triggers root-level
 *  split-out. Cursor inside this band while dragging a tab splits the entire
 *  root (targetPath: []), not just the hit-tested stack. */
const ROOT_EDGE_BAND = 24;

export interface TabDragState {
  sourceLabel: string;
  sourceStackPath: number[];
  title: string;
  pointerX: number;
  pointerY: number;
  target: DropTarget | null;
}

interface ArmedSource {
  label: string;
  stackPath: number[];
  tabIndex: number;
  title: string;
  downX: number;
  downY: number;
}

/** Pixel distance from pointerdown before we commit to a drag. */
const DRAG_THRESHOLD_SQ = 5 * 5;

export type TabDragEndResult =
  | { kind: "click"; stackPath: number[]; tabIndex: number }
  | { kind: "drop" }
  | { kind: "noop" };

/**
 * Tab drag lifecycle with click/drag disambiguation.
 *
 * `arm` records pointerdown state without starting a drag. `update` waits for
 * the cursor to move past a 5 px threshold before flipping into the rendered
 * drag state. `end` returns whether the gesture was a click (no threshold
 * crossed) or a drop so the caller can route a click to "switch active tab".
 *
 * Parking (`wm_begin_tab_drag`) is triggered lazily — only when the cursor
 * hovers a zone that would require chrome-under-panels visibility (edge
 * splits or body-center drops). Same-stack reorders and cross-stack tab-slot
 * insertions stay entirely within the chrome overlay, so their drop
 * indicators are visible without parking. This avoids N set_position IPCs
 * plus the visual flicker of panels leaving/returning for pure reorders.
 *
 * State is kept in a ref (mirrored into a counter `useState` for re-renders)
 * so `end` can read the latest state synchronously — React's functional-update
 * form isn't guaranteed to run inline, and we need the result synchronously.
 */
export function useTabDrag() {
  const armedRef = useRef<ArmedSource | null>(null);
  const stateRef = useRef<TabDragState | null>(null);
  const parkedRef = useRef(false);
  const [, setTick] = useState(0);
  const rerender = useCallback(() => setTick((t) => t + 1), []);

  const arm = useCallback(
    (label: string, stackPath: number[], tabIndex: number, title: string, x: number, y: number) => {
      armedRef.current = { label, stackPath, tabIndex, title, downX: x, downY: y };
    },
    []
  );

  const maybePark = useCallback((target: DropTarget | null) => {
    if (parkedRef.current) return;
    if (!needsPark(target)) return;
    invoke("wm_begin_tab_drag").catch(console.error);
    parkedRef.current = true;
  }, []);

  const update = useCallback(
    (x: number, y: number, host: HostLayout | null) => {
      const armed = armedRef.current;
      if (!armed) return;

      const stacks = host?.stacks ?? [];

      if (stateRef.current) {
        const target = resolveDropTarget(stacks, host, x, y);
        stateRef.current = {
          ...stateRef.current,
          pointerX: x,
          pointerY: y,
          target,
        };
        maybePark(target);
        rerender();
        return;
      }

      const dx = x - armed.downX;
      const dy = y - armed.downY;
      if (dx * dx + dy * dy < DRAG_THRESHOLD_SQ) return;

      const target = resolveDropTarget(stacks, host, x, y);
      stateRef.current = {
        sourceLabel: armed.label,
        sourceStackPath: armed.stackPath,
        title: armed.title,
        pointerX: x,
        pointerY: y,
        target,
      };
      maybePark(target);
      rerender();
    },
    [rerender, maybePark]
  );

  const end = useCallback((): TabDragEndResult => {
    const armed = armedRef.current;
    armedRef.current = null;
    const current = stateRef.current;
    stateRef.current = null;
    parkedRef.current = false;

    if (!current) {
      rerender();
      return armed
        ? { kind: "click", stackPath: armed.stackPath, tabIndex: armed.tabIndex }
        : { kind: "noop" };
    }
    const args: {
      sourceLabel: string;
      targetPath: number[] | null;
      zone: DropZone | null;
      insertIndex: number | null;
    } = current.target
      ? {
          sourceLabel: current.sourceLabel,
          targetPath: current.target.targetPath,
          zone: current.target.zone,
          insertIndex: current.target.insertIndex ?? null,
        }
      : {
          sourceLabel: current.sourceLabel,
          targetPath: null,
          zone: null,
          insertIndex: null,
        };
    invoke("wm_end_tab_drag", args).catch(console.error);
    rerender();
    return { kind: "drop" };
  }, [rerender]);

  return { state: stateRef.current, arm, update, end };
}

/** True when the drop would land on a body region (edge split or body-center)
 *  where the panel webview covers the chrome. Tab-slot insertions render
 *  inside the tab strip, which already sits above panels — no parking
 *  required. A null target means the cursor isn't over any stack, so the
 *  drop zone layer shows nothing and parking would be wasted. */
function needsPark(target: DropTarget | null): boolean {
  if (!target) return false;
  if (target.zone !== "center") return true;
  return target.insertIndex === undefined;
}

/** Root-edge band hit-test. Returns a detach target with `targetPath: []` when
 *  the cursor sits in the outer strip of the window content area. The rect
 *  highlights the half of the content area where the split will land. When
 *  the cursor is in a corner, horizontal (left/right) wins over vertical — an
 *  arbitrary but consistent tie-break. */
function resolveRootEdge(
  windowWidth: number,
  windowHeight: number,
  x: number,
  y: number
): DropTarget | null {
  const cy = HEADER_HEIGHT;
  if (y < cy || y >= windowHeight || x < 0 || x >= windowWidth) return null;
  const contentH = windowHeight - cy;

  if (x < ROOT_EDGE_BAND) {
    return {
      targetPath: [],
      zone: "left",
      rect: { x: 0, y: cy, width: windowWidth / 2, height: contentH },
    };
  }
  if (x >= windowWidth - ROOT_EDGE_BAND) {
    return {
      targetPath: [],
      zone: "right",
      rect: {
        x: windowWidth / 2,
        y: cy,
        width: windowWidth / 2,
        height: contentH,
      },
    };
  }
  if (y < cy + ROOT_EDGE_BAND) {
    return {
      targetPath: [],
      zone: "top",
      rect: { x: 0, y: cy, width: windowWidth, height: contentH / 2 },
    };
  }
  if (y >= windowHeight - ROOT_EDGE_BAND) {
    return {
      targetPath: [],
      zone: "bottom",
      rect: {
        x: 0,
        y: cy + contentH / 2,
        width: windowWidth,
        height: contentH / 2,
      },
    };
  }
  return null;
}

/** Hit-test cursor against stack rects. Returns a drop target that may be:
 *  - a tab-strip insertion (zone: "center", `insertIndex` set, thin-bar rect)
 *  - a root-edge detach (targetPath: [], edge zone, half-window rect)
 *  - a stack-center drop (zone: "center", no `insertIndex`, appends)
 *  - a stack-edge split (zone: left/right/top/bottom, half-panel rect)
 *
 * Priority: tab-strip hits always win (tab strip overlaps the root-top band,
 * and a user dragging along the strip is almost certainly reordering). Then
 * root-edge for cursors in the outer window band over panel bodies. Then
 * stack-level zones for everything inside.
 */
export function resolveDropTarget(
  stacks: StackHeader[],
  host: HostLayout | null,
  x: number,
  y: number
): DropTarget | null {
  // Priority 1: tab-strip insertion on any stack we're directly over.
  for (const s of stacks) {
    if (x < s.x || x >= s.x + s.width || y < s.y || y >= s.y + s.height) continue;
    if (y < s.y + s.tabStripHeight) {
      const slot = computeTabInsertion(s, x);
      if (slot) return slot;
      return {
        targetPath: s.path,
        zone: "center",
        rect: { x: s.x, y: s.y, width: s.width, height: s.height },
      };
    }
  }

  // Priority 2: root-edge band — whole-layout split-out when near the window
  // frame (over a panel body, since the tab-strip check above already ran).
  if (host && stacks.length > 0) {
    const rootEdge = resolveRootEdge(host.windowWidth, host.windowHeight, x, y);
    if (rootEdge) return rootEdge;
  }

  // Priority 3: stack-level center / edge zones.
  for (const s of stacks) {
    if (x < s.x || x >= s.x + s.width || y < s.y || y >= s.y + s.height) continue;
    // Tab-strip band already handled in priority 1; cursor is now in body.

    const u = (x - s.x) / s.width;
    const v = (y - s.y) / s.height;

    const centerBias = 0.25;
    if (Math.abs(u - 0.5) < centerBias && Math.abs(v - 0.5) < centerBias) {
      return {
        targetPath: s.path,
        zone: "center",
        rect: { x: s.x, y: s.y, width: s.width, height: s.height },
      };
    }

    const dists = { left: u, right: 1 - u, top: v, bottom: 1 - v } as const;
    let zone: DropZone = "left";
    let best = Infinity;
    (Object.keys(dists) as (keyof typeof dists)[]).forEach((k) => {
      if (dists[k] < best) {
        best = dists[k];
        zone = k;
      }
    });
    return { targetPath: s.path, zone, rect: edgeRect(s, zone) };
  }
  return null;
}

/** Measure the tab DOM nodes inside a stack's tab strip and return the
 *  insertion slot closest to the cursor's x position. Returns a DropTarget
 *  with `zone: "center"`, the resolved `insertIndex`, and a thin vertical bar
 *  `rect` for the drop indicator. `null` if the DOM can't be measured. */
function computeTabInsertion(s: StackHeader, x: number): DropTarget | null {
  if (typeof document === "undefined") return null;
  const stripSel = `[data-stack-path="${s.path.join(".")}"]`;
  const strip = document.querySelector(stripSel);
  if (!strip) return null;
  const tabs = Array.from(strip.querySelectorAll<HTMLElement>("[data-tab-index]"));
  if (tabs.length === 0) {
    // Empty strip — drop inserts at index 0.
    return {
      targetPath: s.path,
      zone: "center",
      insertIndex: 0,
      rect: { x: s.x, y: s.y, width: 2, height: s.tabStripHeight },
    };
  }

  // Find the nearest boundary — left edge of each tab, plus the right edge
  // of the last tab. `insertIndex` = tab.dataset.tabIndex of the boundary
  // (or tabs.length when we pick the trailing edge).
  const rects = tabs.map((el) => el.getBoundingClientRect());
  let bestDist = Infinity;
  let insertIndex = 0;
  let boundaryX = s.x;

  for (let i = 0; i < rects.length; i++) {
    const leftX = rects[i].left;
    const d = Math.abs(x - leftX);
    if (d < bestDist) {
      bestDist = d;
      insertIndex = Number(tabs[i].dataset.tabIndex ?? i);
      boundaryX = leftX;
    }
  }
  const lastRight = rects[rects.length - 1].right;
  const dEnd = Math.abs(x - lastRight);
  if (dEnd < bestDist) {
    bestDist = dEnd;
    insertIndex = tabs.length;
    boundaryX = lastRight;
  }

  const BAR_W = 3;
  return {
    targetPath: s.path,
    zone: "center",
    insertIndex,
    rect: {
      x: boundaryX - BAR_W / 2,
      y: s.y,
      width: BAR_W,
      height: s.tabStripHeight,
    },
  };
}

function edgeRect(s: StackHeader, zone: DropZone) {
  switch (zone) {
    case "left":
      return { x: s.x, y: s.y, width: s.width / 2, height: s.height };
    case "right":
      return { x: s.x + s.width / 2, y: s.y, width: s.width / 2, height: s.height };
    case "top":
      return { x: s.x, y: s.y, width: s.width, height: s.height / 2 };
    case "bottom":
      return { x: s.x, y: s.y + s.height / 2, width: s.width, height: s.height / 2 };
    default:
      return { x: s.x, y: s.y, width: s.width, height: s.height };
  }
}
