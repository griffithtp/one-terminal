import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { EngineBinding, LayoutSnapshot, SplitDir } from "../types";

export function useLayout() {
  const [layout, setLayout] = useState<LayoutSnapshot | null>(null);

  // ── Fetch initial snapshot + subscribe to layout changes ──────────────────
  useEffect(() => {
    invoke<LayoutSnapshot | null>("wm_snapshot")
      .then(setLayout)
      .catch(() => {});

    const unlisten = listen<LayoutSnapshot | null>("wm:layout", (e) => {
      setLayout(e.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // ── Commands ──────────────────────────────────────────────────────────────

  /**
   * Open a new panel.
   * - `dir` omitted / null → insert as a tab in the active panel's Stack
   *   (auto-wrapping the target into a new Stack if needed).
   * - `dir = "horizontal" | "vertical"` → split the target along that axis.
   * - `engineBinding` — engine the user picked. If it doesn't match this
   *   WM's pinned engine, the launch pops out into a stand-alone host
   *   window and no tab is added.
   */
  const openPanel = useCallback(
    (
      appId: string,
      url: string,
      title: string,
      engineBinding: EngineBinding | null = null,
      target?: string,
      dir: SplitDir | null = null
    ) =>
      invoke<LayoutSnapshot>("wm_open", {
        appId,
        url,
        title,
        target: target ?? null,
        dir,
        engineBinding,
      }),
    []
  );

  return { layout, openPanel };
}
