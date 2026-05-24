/**
 * KeybindingsSection
 *
 * App Menu drawer section that embeds `KeybindingsEditor` inline. The
 * editor handles all capture / assign / reset state; this wrapper just
 * provides the section title chrome.
 */

import { KeybindingsEditor } from "../KeybindingsEditor";
import "./KeybindingsSection.css";

export function KeybindingsSection() {
  return (
    <div className="ot-shortcuts">
      <header className="ot-shortcuts__head">
        <h2 className="ot-shortcuts__title">Keyboard Shortcuts</h2>
        <p className="ot-shortcuts__subtitle">
          Click <em>Edit</em> on a row, press a key combo, then <em>Assign</em>.
        </p>
      </header>
      <div className="ot-shortcuts__editor">
        <KeybindingsEditor />
      </div>
    </div>
  );
}
