/**
 * KeybindingsSection
 *
 * Drawer section for keyboard shortcuts. Renders the full editor inside
 * the overlay webview using a snapshot of chrome's command registry —
 * each edit is sent to chrome as an event, which applies it and emits a
 * fresh snapshot back. See `commands/keybindingsBridge.ts` for the chrome
 * side.
 *
 * Snapshot lifecycle:
 *   - Chrome publishes via `publishKeybindingsSnapshot()` when the drawer
 *     opens (chrome's `openMenuAt`) and after each successful edit.
 *   - This section also requests a snapshot on mount as a safety net.
 */

import { useEffect, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { KeybindingsEditor } from "../KeybindingsEditor";
import {
  KEYBINDINGS_REQUEST_EVENT,
  KEYBINDINGS_SNAPSHOT_EVENT,
  KEYBINDING_SET_EVENT,
  KEYBINDING_RESET_EVENT,
  KEYBINDING_RESET_ALL_EVENT,
  type KeybindingsSnapshotPayload,
} from "../../commands/keybindingsBridge";
import "./KeybindingsSection.css";

export function KeybindingsSection() {
  const [snapshot, setSnapshot] = useState<KeybindingsSnapshotPayload | null>(null);

  useEffect(() => {
    let cancelled = false;
    const unlistenPromise = listen<KeybindingsSnapshotPayload>(KEYBINDINGS_SNAPSHOT_EVENT, (e) => {
      if (!cancelled) setSnapshot(e.payload);
    });
    // Ask chrome for a fresh snapshot now that we're mounted and listening.
    // The snapshot bridge in chrome replies by re-publishing.
    emit(KEYBINDINGS_REQUEST_EVENT).catch(console.error);
    return () => {
      cancelled = true;
      unlistenPromise.then((fn) => fn()).catch(() => {});
    };
  }, []);

  return (
    <div className="ot-shortcuts">
      <header className="ot-shortcuts__head">
        <h2 className="ot-shortcuts__title">Keyboard Shortcuts</h2>
        <p className="ot-shortcuts__subtitle">
          Click <em>Edit</em> on a row, press a key combo, then <em>Assign</em>. Changes apply
          immediately and persist across restarts.
        </p>
      </header>
      <div className="ot-shortcuts__editor">
        {snapshot === null ? (
          <div className="ot-shortcuts__loading">Loading shortcuts…</div>
        ) : (
          // SerializableCommand has additional optional fields (shortcut,
          // widgetLabel) the bridge omits — the editor only reads the
          // fields present in the snapshot, so this widening is safe.
          <KeybindingsEditor
            commands={snapshot.commands as never}
            defaults={snapshot.defaults}
            onAssign={(id, combo) => emit(KEYBINDING_SET_EVENT, { id, combo }).catch(console.error)}
            onReset={(id) => emit(KEYBINDING_RESET_EVENT, { id }).catch(console.error)}
            onResetAll={() => emit(KEYBINDING_RESET_ALL_EVENT).catch(console.error)}
          />
        )}
      </div>
    </div>
  );
}
