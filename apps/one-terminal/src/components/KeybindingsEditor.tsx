/**
 * KeybindingsEditor
 *
 * Self-contained editor for remappable command keybindings. Renders a
 * toolbar with "Reset all" + a table of every command in REMAPPABLE_GROUPS
 * with inline capture / assign / reset row controls.
 *
 * Pure body — no modal backdrop, no own close button, no panel parking.
 * Use `KeybindingsSettings` for the modal version (palette deep-link) or
 * `KeybindingsSection` to embed inside the App Menu drawer.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { registry } from "../commands/registry";
import type { Command } from "../commands/registry";
import {
  getDefaultKeybinding,
  loadKeybindings,
  saveKeybindings,
} from "../commands/keybindingStore";
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
  const parts = formatKeybinding(keybinding);
  return (
    <span className="kb-binding">
      <kbd className="kb-binding__key">{parts}</kbd>
    </span>
  );
}

// ── Main component ────────────────────────────────────────────────────────────

// Only show remappable static command groups — instance and app commands are
// dynamic and don't benefit from persistent keybinding overrides.
const REMAPPABLE_GROUPS = new Set(["widgets", "navigation", "settings"]);

export function KeybindingsEditor() {
  const [commands, setCommands] = useState<Command[]>([]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [pendingCombo, setPendingCombo] = useState("");
  const [conflict, setConflict] = useState<string | null>(null);

  const refresh = useCallback(() => {
    setCommands(registry.getAll().filter((c) => REMAPPABLE_GROUPS.has(c.group)));
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // ── Capture keydown ─────────────────────────────────────────────────────

  const handleCaptureKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      e.preventDefault();
      e.stopPropagation();

      if (e.key === "Escape") {
        setEditingId(null);
        return;
      }
      if (e.key === "Enter" && pendingCombo) {
        commitAssign();
        return;
      }
      // Ignore modifier-only presses — wait for a non-modifier key.
      if (MODIFIER_KEYS.has(e.key)) return;

      const combo = normaliseCombo(e);
      setPendingCombo(combo);

      const existing = registry.findByKeybinding(combo);
      setConflict(existing && existing.id !== editingId ? existing.label : null);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [editingId, pendingCombo]
  );

  // ── Assign ──────────────────────────────────────────────────────────────

  function commitAssign() {
    if (!editingId || !pendingCombo) return;

    // If a conflict exists, unassign the other command's keybinding first.
    const conflicting = registry.findByKeybinding(pendingCombo);
    if (conflicting && conflicting.id !== editingId) {
      registry.register({ ...conflicting, keybinding: undefined });
      const overrides = loadKeybindings();
      if (getDefaultKeybinding(conflicting.id) === undefined) {
        delete overrides[conflicting.id];
      } else {
        // Mark it as explicitly unbound so applyOverrides doesn't restore it.
        overrides[conflicting.id] = "";
      }
      saveKeybindings(overrides);
    }

    // Register the new keybinding.
    const cmd = registry.getAll().find((c) => c.id === editingId);
    if (cmd) registry.register({ ...cmd, keybinding: pendingCombo });

    const overrides = loadKeybindings();
    overrides[editingId] = pendingCombo;
    saveKeybindings(overrides);

    setEditingId(null);
    refresh();
  }

  // ── Reset single ────────────────────────────────────────────────────────

  function resetOne(id: string) {
    const defaultKb = getDefaultKeybinding(id);
    const cmd = registry.getAll().find((c) => c.id === id);
    if (cmd) registry.register({ ...cmd, keybinding: defaultKb });

    const overrides = loadKeybindings();
    delete overrides[id];
    saveKeybindings(overrides);

    refresh();
  }

  // ── Reset all ───────────────────────────────────────────────────────────

  function resetAll() {
    for (const cmd of registry.getAll()) {
      if (!REMAPPABLE_GROUPS.has(cmd.group)) continue;
      const defaultKb = getDefaultKeybinding(cmd.id);
      registry.register({ ...cmd, keybinding: defaultKb });
    }
    saveKeybindings({});
    setEditingId(null);
    refresh();
  }

  // ── Render ──────────────────────────────────────────────────────────────

  return (
    <div className="kb-editor">
      <div className="kb-editor__toolbar">
        <button type="button" className="kb-header__reset-all" onClick={resetAll}>
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
            {commands.map((cmd) => {
              const isEditing = editingId === cmd.id;
              const defaultKb = getDefaultKeybinding(cmd.id);
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
                        onCancel={() => setEditingId(null)}
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
                            onClick={() => resetOne(cmd.id)}
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
