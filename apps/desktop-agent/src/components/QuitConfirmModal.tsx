import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

/**
 * Shown when the tray "Exit" action emits `cda:quit-requested`.
 * Lets the user choose to save Terminal state (default) or discard it before
 * the Desktop Agent process exits.
 */
export function QuitConfirmModal() {
  const [visible, setVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen("cda:quit-requested", () => {
      setVisible(true);
      setBusy(false);
      setError(null);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!visible) return null;

  const handleSave = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke("cda_quit");
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const handleDiscard = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke("cda_quit_discard");
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const handleCancel = () => {
    setVisible(false);
  };

  return (
    <div className="qcm__overlay" role="dialog" aria-modal="true">
      <div className="qcm__dialog">
        <header className="qcm__header">
          <span className="qcm__title">Quit Desktop Agent</span>
        </header>

        <div className="qcm__body">
          <p className="qcm__message">
            Do you want to save your open Terminal windows before quitting?
          </p>
          <p className="qcm__detail">
            <strong>Save</strong> — Terminal windows will be restored on the next launch.
            <br />
            <strong>{"Don't Save"}</strong> — All Terminal state is deleted. Next launch starts with
            no open windows.
          </p>
          {error && <div className="qcm__error">{error}</div>}
        </div>

        <footer className="qcm__footer">
          <button className="qcm__cancel-btn" onClick={handleCancel} disabled={busy}>
            Cancel
          </button>
          <button className="qcm__discard-btn" onClick={handleDiscard} disabled={busy}>
            {busy ? "Quitting…" : "Don't Save"}
          </button>
          <button className="qcm__save-btn" onClick={handleSave} disabled={busy} autoFocus>
            {busy ? "Quitting…" : "Save & Quit"}
          </button>
        </footer>
      </div>
    </div>
  );
}
