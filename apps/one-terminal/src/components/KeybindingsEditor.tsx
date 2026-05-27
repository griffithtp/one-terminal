/**
 * KeybindingsEditor
 *
 * Self-contained editor for remappable command keybindings. Props-driven:
 * consumer supplies the commands snapshot + defaults map and receives
 * onAssign / onReset / onResetAll callbacks.
 *
 * This indirection lets the editor render inside the overlay webview (where
 * the local command registry is empty) by sourcing data from a snapshot
 * broadcast by the chrome webview — see `commands/keybindingsBridge.ts` for
 * the chrome side and `OverlayMenu`'s Shortcuts section for the consumer.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import type { SerializableCommand } from "../commands/registry";
import { normaliseCombo } from "../commands/keyboardListener";
import "./KeybindingsSettings.css";

// ── Helpers ───────────────────────────────────────────────────────────────────

const GROUP_LABELS: Record<string, string> = {
  navigation: "Navigation",
  widgets: "Widgets",
  settings: "Settings",
};

const MODIFIER_KEYS = new Set(["Meta", "Control", "Shift", "Alt"]);

const isMac = /Mac|iPhone|iPad/i.test(navigator.userAgent);

/** Pretty-prints a normalised keybinding string for display. */
function formatKeybinding(kb: string): string {
  return kb
    .split("+")
    .map((part) => {
      if (part === "CmdOrCtrl") return isMac ? "⌘" : "Ctrl";
      if (part === "Ctrl") return isMac ? "⌃" : "Ctrl";
      if (part === "Shift") return isMac ? "⇧" : "Shift";
      if (part === "Alt") return isMac ? "⌥" : "Alt";
      return part;
    })
    .join(isMac ? "" : "+");
}

// Only show remappable static command groups — instance and app commands are
// dynamic and don't benefit from persistent keybinding overrides.
const REMAPPABLE_GROUPS = new Set(["widgets", "navigation", "settings"]);

// ── Capture cell ──────────────────────────────────────────────────────────────

interface CaptureCellProps {
  pendingCombo: string;
  conflict: string | null;
  onKeyDown: (e: React.KeyboardEvent<HTMLDivElement>) => void;
  onAssign: () => void;
  onCancel: () => void;
}

function CaptureCell({ pendingCombo, conflict, onKeyDown, onAssign, onCancel }: CaptureCellProps) {
  const divRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    divRef.current?.focus();
  }, []);

  return (
    <div className="kb-capture">
      <div className="kb-capture__input">
        <div
          ref={divRef}
          className={`kb-capture__field${pendingCombo ? "" : " kb-capture__field--placeholder"}`}
          tabIndex={0}
          onKeyDown={onKeyDown}
          role="textbox"
          aria-label="Press a key combination"
        >
          {pendingCombo ? formatKeybinding(pendingCombo) : "Press a key combo…"}
        </div>
        <button className="kb-btn kb-btn--primary" onClick={onAssign} disabled={!pendingCombo}>
          Assign
        </button>
        <button className="kb-btn" onClick={onCancel}>
          Cancel
        </button>
      </div>
      {conflict && (
        <div className="kb-capture__conflict">
          Already used by: <strong>{conflict}</strong> — assigning will remove it there.
        </div>
      )}
    </div>
  );
}

// ── Binding display ───────────────────────────────────────────────────────────

function BindingDisplay({ keybinding }: { keybinding: string | undefined }) {
  if (!keybinding) return <span className="kb-binding--none">—</span>;
  return (
    <span className="kb-binding">
      <kbd className="kb-binding__key">{formatKeybinding(keybinding)}</kbd>
    </span>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

interface Props {
  /** Snapshot of all commands from the chrome-side registry. */
  commands: SerializableCommand[];
  /** Default keybinding per command id (undefined / null when no default). */
  defaults: Record<string, string | null>;
  onAssign: (id: string, combo: string) => void;
  onReset: (id: string) => void;
  onResetAll: () => void;
}

export function KeybindingsEditor({ commands, defaults, onAssign, onReset, onResetAll }: Props) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [pendingCombo, setPendingCombo] = useState("");
  const [conflict, setConflict] = useState<string | null>(null);

  // Filter to remappable groups. Memoise-free is fine — list is small (~20).
  const filtered = commands.filter((c) => REMAPPABLE_GROUPS.has(c.group));

  // Snapshot might refresh while editing (e.g. chrome re-emits after an
  // assign). If the row we're editing is gone, drop edit state.
  useEffect(() => {
    if (editingId && !commands.find((c) => c.id === editingId)) {
      setEditingId(null);
      setPendingCombo("");
      setConflict(null);
    }
  }, [commands, editingId]);

  const commitAssign = useCallback(() => {
    if (!editingId || !pendingCombo) return;
    onAssign(editingId, pendingCombo);
    setEditingId(null);
    setPendingCombo("");
    setConflict(null);
  }, [editingId, pendingCombo, onAssign]);

  const handleCaptureKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === "Escape") {
        setEditingId(null);
        setPendingCombo("");
        setConflict(null);
        return;
      }
      if (e.key === "Enter" && pendingCombo) {
        commitAssign();
        return;
      }
      if (MODIFIER_KEYS.has(e.key)) return;

      const combo = normaliseCombo(e);
      setPendingCombo(combo);

      // Conflict detection: search the snapshot for an existing assignment.
      const existing = commands.find((c) => c.keybinding === combo && c.id !== editingId);
      setConflict(existing ? existing.label : null);
    },
    [editingId, pendingCombo, commands, commitAssign]
  );

  return (
    <div className="kb-editor">
      <div className="kb-editor__toolbar">
        <button type="button" className="kb-header__reset-all" onClick={onResetAll}>
          Reset all
        </button>
      </div>

      <div className="kb-table-wrap">
        <table className="kb-table">
          <thead>
            <tr>
              <th className="kb-col-label">Command</th>
              <th className="kb-col-group">Group</th>
              <th className="kb-col-binding">Shortcut</th>
              <th className="kb-col-actions" />
            </tr>
          </thead>
          <tbody>
            {filtered.map((cmd) => {
              const isEditing = editingId === cmd.id;
              const defaultKb = defaults[cmd.id] ?? undefined;
              const isModified = cmd.keybinding !== defaultKb;

              return (
                <tr key={cmd.id} className={isEditing ? "kb-row--editing" : ""}>
                  <td className="kb-col-label">{cmd.label}</td>
                  <td className="kb-col-group">{GROUP_LABELS[cmd.group] ?? cmd.group}</td>
                  <td className="kb-col-binding">
                    {isEditing ? (
                      <CaptureCell
                        pendingCombo={pendingCombo}
                        conflict={conflict}
                        onKeyDown={handleCaptureKeyDown}
                        onAssign={commitAssign}
                        onCancel={() => {
                          setEditingId(null);
                          setPendingCombo("");
                          setConflict(null);
                        }}
                      />
                    ) : (
                      <BindingDisplay keybinding={cmd.keybinding} />
                    )}
                  </td>
                  <td className="kb-col-actions">
                    {!isEditing && (
                      <div className="kb-actions">
                        <button
                          className="kb-btn"
                          onClick={() => {
                            setEditingId(cmd.id);
                            setPendingCombo("");
                            setConflict(null);
                          }}
                        >
                          Edit
                        </button>
                        {isModified && (
                          <button
                            className="kb-btn kb-btn--danger"
                            onClick={() => onReset(cmd.id)}
                            title={`Restore default: ${defaultKb ?? "none"}`}
                          >
                            Reset
                          </button>
                        )}
                      </div>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
