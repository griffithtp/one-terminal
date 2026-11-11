/**
 * SavedDashboardPicker
 *
 * Shown when this Terminal window has no dashboard tabs open but the
 * account has saved dashboards — the starting state of every new Terminal.
 * Lists them so the user can open one here, or start a new dashboard
 * instead. Styled like the empty-dashboard app picker
 * ([EmptyDashboardPicker](./EmptyDashboardPicker.tsx)).
 */

import { invoke } from "@tauri-apps/api/core";

interface Props {
  /** The account's dashboards, none of them open in this window. */
  names: string[];
  /** Dashboard name → the other Terminal window that has it active. */
  lockedBy: Record<string, string>;
  onOpen: (name: string) => void;
}

export function SavedDashboardPicker({ names, lockedBy, onOpen }: Props) {
  return (
    <div className="wm-empty-picker">
      <div className="wm-empty-picker__head">
        <h2 className="wm-empty-picker__title">Open a dashboard</h2>
        <p className="wm-empty-picker__subtitle">
          Pick one of your saved dashboards to open in this Terminal, or start a new one.
        </p>
      </div>
      <ul className="wm-empty-picker__list">
        <li>
          <button
            type="button"
            className="wm-empty-picker__card"
            onClick={() => invoke("wm_dashboard_create_open").catch(console.error)}
          >
            <span className="wm-empty-picker__card-title">+ New dashboard</span>
            <span className="wm-empty-picker__card-desc">Start from an empty layout.</span>
          </button>
        </li>
        {names.map((name) => (
          <li key={name}>
            <button type="button" className="wm-empty-picker__card" onClick={() => onOpen(name)}>
              <span className="wm-empty-picker__card-head">
                <span className="wm-empty-picker__card-title">{name}</span>
              </span>
              {lockedBy[name] && (
                <span className="wm-empty-picker__card-badge">Open in {lockedBy[name]}</span>
              )}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
