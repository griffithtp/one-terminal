import { useEffect, useMemo, useRef, useState } from "react";
import { registry } from "../commands/registry";
import type { Command } from "../commands/registry";
import type { HostLayout, LayoutSnapshot } from "../types";
import "./CommandPalette.css";

// ── Group display labels ───────────────────────────────────────────────────

const GROUP_LABELS: Record<string, string> = {
  navigation: "Navigation",
  widgets: "Widgets",
  settings: "Settings",
  apps: "Apps",
  "widget-instances": "Open Widgets",
};

// ── Highlight ring ─────────────────────────────────────────────────────────

interface HighlightRingProps {
  layout: LayoutSnapshot | null;
  hostLayout: HostLayout | null;
  widgetLabel: string | null;
}

export function PanelHighlightLayer({ layout, hostLayout, widgetLabel }: HighlightRingProps) {
  if (!widgetLabel) return null;

  // Non-stacked leaf panels have full rects in layout.panels.
  const panel = layout?.panels.find((p) => p.id === widgetLabel);
  if (panel) {
    return (
      <div
        className="wm-panel-highlight"
        style={{ left: panel.x, top: panel.y, width: panel.width, height: panel.height }}
      />
    );
  }

  // Stacked panels: highlight the entire stack rect.
  const stack = hostLayout?.stacks.find((s) => s.tabs.some((t) => t.label === widgetLabel));
  if (stack) {
    return (
      <div
        className="wm-panel-highlight"
        style={{ left: stack.x, top: stack.y, width: stack.width, height: stack.height }}
      />
    );
  }

  return null;
}

// ── Palette ────────────────────────────────────────────────────────────────

interface Props {
  isOpen: boolean;
  onClose: () => void;
  onHighlight: (widgetLabel: string | null) => void;
}

export function CommandPalette({ isOpen, onClose, onHighlight }: Props) {
  const [query, setQuery] = useState("");
  const [selectedIdx, setSelectedIdx] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  const results = useMemo(() => registry.search(query), [query]);

  // Drive the widget highlight ring whenever the selected result changes.
  useEffect(() => {
    onHighlight(results[selectedIdx]?.widgetLabel ?? null);
  }, [selectedIdx, results, onHighlight]);

  // Reset and focus when palette opens.
  useEffect(() => {
    if (isOpen) {
      setQuery("");
      setSelectedIdx(0);
      // rAF ensures the input is in the DOM before focusing.
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [isOpen]);

  function handleKeyDown(e: React.KeyboardEvent) {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setSelectedIdx((i) => Math.min(i + 1, results.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setSelectedIdx((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        execute(results[selectedIdx]);
        break;
      case "Escape":
        onClose();
        break;
    }
    // Prevent keydown from leaking to the keyboard listener (06-C).
    e.stopPropagation();
  }

  function execute(cmd?: Command) {
    if (!cmd) return;
    onHighlight(null);
    onClose();
    registry.execute(cmd.id).catch(console.error);
  }

  if (!isOpen) return null;

  return (
    <div className="palette-backdrop" onClick={onClose}>
      <div className="palette-panel" onClick={(e) => e.stopPropagation()}>
        <input
          ref={inputRef}
          className="palette-input"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setSelectedIdx(0);
          }}
          onKeyDown={handleKeyDown}
          placeholder="Type a command…"
          spellCheck={false}
          aria-label="Command palette search"
          role="combobox"
          aria-expanded={results.length > 0}
          aria-autocomplete="list"
        />
        {results.length === 0 ? (
          <div className="palette-empty">No commands match</div>
        ) : (
          <ul className="palette-list" role="listbox">
            {results.map((cmd, i) => (
              <li
                key={cmd.id}
                className={`palette-item${i === selectedIdx ? " palette-item--selected" : ""}`}
                role="option"
                aria-selected={i === selectedIdx}
                onMouseEnter={() => setSelectedIdx(i)}
                onClick={() => execute(cmd)}
              >
                <span className="palette-item__group">{GROUP_LABELS[cmd.group] ?? cmd.group}</span>
                <span className="palette-item__label">{cmd.label}</span>
                {cmd.shortcut && <kbd className="palette-item__shortcut">{cmd.shortcut}</kbd>}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
