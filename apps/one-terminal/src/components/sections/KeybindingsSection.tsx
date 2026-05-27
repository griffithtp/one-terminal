/**
 * KeybindingsSection
 *
 * Drawer section for keyboard shortcuts. Rendered inside the overlay
 * webview, which has its own (empty) command registry — embedding the
 * editor directly would show no commands. Until a registry IPC bridge
 * exists (tracked separately), this section is a launcher that opens the
 * existing chrome-side `KeybindingsSettings` modal via the
 * `settings.keybindings` command.
 *
 * Dispatch path: the overlay emits `wm:palette-execute` with the command
 * id, which the chrome already listens for (App.tsx → registry.execute).
 * That action toggles `settingsOpen` on, mounting the chrome modal. Panel
 * parking for the modal is handled by chrome's existing useEffect.
 */

import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import "./KeybindingsSection.css";

export function KeybindingsSection() {
  function openEditor() {
    // Dismiss the overlay (closes the drawer) so the chrome modal becomes
    // visible — without this, chrome's modal would render below panels and
    // chrome's existing park-on-open useEffect would handle visibility.
    invoke("wm_ctx_menu_close").catch(console.error);
    emit("wm:menu-closed").catch(console.error);
    emit("wm:palette-execute", "settings.keybindings").catch(console.error);
  }

  return (
    <div className="ot-shortcuts">
      <header className="ot-shortcuts__head">
        <h2 className="ot-shortcuts__title">Keyboard Shortcuts</h2>
        <p className="ot-shortcuts__subtitle">
          Remap shortcuts in the dedicated editor. Captured combos override defaults and
          persist across restarts.
        </p>
      </header>
      <div className="ot-shortcuts__editor">
        <div className="ot-shortcuts__launch">
          <button type="button" className="ot-shortcuts__launch-btn" onClick={openEditor}>
            Open shortcuts editor
          </button>
          <p className="ot-shortcuts__launch-hint">
            Opens the full editor as a modal. The drawer will close.
          </p>
        </div>
      </div>
    </div>
  );
}
