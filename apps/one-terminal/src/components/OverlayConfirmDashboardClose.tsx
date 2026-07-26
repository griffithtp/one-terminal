/**
 * OverlayConfirmDashboardClose
 *
 * Renders the "close dashboard" confirm dialog in the overlay webview, for
 * the same reason OverlayConfirmDashboardSwitch does (widgets stay visible
 * behind it). Triggered by `wm:dashboard-confirm-close`, which fires from
 * the dashboard tab context menu's "Close dashboard" item
 * (`OverlayDashboardTabMenu`) when `wm_close_dashboard` returns
 * NeedsConfirm — either the dashboard has unsaved changes (auto-save off)
 * or it owns keep-alive widgets currently parked in the background.
 *
 * Unlike `OverlayConfirmDashboardDelete`, closing doesn't delete the
 * dashboard's data — it stays reopenable from the Manage drawer via
 * `wm_reopen_dashboard`. Only the *live, unsaved* state (if any) is what's
 * actually at risk here, which is why this dialog still offers Save/Discard.
 */

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Payload {
  name: string;
  dirty: boolean;
}

export function OverlayConfirmDashboardClose() {
  const [payload, setPayload] = useState<Payload | null>(null);

  useEffect(() => {
    const promise = listen<Payload>("wm:dashboard-confirm-close", (e) => {
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

  const closeAnyway = useCallback(async () => {
    if (!payload) return;
    const name = payload.name;
    dismiss();
    try {
      await invoke("wm_close_dashboard", { name, force: true });
    } catch (e) {
      console.error("[confirm-close] close:", e);
    }
  }, [payload, dismiss]);

  const saveAndClose = useCallback(async () => {
    if (!payload) return;
    const name = payload.name;
    dismiss();
    try {
      await invoke("wm_save_dashboard");
      await invoke("wm_close_dashboard", { name, force: true });
    } catch (e) {
      console.error("[confirm-close] save:", e);
    }
  }, [payload, dismiss]);

  const discardAndClose = useCallback(async () => {
    if (!payload) return;
    const name = payload.name;
    dismiss();
    try {
      await invoke("wm_discard_dashboard");
      await invoke("wm_close_dashboard", { name, force: true });
    } catch (e) {
      console.error("[confirm-close] discard:", e);
    }
  }, [payload, dismiss]);

  if (!payload) return null;

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">Close Dashboard</div>
        {payload.dirty ? (
          <p className="wm-ds-dialog__body">
            <strong>&ldquo;{payload.name}&rdquo;</strong> has unsaved layout changes. Save before
            closing it or discard them? You can reopen it later from the Manage drawer.
          </p>
        ) : (
          <p className="wm-ds-dialog__body">
            <strong>&ldquo;{payload.name}&rdquo;</strong> has widgets running in the background.
            Closing it will stop them — you can reopen it later from the Manage drawer.
          </p>
        )}
        <div className="wm-ds-dialog__actions">
          <button type="button" className="wm-ds-dialog__btn" onClick={dismiss}>
            Cancel
          </button>
          {payload.dirty ? (
            <>
              <button
                type="button"
                className="wm-ds-dialog__btn wm-ds-dialog__btn--danger"
                onClick={discardAndClose}
              >
                Discard &amp; Close
              </button>
              <button
                type="button"
                className="wm-ds-dialog__btn wm-ds-dialog__btn--primary"
                onClick={saveAndClose}
              >
                Save &amp; Close
              </button>
            </>
          ) : (
            <button
              type="button"
              className="wm-ds-dialog__btn wm-ds-dialog__btn--danger"
              onClick={closeAnyway}
            >
              Close Anyway
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
