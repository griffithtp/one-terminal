/**
 * EnginePickerDialog
 *
 * Modal shown when the user clicks an app that declares multiple supported
 * engines on the current OS. Lets the user pick which engine to launch with.
 *
 * Pure presentational — state is owned by `useAppLaunch`.
 */

import type { AppRecord, EngineBinding } from "../types";

interface Props {
  app: AppRecord;
  engines: EngineBinding[];
  onPick: (binding: EngineBinding) => void;
  onCancel: () => void;
}

export function EnginePickerDialog({ app, engines, onPick, onCancel }: Props) {
  return (
    <div className="wm-engine-picker__backdrop" role="dialog" aria-modal="true">
      <div className="wm-engine-picker">
        <div className="wm-engine-picker__title">
          Launch <strong>{app.title ?? app.name}</strong> with
        </div>
        <ul className="wm-engine-picker__list">
          {engines.map((b) => (
            <li key={`${b.family}@${b.version}`}>
              <button type="button" className="wm-engine-picker__option" onClick={() => onPick(b)}>
                <span className="wm-engine-picker__family">{b.family}</span>
                <span className="wm-engine-picker__version">{b.version}</span>
              </button>
            </li>
          ))}
        </ul>
        <div className="wm-engine-picker__actions">
          <button type="button" className="wm-engine-picker__cancel" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
