/**
 * OverlayCreateDashboard
 *
 * "New dashboard" prompt rendered in the overlay webview (so widgets stay
 * visible behind the backdrop). Triggered by `wm:dashboard-create-open`
 * which the header's "+" button and the `dashboard:create` palette command
 * both fire via `wm_dashboard_create_open`.
 *
 * On submit, invokes the global Rust dashboard commands directly:
 *   - `wm_create_dashboard` creates the entry
 *   - `wm_switch_dashboard` activates it (mirrors the old chrome behaviour
 *     where the new dashboard immediately becomes active)
 *
 * Dismiss parks the overlay via the standard `wm_ctx_menu_close`.
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export function OverlayCreateDashboard() {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  // Listen for chrome's `wm_dashboard_create_open` invoke.
  useEffect(() => {
    const promise = listen("wm:dashboard-create-open", () => {
      setName("");
      setOpen(true);
    });
    return () => {
      promise.then((fn) => fn()).catch(() => {});
    };
  }, []);

  // Auto-focus the input when the dialog appears.
  useEffect(() => {
    if (open) {
      // Defer to next frame so the input is mounted.
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  const dismiss = useCallback(() => {
    setOpen(false);
    invoke("wm_ctx_menu_close").catch(console.error);
  }, []);

  const handleConfirm = useCallback(async () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    dismiss();
    try {
      await invoke("wm_create_dashboard", { name: trimmed });
      await invoke("wm_switch_dashboard", { name: trimmed });
    } catch (e) {
      console.error("[create-dashboard]", e);
    }
  }, [name, dismiss]);

  const handleKeyDown = useCallback(
    (e: ReactKeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter") handleConfirm();
      if (e.key === "Escape") dismiss();
    },
    [handleConfirm, dismiss]
  );

  if (!open) return null;

  const canSubmit = name.trim().length > 0;

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true" onClick={dismiss}>
      <div className="wm-ds-dialog" onClick={(e) => e.stopPropagation()}>
        <div className="wm-ds-dialog__title">New Dashboard</div>
        <input
          ref={inputRef}
          className="wm-ds-dialog__input"
          placeholder="Dashboard name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <div className="wm-ds-dialog__actions">
          <button type="button" className="wm-ds-dialog__btn" onClick={dismiss}>
            Cancel
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--primary"
            disabled={!canSubmit}
            onClick={handleConfirm}
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
