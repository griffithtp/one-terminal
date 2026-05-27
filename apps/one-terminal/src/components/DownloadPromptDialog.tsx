/**
 * DownloadPromptDialog
 *
 * Modal shown when the selected engine isn't installed locally yet. Confirms
 * the download, then shows a live progress bar while the engine downloads.
 * State is owned by `useAppLaunch`; this component is pure presentational.
 */

import type { EngineBinding } from "../types";

export interface DownloadEvent {
  family: string;
  version: string;
  total?: number;
  downloaded?: number;
  message?: string;
  path?: string;
}

interface Props {
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

export function DownloadPromptDialog({
  binding,
  label,
  sizeBytes,
  busy,
  progress,
  errorMessage,
  onConfirm,
  onCancel,
}: Props) {
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
