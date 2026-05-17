import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { HostLayout } from "../types";

interface UseHostLayoutResult {
  host: HostLayout | null;
  /**
   * Optimistically drop a tab by its webview label so the UI updates the
   * instant the user clicks ×, without waiting for Rust's reflow + emit
   * roundtrip. Any stack that empties as a result is dropped too. The next
   * `wm:host-layout` event overwrites this optimistic state with the truth.
   */
  removeTab: (label: string) => void;
}

/**
 * Subscribes to `wm:host-layout` events — the projection of the N-ary layout
 * tree into tab-strip and splitter-handle rects. Emitted by Rust whenever the
 * tree changes (`update_layout`) or the window is resized.
 */
export function useHostLayout(): UseHostLayoutResult {
  const [host, setHost] = useState<HostLayout | null>(null);
  // Track which labels have had zoom restored so we only do it once per session.
  const zoomRestored = useRef<Set<string>>(new Set());

  useEffect(() => {
    const restoreZoom = (layout: HostLayout) => {
      for (const stack of layout.stacks) {
        for (const tab of stack.tabs) {
          if (tab.zoomFactor !== 1.0 && !zoomRestored.current.has(tab.label)) {
            zoomRestored.current.add(tab.label);
            invoke("wm_set_zoom", { label: tab.label, zoomFactor: tab.zoomFactor }).catch(
              console.error
            );
          }
        }
      }
    };

    // Fetch the current host layout on mount — the Rust-side `wm:host-layout`
    // event fires during setup, before this listener is registered, so we'd
    // otherwise miss the initial state on launch/reload.
    invoke<HostLayout>("wm_host_snapshot")
      .then((layout) => {
        setHost(layout);
        restoreZoom(layout);
      })
      .catch(() => {});

    const unlisten = listen<HostLayout>("wm:host-layout", (e) => {
      setHost(e.payload);
      restoreZoom(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const removeTab = useCallback((label: string) => {
    setHost((prev) => {
      if (!prev) return prev;
      const stacks = prev.stacks
        .map((s) => ({ ...s, tabs: s.tabs.filter((t) => t.label !== label) }))
        .filter((s) => s.tabs.length > 0)
        .map((s) => ({ ...s, active: Math.min(s.active, s.tabs.length - 1) }));
      return { ...prev, stacks };
    });
  }, []);

  return { host, removeTab };
}
