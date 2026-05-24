/**
 * Header
 *
 * 40 px toolbar that sits at the top of the chrome webview.
 *
 * The widget launch flow (app directory fetch, engine picker, download
 * confirmation) is owned by `useAppLaunch` in the parent. Header receives
 * the resolved `apps` list plus `enginesFor` / `onAppClick` and just
 * renders the launch buttons — clicks delegate back to the hook.
 *
 * The error banner mirrors the hook's `errorMessage`; clearing dismisses
 * via `onClearError`.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppRecord, EngineBinding } from "../types";
import { DashboardSwitcher } from "./DashboardSwitcher";
import type { UseDashboardsResult } from "../hooks/useDashboards";
import { getTerminalConfig } from "../lib/terminalConfig";

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
      title={
        current ? `FDC3: ${current.name} — click to change` : "FDC3: No channel — click to join"
      }
      onClick={handleClick}
      aria-haspopup="listbox"
    >
      <span
        className="wm-channel-selector__dot"
        style={{ background: current?.color ?? "transparent" }}
        aria-hidden
      />
      <span className="wm-channel-selector__label">{current ? current.name : "No channel"}</span>
      <span className="wm-channel-selector__caret" aria-hidden>
        ▾
      </span>
    </button>
  );
}

interface Props {
  /** Apps registered in the App Directory (resolved by useAppLaunch). */
  apps: AppRecord[];
  /** Engine bindings declared by an app for the current OS. */
  enginesFor: (app: AppRecord) => EngineBinding[];
  /** Launches the app — picker / download flow runs inside useAppLaunch. */
  onAppClick: (app: AppRecord) => void;
  /** Most recent launch error from useAppLaunch (null when clean). */
  errorMessage: string | null;
  /** Dismiss the inline error banner. */
  onClearError: () => void;
  /** Dashboard state + actions from the parent's useDashboards() call. */
  dashboards: UseDashboardsResult;
  /** Open / close the App Menu drawer (rendered by the parent). */
  onMenuToggle: () => void;
}

export function Header({
  apps,
  enginesFor,
  onAppClick,
  errorMessage,
  onClearError,
  dashboards,
  onMenuToggle,
}: Props) {
  const [channelId, setChannelId] = useState<string | null>(null);
  const [terminalTitle, setTerminalTitle] = useState<string>("OneTerminal");

  // Hydrate terminal title once for the brand label.
  useEffect(() => {
    getTerminalConfig()
      .then((c) => {
        if (c.title) setTerminalTitle(c.title);
      })
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

  const handleChannelPillClick = useCallback(
    (rect: DOMRect) => {
      invoke("wm_channel_picker_open", {
        x: rect.left,
        y: rect.bottom + 4,
        channelId,
      }).catch(console.error);
    },
    [channelId]
  );

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
      <button
        type="button"
        className="wm-header__brand-btn"
        onClick={onMenuToggle}
        title="Open menu"
        aria-label="Open menu"
        aria-haspopup="menu"
      >
        <span className="wm-header__brand-icon" aria-hidden>
          ☰
        </span>
        <span className="wm-header__brand-label">{terminalTitle}</span>
      </button>

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
              onClick={() => onAppClick(app)}
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

      {errorMessage && (
        <div className="wm-header__error" role="alert">
          {errorMessage}
          <button
            type="button"
            className="wm-header__error-close"
            onClick={onClearError}
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
