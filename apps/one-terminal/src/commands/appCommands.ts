import { loadWidgetRegistry } from "../lib/widgetRegistry";
import { registry } from "./registry";
import type { AppRecord } from "../types";

type OpenPanelFn = (appId: string, url: string, title: string) => void;

/** Loads the widget catalog (from App Directory in Enterprise, or the local
 *  registry in Standalone) and registers one "Open <App>" command per entry.
 *  Call once after Tauri is ready. */
export async function initAppCommands(onOpen: OpenPanelFn): Promise<void> {
  const apps: AppRecord[] = await loadWidgetRegistry();
  if (apps.length === 0) return;

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
