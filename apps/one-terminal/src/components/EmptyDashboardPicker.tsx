/**
 * EmptyDashboardPicker
 *
 * Centred inline app picker shown when the active dashboard has zero
 * widgets. Lets the user add the first widget without opening the App
 * Menu drawer — once the first widget lands the layout takes over.
 *
 * Wires `onSelect` straight to `useAppLaunch.launchApp` so the engine
 * picker / download flow happens in chrome (parked correctly). No overlay
 * involvement; this surface lives in the chrome webview and is naturally
 * uncovered when the dashboard is empty.
 */

import { useMemo, useState } from "react";
import type { AppRecord, EngineBinding } from "../types";

interface Props {
  apps: AppRecord[];
  enginesFor: (app: AppRecord) => EngineBinding[];
  /** First-widget launch — `target` is intentionally omitted; the new tab
   *  becomes the dashboard's root. */
  onSelect: (app: AppRecord) => void;
}

export function EmptyDashboardPicker({ apps, enginesFor, onSelect }: Props) {
  const [query, setQuery] = useState("");

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return apps;
    return apps.filter((app) => {
      const haystack =
        `${app.title ?? ""} ${app.name ?? ""} ${app.description ?? ""}`.toLowerCase();
      return haystack.includes(q);
    });
  }, [apps, query]);

  return (
    <div className="wm-empty-picker">
      <div className="wm-empty-picker__head">
        <h2 className="wm-empty-picker__title">Add your first widget</h2>
        <p className="wm-empty-picker__subtitle">
          Pick any app from the App Directory to start building this dashboard.
        </p>
        <input
          type="search"
          className="wm-empty-picker__search"
          placeholder="Search apps…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          autoFocus
        />
      </div>

      <ul className="wm-empty-picker__list">
        {filtered.length === 0 ? (
          <li className="wm-empty-picker__empty">
            {apps.length === 0 ? "Loading apps…" : "No apps match your search."}
          </li>
        ) : (
          filtered.map((app) => {
            const engineCount = enginesFor(app).length;
            const title = app.title ?? app.name;
            const tooltip =
              engineCount > 1
                ? `${app.description ?? `Launch ${title}`} — choose engine (${engineCount} available)`
                : (app.description ?? `Launch ${title}`);
            return (
              <li key={app.appId}>
                <button
                  type="button"
                  className="wm-empty-picker__card"
                  onClick={() => onSelect(app)}
                  title={tooltip}
                >
                  <span className="wm-empty-picker__card-head">
                    <span className="wm-empty-picker__card-title">{title}</span>
                    {engineCount > 1 && (
                      <span className="wm-empty-picker__card-badge" aria-hidden>
                        ▾
                      </span>
                    )}
                  </span>
                  {app.description && (
                    <span className="wm-empty-picker__card-desc">{app.description}</span>
                  )}
                </button>
              </li>
            );
          })
        )}
      </ul>
    </div>
  );
}
