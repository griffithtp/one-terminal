/**
 * OverlayConfirmDashboardDelete
 *
 * Renders the "delete dashboard" (permanent) confirm dialog in the overlay
 * webview, for the same reason OverlayConfirmDashboardSwitch does (widgets
 * stay visible behind it). Triggered by `wm:dashboard-confirm-delete`, which
 * fires from `useDashboards.remove` — the Manage drawer's "Delete" button,
 * only ever shown for already-closed dashboards — when `wm_delete_dashboard`
 * returns NeedsConfirm. That now happens unconditionally (deletion is
 * irreversible), so unlike the close dialog there's no dirty/keep-alive
 * branching here: a closed dashboard can be neither dirty nor own parked
 * panels, so there's nothing to offer Save/Discard for.
 *
 * See `OverlayConfirmDashboardClose` for the non-destructive "close"
 * variant, which does still branch on those.
 */

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface Payload {
  name: string;
}

export function OverlayConfirmDashboardDelete() {
  const [payload, setPayload] = useState<Payload | null>(null);

  useEffect(() => {
    const promise = listen<Payload>("wm:dashboard-confirm-delete", (e) => {
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

  const confirmDelete = useCallback(async () => {
    if (!payload) return;
    const name = payload.name;
    dismiss();
    try {
      await invoke("wm_delete_dashboard", { name, force: true });
    } catch (e) {
      console.error("[confirm-delete] delete:", e);
    }
  }, [payload, dismiss]);

  if (!payload) return null;

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">Delete Dashboard</div>
        <p className="wm-ds-dialog__body">
          Permanently delete <strong>&ldquo;{payload.name}&rdquo;</strong>? This can&rsquo;t be
          undone.
        </p>
        <div className="wm-ds-dialog__actions">
          <button type="button" className="wm-ds-dialog__btn" onClick={dismiss}>
            Cancel
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--danger"
            onClick={confirmDelete}
          >
            Delete
          </button>
        </div>
      </div>
    </div>
  );
}
