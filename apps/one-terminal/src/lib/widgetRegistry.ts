/**
 * Single source for the launcher's widget catalog.
 *
 * Delegates to the Rust `wm_list_apps` command, which dispatches between the
 * HTTP App Directory backend (Enterprise) and the local widgets.config.json
 * backend (Standalone) based on the terminal config's `widgetSource`. Callers
 * receive the same AppRecord[] shape either way.
 */

import { invoke } from "@tauri-apps/api/core";
import type { AppRecord } from "../types";

export async function loadWidgetRegistry(): Promise<AppRecord[]> {
  try {
    const res = await invoke<{ applications: AppRecord[] }>("wm_list_apps");
    return res.applications ?? [];
  } catch {
    return [];
  }
}
