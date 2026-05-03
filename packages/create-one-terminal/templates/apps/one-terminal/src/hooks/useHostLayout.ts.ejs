import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
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

  useEffect(() => {
    const unlisten = listen<HostLayout>("wm:host-layout", (e) => {
      setHost(e.payload);
    });
    return () => { unlisten.then((fn) => fn()); };
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
