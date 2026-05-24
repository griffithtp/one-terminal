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
import { EnginePickerDialog } from "../components/EnginePickerDialog";
import {
  DownloadPromptDialog,
  type DownloadEvent,
} from "../components/DownloadPromptDialog";
import { getTerminalConfig } from "../lib/terminalConfig";

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
    engineBinding: EngineBinding | null
  ) => void;
}

export interface UseAppLaunchResult {
  apps: AppRecord[];
  /** Engine bindings declared by the app for the current OS. */
  enginesFor: (app: AppRecord) => EngineBinding[];
  /**
   * Entry point: resolves engine (picker if multiple, auto if 0/1), prompts
   * download if needed, then opens the tab. Safe to call from any UI.
   */
  launchApp: (app: AppRecord) => void;
  /** Engine picker dialog node, or null when not shown. Mount once in App. */
  pickerNode: ReactNode | null;
  /** Download confirm/progress dialog node, or null when not shown. */
  downloadNode: ReactNode | null;
  errorMessage: string | null;
  clearError: () => void;
}

export function useAppLaunch({ onOpenTab }: UseAppLaunchOpts): UseAppLaunchResult {
  const [apps, setApps] = useState<AppRecord[]>([]);
  const [pickerApp, setPickerApp] = useState<AppRecord | null>(null);
  const [downloadPrompt, setDownloadPrompt] = useState<DownloadPrompt | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<DownloadEvent | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const currentOs = useMemo(() => detectCurrentOs(), []);

  // Fetch config from Rust once on mount, then load the app directory.
  useEffect(() => {
    getTerminalConfig()
      .then((c) => fetch(c.appDirectoryUrl))
      .then((r) => r.json())
      .then((d) => setApps(d.applications ?? []))
      .catch(() => {});
  }, []);

  // ── Park panels while any picker / download dialog is open ──────────────
  const dialogOpen = pickerApp !== null || downloadPrompt !== null;
  const lastParkedRef = useRef(false);
  useEffect(() => {
    if (dialogOpen && !lastParkedRef.current) {
      lastParkedRef.current = true;
      invoke("wm_park_panels").catch(console.error);
    } else if (!dialogOpen && lastParkedRef.current) {
      lastParkedRef.current = false;
      invoke("wm_unpark_panels").catch(console.error);
    }
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
    (app: AppRecord, engineBinding: EngineBinding | null) => {
      onOpenTab(app.appId, app.details?.url ?? "", app.title ?? app.name, engineBinding);
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
          doOpenTab(pending.app, pending.binding);
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
    (app: AppRecord) => {
      setErrorMessage(null);
      const engines = enginesFor(app);
      if (engines.length === 0) {
        // No engine constraint — let the WM use its own pinned engine.
        doOpenTab(app, null);
        return;
      }
      if (engines.length === 1) {
        proceedLaunch({ app, binding: engines[0] }).catch(console.error);
        return;
      }
      setPickerApp(app);
    },
    [enginesFor, doOpenTab, proceedLaunch]
  );

  const handlePickerPick = useCallback(
    (binding: EngineBinding) => {
      const app = pickerApp;
      setPickerApp(null);
      if (!app) return;
      proceedLaunch({ app, binding }).catch(console.error);
    },
    [pickerApp, proceedLaunch]
  );

  const handleDownloadConfirm = useCallback(async () => {
    const prompt = downloadPrompt;
    if (!prompt) return;
    setDownloading(true);
    setDownloadProgress(null);
    setErrorMessage(null);
    try {
      await invoke("wm_engine_install", { binding: prompt.pending.binding });
      // Sentinel is now in place; launch the app.
      const { app, binding } = prompt.pending;
      setDownloading(false);
      setDownloadPrompt(null);
      setDownloadProgress(null);
      doOpenTab(app, binding);
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

  const pickerNode: ReactNode = pickerApp ? (
    <EnginePickerDialog
      app={pickerApp}
      engines={enginesFor(pickerApp)}
      onCancel={() => setPickerApp(null)}
      onPick={handlePickerPick}
    />
  ) : null;

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
    pickerNode,
    downloadNode,
    errorMessage,
    clearError,
  };
}
