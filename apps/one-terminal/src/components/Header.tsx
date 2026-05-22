/**
 * Header
 *
 * 40 px toolbar that sits at the top of the chrome webview.
 * Fetches the FDC3 App Directory (port 3005) and renders a launch button per
 * registered app. Clicking launches the app as a new tab in the active group.
 *
 * When the app declares more than one supported engine for this OS the user
 * is prompted to pick which engine to launch with. If that engine isn't
 * installed locally yet the user is offered a download confirmation; the
 * actual install runs in Rust and emits progress events back here.
 *
 * Open-panel chips are rendered here too — one per open panel — with a Close
 * (×) button. Creating splits happens exclusively via tab drag-and-drop.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppRecord, EngineBinding, OsKey, TerminalConfig } from "../types";
import { DashboardSwitcher } from "./DashboardSwitcher";
import type { UseDashboardsResult } from "../hooks/useDashboards";

// ── FDC3 system channels ─────────────────────────────────────────────────────
//
// The 8 standard FDC3 2.2 system channels with their display colours.

interface FdcChannel {
  id: string;
  name: string;
  color: string;
}

const FDC3_CHANNELS: FdcChannel[] = [
  { id: "fdc3.channel.1", name: "Channel 1", color: "#e11d48" },
  { id: "fdc3.channel.2", name: "Channel 2", color: "#ea580c" },
  { id: "fdc3.channel.3", name: "Channel 3", color: "#ca8a04" },
  { id: "fdc3.channel.4", name: "Channel 4", color: "#16a34a" },
  { id: "fdc3.channel.5", name: "Channel 5", color: "#0891b2" },
  { id: "fdc3.channel.6", name: "Channel 6", color: "#2563eb" },
  { id: "fdc3.channel.7", name: "Channel 7", color: "#7c3aed" },
  { id: "fdc3.channel.8", name: "Channel 8", color: "#db2777" },
];

// ── ChannelSelector ───────────────────────────────────────────────────────────

interface ChannelSelectorProps {
  channelId: string | null;
  onPillClick: (rect: DOMRect) => void;
}

function ChannelSelector({ channelId, onPillClick }: ChannelSelectorProps) {
  const btnRef = useRef<HTMLButtonElement>(null);
  const current = FDC3_CHANNELS.find((c) => c.id === channelId) ?? null;

  const handleClick = useCallback(() => {
    if (btnRef.current) onPillClick(btnRef.current.getBoundingClientRect());
  }, [onPillClick]);

  return (
    <button
      ref={btnRef}
      type="button"
      className="wm-channel-selector__pill"
      title={current ? `FDC3: ${current.name} — click to change` : "FDC3: No channel — click to join"}
      onClick={handleClick}
      aria-haspopup="listbox"
    >
      <span
        className="wm-channel-selector__dot"
        style={{ background: current?.color ?? "transparent" }}
        aria-hidden
      />
      <span className="wm-channel-selector__label">
        {current ? current.name : "No channel"}
      </span>
      <span className="wm-channel-selector__caret" aria-hidden>▾</span>
    </button>
  );
}

function detectCurrentOs(): OsKey {
  const ua = navigator.userAgent;
  if (/Mac|iPhone|iPad/i.test(ua)) return "macos";
  if (/Windows/i.test(ua)) return "windows";
  return "linux";
}

interface Props {
  /**
   * Open as a tab in the active Stack (auto-wraps if needed). `engineBinding`
   * is the engine the user picked from the app's directory entry; pass `null`
   * to let the WM use its own pinned engine.
   */
  onOpenTab: (
    appId: string,
    url: string,
    title: string,
    engineBinding: EngineBinding | null
  ) => void;
  /** Dashboard state + actions from the parent's useDashboards() call. */
  dashboards: UseDashboardsResult;
}

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

interface DownloadEvent {
  family: string;
  version: string;
  total?: number;
  downloaded?: number;
  message?: string;
  path?: string;
}

// ── Picker (engine choice) ───────────────────────────────────────────────────

interface EnginePickerProps {
  app: AppRecord;
  engines: EngineBinding[];
  onPick: (binding: EngineBinding) => void;
  onCancel: () => void;
}

function EnginePickerDialog({ app, engines, onPick, onCancel }: EnginePickerProps) {
  return (
    <div className="wm-engine-picker__backdrop" role="dialog" aria-modal="true">
      <div className="wm-engine-picker">
        <div className="wm-engine-picker__title">
          Launch <strong>{app.title ?? app.name}</strong> with
        </div>
        <ul className="wm-engine-picker__list">
          {engines.map((b) => (
            <li key={`${b.family}@${b.version}`}>
              <button type="button" className="wm-engine-picker__option" onClick={() => onPick(b)}>
                <span className="wm-engine-picker__family">{b.family}</span>
                <span className="wm-engine-picker__version">{b.version}</span>
              </button>
            </li>
          ))}
        </ul>
        <div className="wm-engine-picker__actions">
          <button type="button" className="wm-engine-picker__cancel" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Download confirmation + progress ─────────────────────────────────────────

interface DownloadPromptProps {
  binding: EngineBinding;
  label: string;
  sizeBytes: number;
  busy: boolean;
  progress: DownloadEvent | null;
  errorMessage: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}

function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function DownloadPromptDialog({
  binding,
  label,
  sizeBytes,
  busy,
  progress,
  errorMessage,
  onConfirm,
  onCancel,
}: DownloadPromptProps) {
  const downloaded = progress?.downloaded ?? 0;
  const total = progress?.total ?? sizeBytes;
  const pct = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;

  return (
    <div className="wm-engine-picker__backdrop" role="dialog" aria-modal="true">
      <div className="wm-engine-picker">
        <div className="wm-engine-picker__title">
          Download <strong>{label}</strong>?
        </div>
        <p className="wm-engine-picker__note">
          {binding.family}@{binding.version} isn&apos;t installed on this machine. The window
          manager needs to download <strong>{formatBytes(sizeBytes)}</strong> before it can launch a
          new external window with this engine.
        </p>

        {busy && (
          <div className="wm-engine-picker__progress" aria-live="polite">
            <div className="wm-engine-picker__progress-bar" style={{ width: `${pct}%` }} />
            <div className="wm-engine-picker__progress-text">
              {formatBytes(downloaded)} / {formatBytes(total)} ({pct}%)
            </div>
          </div>
        )}

        {errorMessage && (
          <div className="wm-engine-picker__error" role="alert">
            {errorMessage}
          </div>
        )}

        <div className="wm-engine-picker__actions">
          <button
            type="button"
            className="wm-engine-picker__cancel"
            onClick={onCancel}
            disabled={busy}
          >
            Cancel
          </button>
          <button
            type="button"
            className="wm-engine-picker__option"
            onClick={onConfirm}
            disabled={busy}
            style={{ flex: 0 }}
          >
            {busy ? "Downloading…" : "Download & launch"}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Component ────────────────────────────────────────────────────────────────

interface PendingLaunch {
  app: AppRecord;
  binding: EngineBinding;
}

interface DownloadPrompt {
  pending: PendingLaunch;
  label: string;
  sizeBytes: number;
}

export function Header({ onOpenTab, dashboards }: Props) {
  const [cfg, setCfg] = useState<TerminalConfig | null>(null);
  const [apps, setApps] = useState<AppRecord[]>([]);
  const [pickerApp, setPickerApp] = useState<AppRecord | null>(null);
  const [downloadPrompt, setDownloadPrompt] = useState<DownloadPrompt | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState<DownloadEvent | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);
  const [channelId, setChannelId] = useState<string | null>(null);
  const currentOs = useMemo(() => detectCurrentOs(), []);

  // Fetch config from Rust once on mount, then load the app directory.
  useEffect(() => {
    invoke<TerminalConfig>("wm_config")
      .then((c) => {
        setCfg(c);
        return fetch(c.appDirectoryUrl);
      })
      .then((r) => r.json())
      .then((d) => setApps(d.applications ?? []))
      .catch(() => {});
  }, []);

  // Hydrate FDC3 channel on mount and subscribe to changes.
  useEffect(() => {
    invoke<string | null>("wm_get_terminal_fdc3_channel")
      .then((id) => setChannelId(id ?? null))
      .catch(() => {});

    const unlisten = listen<{ channelId: string | null }>("wm:terminal-channel", (e) => {
      setChannelId(e.payload.channelId);
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const handleChannelPillClick = useCallback((rect: DOMRect) => {
    invoke("wm_channel_picker_open", {
      x: rect.left,
      y: rect.bottom + 4,
      channelId,
    }).catch(console.error);
  }, [channelId]);

  // ── Park panels while any picker / download dialog is open ──────────────
  //
  // Panel webviews sit above the chrome in z-order. Without this, the
  // dialogs render but every click lands on whichever panel covers the
  // dialog — looking like the WM has frozen. Parking moves panels offscreen
  // until the flow finishes.
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

  const launchApp = useCallback(
    (app: AppRecord, engineBinding: EngineBinding | null) => {
      onOpenTab(app.appId, app.details?.url ?? "", app.title ?? app.name, engineBinding);
    },
    [onOpenTab]
  );

  // Resolve `binding` then either launch (Ready) or show the download
  // confirmation dialog (NeedsDownload). Errors surface as a banner. The
  // picker / prompt state takes care of unparking once the user is done.
  const proceedLaunch = useCallback(
    async (pending: PendingLaunch) => {
      try {
        const status = await invoke<EngineStatus>("wm_engine_status", {
          binding: pending.binding,
        });
        if (status.status === "ready") {
          launchApp(pending.app, pending.binding);
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
    [launchApp]
  );

  const handleAppClick = useCallback(
    (app: AppRecord) => {
      setErrorMessage(null);
      const engines = enginesFor(app);
      if (engines.length === 0) {
        // No engine constraint — let the WM use its own pinned engine.
        launchApp(app, null);
        return;
      }
      if (engines.length === 1) {
        proceedLaunch({ app, binding: engines[0] }).catch(console.error);
        return;
      }
      setPickerApp(app);
    },
    [enginesFor, launchApp, proceedLaunch]
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
      launchApp(app, binding);
    } catch (e) {
      setDownloading(false);
      setErrorMessage(e instanceof Error ? e.message : String(e));
    }
  }, [downloadPrompt, launchApp]);

  const handleDownloadCancel = useCallback(() => {
    if (downloading) return;
    setDownloadPrompt(null);
    setDownloadProgress(null);
    setErrorMessage(null);
  }, [downloading]);

  // Tauri's automatic data-tauri-drag-region handler is only injected into
  // the primary webview, not child webviews. Our chrome is a child webview
  // (win.add_child), so the attribute alone doesn't drag — we have to call
  // startDragging() ourselves. The attribute serves as the opt-in marker.
  const handleHeaderPointerDown = useCallback((e: React.PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return;
    if (!(e.target as HTMLElement).hasAttribute("data-tauri-drag-region")) return;
    getCurrentWindow().startDragging().catch(console.error);
  }, []);

  const handleHeaderDoubleClick = useCallback((e: React.MouseEvent<HTMLElement>) => {
    if (!(e.target as HTMLElement).hasAttribute("data-tauri-drag-region")) return;
    getCurrentWindow().toggleMaximize().catch(console.error);
  }, []);

  const handleMinimize = useCallback(() => {
    getCurrentWindow().minimize().catch(console.error);
  }, []);

  const handleMaximize = useCallback(() => {
    getCurrentWindow().toggleMaximize().catch(console.error);
  }, []);

  const handleClose = useCallback(() => {
    getCurrentWindow().close().catch(console.error);
  }, []);

  return (
    <header
      className="wm-header"
      data-interactive
      data-tauri-drag-region
      onPointerDown={handleHeaderPointerDown}
      onDoubleClick={handleHeaderDoubleClick}
    >
      <span className="wm-header__brand" data-tauri-drag-region>
        {cfg?.title ?? "OneTerminal"}
      </span>

      <DashboardSwitcher ds={dashboards} />

      <ChannelSelector channelId={channelId} onPillClick={handleChannelPillClick} />

      <div className="wm-header__apps">
        {apps.map((app) => {
          const engineCount = enginesFor(app).length;
          const baseTitle = app.description ?? "Open as tab";
          const tooltip =
            engineCount > 1
              ? `${baseTitle} — choose browser engine (${engineCount} available)`
              : baseTitle;
          return (
            <button
              key={app.appId}
              className="wm-header__app-btn"
              title={tooltip}
              onClick={() => handleAppClick(app)}
            >
              {app.title ?? app.name}
              {engineCount > 1 && (
                <span className="wm-header__app-btn-badge" aria-hidden>
                  ▾
                </span>
              )}
            </button>
          );
        })}
      </div>

      {pickerApp && (
        <EnginePickerDialog
          app={pickerApp}
          engines={enginesFor(pickerApp)}
          onCancel={() => setPickerApp(null)}
          onPick={handlePickerPick}
        />
      )}

      {downloadPrompt && (
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
      )}

      {errorMessage && !downloadPrompt && (
        <div className="wm-header__error" role="alert">
          {errorMessage}
          <button
            type="button"
            className="wm-header__error-close"
            onClick={() => setErrorMessage(null)}
            aria-label="Dismiss"
          >
            ✕
          </button>
        </div>
      )}

      <div className="wm-header__controls">
        <button
          type="button"
          className="wm-header__control-btn"
          title="Minimize"
          aria-label="Minimize"
          onClick={handleMinimize}
        >
          &#x2012;
        </button>
        <button
          type="button"
          className="wm-header__control-btn"
          title="Maximize"
          aria-label="Maximize"
          onClick={handleMaximize}
        >
          &#x25A2;
        </button>
        <button
          type="button"
          className="wm-header__control-btn wm-header__control-btn--close"
          title="Close"
          aria-label="Close"
          onClick={handleClose}
        >
          &#x2715;
        </button>
      </div>
    </header>
  );
}
