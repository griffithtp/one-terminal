/**
 * Single source for the launcher's widget catalog.
 *
 * Delegates to the Rust `wm_list_apps` command, which returns the *union* of
 * the App Directory and the local widgets.config.json, each record tagged with
 * its `source` / `catalogId`. Pass `appDirectoryUrl` to point the appd source
 * at a user-overridden endpoint; omit it to use the terminal config default.
 */

import { invoke } from "@tauri-apps/api/core";
import type { AppRecord } from "../types";

export async function loadWidgetRegistry(appDirectoryUrl?: string): Promise<AppRecord[]> {
  try {
    const res = await invoke<{ applications: AppRecord[] }>("wm_list_apps", {
      appDirectoryUrl: appDirectoryUrl ?? null,
    });
    return res.applications ?? [];
  } catch {
    return [];
  }
}
