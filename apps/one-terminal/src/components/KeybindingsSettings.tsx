/**
 * KeybindingsSettings
 *
 * Modal wrapper around `KeybindingsEditor`. Used by the `settings.keybindings`
 * command (typically reached via the command palette) — backdrop + centred
 * panel with a close button. The drawer's Shortcuts section embeds the
 * editor directly without this shell.
 */

import { KeybindingsEditor } from "./KeybindingsEditor";
import "./KeybindingsSettings.css";

interface Props {
  onClose: () => void;
}

export function KeybindingsSettings({ onClose }: Props) {
  return (
    <div className="kb-backdrop" onClick={onClose}>
      <div className="kb-panel" onClick={(e) => e.stopPropagation()}>
        <div className="kb-header">
          <span className="kb-header__title">Keyboard Shortcuts</span>
          <button className="kb-header__close" onClick={onClose} aria-label="Close">
            ✕
          </button>
        </div>
        <KeybindingsEditor />
      </div>
    </div>
  );
}
