/**
 * useAppLaunch
 *
 * Owns the full widget-launch state machine: app directory fetch, engine
 * selection (auto / picker), download confirmation, download progress.
 * Returns the rendered picker / download dialogs as ReactNodes so consumers
 * can mount them at any level of the tree. Picker + download dialogs are
 * rendered at the App level so they survive any sub-tree (sidebar, header)
 * being torn down mid-flow — e.g. a download that takes 30 s shouldn't be
 * killed by closing the App Menu drawer.
 *
 * Panels are parked while any dialog is open (Tauri panel webviews sit above
 * the chrome in z-order; without parking, clicks fall through to whichever
 * panel covers the dialog).
 */

import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppRecord, EngineBinding, OsKey } from "../types";
import { DownloadPromptDialog, type DownloadEvent } from "../components/DownloadPromptDialog";
import { useWidgetCatalog } from "./useWidgetCatalog";
import { popPark, pushPark } from "../lib/parkPanels";

// ── Tauri-side EngineStatus payload ──────────────────────────────────────────
//
// Shape mirrors `engines::EngineStatus` in the WM's lib. Discriminator is the
// `status` field — kebab-case from `#[serde(rename_all = "kebab-case")]`.
type EngineStatus =
  | { status: "ready"; family: string; version: string; path?: string }
  | {
      status: "needs-download";
      family: string;
      version: string;
      label: string;
      sizeBytes: number;
    }
  | { status: "unsupported"; family: string; version: string; message: string };

function detectCurrentOs(): OsKey {
  const ua = navigator.userAgent;
  if (/Mac|iPhone|iPad/i.test(ua)) return "macos";
  if (/Windows/i.test(ua)) return "windows";
  return "linux";
}

interface PendingLaunch {
  app: AppRecord;
  binding: EngineBinding;
  /** When set, the new tab joins this label's Stack. */
  target?: string | null;
}

interface DownloadPrompt {
  pending: PendingLaunch;
  label: string;
  sizeBytes: number;
}

export interface UseAppLaunchOpts {
  /**
   * Called once the engine is resolved (and downloaded if needed) — opens
   * the app as a new tab in the WM. `engineBinding` is null when the app
   * declares no engine constraint (WM uses its own pinned engine).
   */
  onOpenTab: (
    appId: string,
    url: string,
    title: string,
    engineBinding: EngineBinding | null,
    target?: string | null
  ) => void;
}

export interface UseAppLaunchResult {
  apps: AppRecord[];
  /** Engine bindings declared by the app for the current OS. */
  enginesFor: (app: AppRecord) => EngineBinding[];
  /**
   * Entry point: resolves engine (picker if multiple, auto if 0/1), prompts
   * download if needed, then opens the tab. Safe to call from any UI.
   *
   * `target` — when set, the new tab joins this panel label's Stack
   * (auto-wrapping it into a new Stack if needed). Used by the per-widget
   * and group kebab menus so "Add Widget" / "Duplicate" land alongside
   * the source widget rather than at the active panel.
   */
  launchApp: (app: AppRecord, target?: string | null) => void;
  /** Download confirm/progress dialog node, or null when not shown. */
  downloadNode: ReactNode | null;
  errorMessage: string | null;
  clearError: () => void;
}

export function useAppLaunch({ onOpenTab }: UseAppLaunchOpts): UseAppLaunchResult {
  // Catalog (union of App Directory + local widgets) with live endpoint refresh.
  const apps = useWidgetCatalog();
  const [downloadPrompt, setDownloadPrompt] = useState<DownloadPrompt | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<DownloadEvent | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const currentOs = useMemo(() => detectCurrentOs(), []);

  // ── Park panels while the download dialog is open ───────────────────────
  // The engine picker is overlay-resident now (see `wm_engine_picker_open`)
  // so panels are not parked while it's visible — widgets stay rendered
  // behind the picker. The download prompt still renders in chrome because
  // it carries long-running progress UI; parking is acceptable there.
  const dialogOpen = downloadPrompt !== null;
  useEffect(() => {
    if (!dialogOpen) return;
    pushPark();
    return () => popPark();
  }, [dialogOpen]);

  // ── Listen for download progress while a download is in flight ───────────
  useEffect(() => {
    if (!downloading) return;
    const promises = [
      listen<DownloadEvent>("engine:download:start", (e) => setDownloadProgress(e.payload)),
      listen<DownloadEvent>("engine:download:progress", (e) => setDownloadProgress(e.payload)),
    ];
    return () => {
      promises.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, [downloading]);

  const enginesFor = useCallback(
    (app: AppRecord): EngineBinding[] => app.engineBindings?.[currentOs] ?? [],
    [currentOs]
  );

  const doOpenTab = useCallback(
    (app: AppRecord, engineBinding: EngineBinding | null, target?: string | null) => {
      onOpenTab(app.appId, app.details?.url ?? "", app.title ?? app.name, engineBinding, target);
    },
    [onOpenTab]
  );

  // Resolve `binding` then either launch (Ready) or show the download
  // confirmation dialog (NeedsDownload). Errors surface via errorMessage.
  const proceedLaunch = useCallback(
    async (pending: PendingLaunch) => {
      try {
        const status = await invoke<EngineStatus>("wm_engine_status", {
          binding: pending.binding,
        });
        if (status.status === "ready") {
          doOpenTab(pending.app, pending.binding, pending.target);
        } else if (status.status === "needs-download") {
          setDownloadPrompt({
            pending,
            label: status.label,
            sizeBytes: status.sizeBytes,
          });
        } else {
          setErrorMessage(status.message);
        }
      } catch (e) {
        setErrorMessage(e instanceof Error ? e.message : String(e));
      }
    },
    [doOpenTab]
  );

  const launchApp = useCallback(
    (app: AppRecord, target?: string | null) => {
      setErrorMessage(null);
      const engines = enginesFor(app);
      if (engines.length === 0) {
        // No engine constraint — let the WM use its own pinned engine.
        doOpenTab(app, null, target);
        return;
      }
      if (engines.length === 1) {
        proceedLaunch({ app, binding: engines[0], target }).catch(console.error);
        return;
      }
      // Multi-engine: render the picker in the overlay so widgets stay
      // visible and clicks land on the dialog. The overlay sends back the
      // chosen binding via `wm:engine-picker-pick` (or `…-cancel`).
      invoke("wm_engine_picker_open", {
        payload: { app, engines, target: target ?? null },
      }).catch(console.error);
    },
    [enginesFor, doOpenTab, proceedLaunch]
  );

  // Listen for the overlay's engine-picker responses and resume the launch
  // flow. `appsRef` shields the listener from stale closures over the apps
  // list — App Directory loads asynchronously after mount.
  const appsRef = useRef(apps);
  appsRef.current = apps;
  useEffect(() => {
    const subs = [
      listen<{ appId: string; binding: EngineBinding; target: string | null }>(
        "wm:engine-picker-pick",
        (e) => {
          const app = appsRef.current.find((a) => a.appId === e.payload.appId);
          if (!app) return;
          proceedLaunch({
            app,
            binding: e.payload.binding,
            target: e.payload.target,
          }).catch(console.error);
        }
      ),
      listen("wm:engine-picker-cancel", () => {
        // User cancelled — nothing to clean up; chrome held no picker state.
      }),
    ];
    return () => {
      subs.forEach((p) => p.then((fn) => fn()).catch(() => {}));
    };
  }, [proceedLaunch]);

  const handleDownloadConfirm = useCallback(async () => {
    const prompt = downloadPrompt;
    if (!prompt) return;
    setDownloading(true);
    setDownloadProgress(null);
    setErrorMessage(null);
    try {
      await invoke("wm_engine_install", { binding: prompt.pending.binding });
      // Sentinel is now in place; launch the app.
      const { app, binding, target } = prompt.pending;
      setDownloading(false);
      setDownloadPrompt(null);
      setDownloadProgress(null);
      doOpenTab(app, binding, target);
    } catch (e) {
      setDownloading(false);
      setErrorMessage(e instanceof Error ? e.message : String(e));
    }
  }, [downloadPrompt, doOpenTab]);

  const handleDownloadCancel = useCallback(() => {
    if (downloading) return;
    setDownloadPrompt(null);
    setDownloadProgress(null);
    setErrorMessage(null);
  }, [downloading]);

  const clearError = useCallback(() => setErrorMessage(null), []);

  const downloadNode: ReactNode = downloadPrompt ? (
    <DownloadPromptDialog
      binding={downloadPrompt.pending.binding}
      label={downloadPrompt.label}
      sizeBytes={downloadPrompt.sizeBytes}
      busy={downloading}
      progress={downloadProgress}
      errorMessage={errorMessage}
      onConfirm={handleDownloadConfirm}
      onCancel={handleDownloadCancel}
    />
  ) : null;

  return {
    apps,
    enginesFor,
    launchApp,
    downloadNode,
    errorMessage,
    clearError,
  };
}
