/**
 * useWidgetCatalog
 *
 * Loads the launcher's widget catalog (union of App Directory + local
 * widgets.config.json) and keeps it in sync with the user's App Directory
 * endpoint override. Resolves the effective endpoint from the terminal config
 * default plus the override store, then re-fetches live whenever the override
 * changes — so editing the endpoint in settings updates the catalog without a
 * restart.
 *
 * Shared by `useAppDirectory` (read-only Add Widget list) and `useAppLaunch`
 * (full launch state machine) so both surfaces see the same catalog.
 */

import { useEffect, useState } from "react";
import { getTerminalConfig } from "../lib/terminalConfig";
import { loadWidgetRegistry } from "../lib/widgetRegistry";
import { resolveEffectiveUrl, subscribe } from "../settings/appDirectorySettings";
import type { AppRecord } from "../types";

export function useWidgetCatalog(): AppRecord[] {
  const [apps, setApps] = useState<AppRecord[]>([]);

  useEffect(() => {
    let cancelled = false;

    const reload = () => {
      getTerminalConfig()
        .then((cfg) => loadWidgetRegistry(resolveEffectiveUrl(cfg.appDirectoryUrl)))
        .then((next) => {
          if (!cancelled) setApps(next);
        })
        .catch(() => {});
    };

    reload();
    // Re-fetch whenever the user saves a new endpoint override.
    const unsubscribe = subscribe(reload);

    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, []);

  return apps;
}
