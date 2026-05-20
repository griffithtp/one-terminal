import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";

type StartMode = "blank" | "template" | "saved";

interface Props {
  onConfirm: (name: string, mode: StartMode) => void;
  onCancel: () => void;
}

export function NewTerminalDialog({ onConfirm, onCancel }: Props) {
  const [name, setName] = useState("");
  const [mode, setMode] = useState<StartMode>("blank");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const canSubmit = name.trim().length > 0;

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key === "Enter" && canSubmit) onConfirm(name.trim(), mode);
      if (e.key === "Escape") onCancel();
    },
    [name, mode, canSubmit, onConfirm, onCancel]
  );

  return (
    <div className="wm-ds-dialog__backdrop" role="dialog" aria-modal="true">
      <div className="wm-ds-dialog">
        <div className="wm-ds-dialog__title">New Terminal</div>

        <input
          ref={inputRef}
          className="wm-ds-dialog__input"
          placeholder="Terminal name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={handleKeyDown}
        />

        <div className="wm-ntd__modes">
          <label className="wm-ntd__mode">
            <input
              type="radio"
              name="start-mode"
              value="blank"
              checked={mode === "blank"}
              onChange={() => setMode("blank")}
            />
            <span className="wm-ntd__mode-label">Blank</span>
            <span className="wm-ntd__mode-desc">One empty dashboard, no panels</span>
          </label>

          <label className="wm-ntd__mode wm-ntd__mode--disabled">
            <input type="radio" name="start-mode" value="template" disabled />
            <span className="wm-ntd__mode-label">From template</span>
            <span className="wm-ntd__mode-desc">Pick a layout template — available in Phase 4</span>
          </label>

          <label className="wm-ntd__mode wm-ntd__mode--disabled">
            <input type="radio" name="start-mode" value="saved" disabled />
            <span className="wm-ntd__mode-label">From saved dashboards</span>
            <span className="wm-ntd__mode-desc">
              Load from existing dashboards — available in Phase 3
            </span>
          </label>
        </div>

        <div className="wm-ds-dialog__actions">
          <button type="button" className="wm-ds-dialog__btn" onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className="wm-ds-dialog__btn wm-ds-dialog__btn--primary"
            disabled={!canSubmit}
            onClick={() => canSubmit && onConfirm(name.trim(), mode)}
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
