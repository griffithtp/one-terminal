import { invoke } from "@tauri-apps/api/core";
import { registry } from "./registry";
import type { AppRecord, TerminalConfig } from "../types";

type OpenPanelFn = (appId: string, url: string, title: string) => void;

/** Fetches App Directory records and registers one "Open <App>" command per
 *  app. Call once after the App Directory is reachable (after wm_config resolves). */
export async function initAppCommands(onOpen: OpenPanelFn): Promise<void> {
  let apps: AppRecord[];
  try {
    const cfg = await invoke<TerminalConfig>("wm_config");
    const res = await fetch(cfg.appDirectoryUrl);
    const data = await res.json();
    apps = data.applications ?? [];
  } catch {
    return;
  }

  for (const app of apps) {
    const title = app.title ?? app.name;
    const url = app.details?.url ?? "";
    registry.register({
      id: `app.open-${app.appId}`,
      label: `Open ${title}`,
      keywords: ["open", "launch", "app", title.toLowerCase(), app.appId],
      group: "apps",
      action: () => onOpen(app.appId, url, title),
    });
  }
}
