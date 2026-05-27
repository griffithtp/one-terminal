/**
 * OverlayConfirmDashboardSwitch
 *
 * Renders the unsaved-changes confirm dialog in the overlay webview so
 * widgets remain visible behind it (the chrome modal version would park
 * panels). Triggered by `wm:dashboard-confirm-switch` event, which fires
 * from `useDashboards.switchTo` when `wm_switch_dashboard` returns
 * NeedsConfirm.
 *
 * Actions invoke the global Rust commands directly (no hook plumbing
 * required) and dismiss the overlay via `wm_ctx_menu_close`.
 */

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Payload {
  activeName: string;
  pendingName: string;
}

export function OverlayConfirmDashboardSwitch() {
  const [payload, setPayload] = useState<Payload | null>(null);

  useEffect(() => {
    const promise = listen<Payload>("wm:dashboard-confirm-switch", (e) => {
      setPayload(e.payload);
    });
    return () => {
      promise.then((fn) => fn()).catch(() => {});
    };
  }, []);

  const dismiss = useCallback(() => {
    setPayload(null);
    invoke("wm_ctx_menu_close").catch(console.error);
  }, []);

  const handleSave = useCallback(async () => {
    if (!payload) return;
    const target = payload.pendingName;
    dismiss();
    try {
      await invoke("wm_save_dashboard");
      await invoke("wm_switch_dashboard", { name: target });
    } catch (e) {
      console.error("[confirm-switch] save:", e);
    }
  }, [payload, dismiss]);

  const handleDiscard = useCallback(async () => {
    if (!payload) return;
    const target = payload.pendingName;
    dismiss();
    try {
      await invoke("wm_discard_dashboard");
      await invoke("wm_switch_dashboard", { name: target });
    } catch (e) {
      console.error("[confirm-switch] discard:", e);
    }
  }, [payload, dismiss]);

  if (!payload) return null;

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">Unsaved Changes</div>
        <p className="wm-ds-dialog__body">
          <strong>&ldquo;{payload.activeName}&rdquo;</strong> has unsaved layout changes. Save
          before switching to <strong>&ldquo;{payload.pendingName}&rdquo;</strong> or discard
          them?
        </p>
        <div className="wm-ds-dialog__actions">
          <button type="button" className="wm-ds-dialog__btn" onClick={dismiss}>
            Cancel
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--danger"
            onClick={handleDiscard}
          >
            Discard
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--primary"
            onClick={handleSave}
          >
            Save
          </button>
        </div>
      </div>
    </div>
  );
}
