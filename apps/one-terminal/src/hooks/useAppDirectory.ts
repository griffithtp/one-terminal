/**
 * useAppDirectory
 *
 * Read-only slice of the App Directory — fetches the registered apps and
 * exposes a per-OS `enginesFor` helper. Used by surfaces that need to list
 * or filter apps but don't own the launch state machine (e.g. the overlay's
 * Add Widget section, which emits an event back to chrome instead of
 * launching directly).
 *
 * The full launch state machine (picker / download / errors) lives in
 * `useAppLaunch` and continues to run in the chrome webview so dialogs
 * survive the drawer closing mid-flow.
 */

import { useCallback, useMemo } from "react";
import { useWidgetCatalog } from "./useWidgetCatalog";
import type { AppRecord, EngineBinding, OsKey } from "../types";

function detectCurrentOs(): OsKey {
  const ua = navigator.userAgent;
  if (/Mac|iPhone|iPad/i.test(ua)) return "macos";
  if (/Windows/i.test(ua)) return "windows";
  return "linux";
}

export interface UseAppDirectoryResult {
  apps: AppRecord[];
  enginesFor: (app: AppRecord) => EngineBinding[];
}

export function useAppDirectory(): UseAppDirectoryResult {
  const apps = useWidgetCatalog();
  const currentOs = useMemo(() => detectCurrentOs(), []);

  const enginesFor = useCallback(
    (app: AppRecord): EngineBinding[] => app.engineBindings?.[currentOs] ?? [],
    [currentOs]
  );

  return { apps, enginesFor };
}
