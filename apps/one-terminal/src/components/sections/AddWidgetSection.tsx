/**
 * AddWidgetSection
 *
 * Drawer section that replaces the header's app-launcher row. Lists every
 * app from the App Directory with a search filter; clicking a card delegates
 * to the parent's `onSelect` which both launches the app (engine picker /
 * download flow happens via useAppLaunch) and closes the drawer.
 */

import { useMemo, useState } from "react";
import type { AppRecord, EngineBinding } from "../../types";
import "./AddWidgetSection.css";

interface Props {
  apps: AppRecord[];
  enginesFor: (app: AppRecord) => EngineBinding[];
  /**
   * Called when the user picks an app to launch. The parent is responsible
   * for triggering `useAppLaunch.launchApp(app)` and (typically) closing the
   * drawer afterwards so the new widget is immediately visible.
   */
  onSelect: (app: AppRecord) => void;
}

export function AddWidgetSection({ apps, enginesFor, onSelect }: Props) {
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
    <div className="ot-add-widget">
      <header className="ot-add-widget__head">
        <h2 className="ot-add-widget__title">Add Widget</h2>
        <p className="ot-add-widget__subtitle">
          Launch any app from the App Directory as a new tab.
        </p>
        <input
          type="search"
          className="ot-add-widget__search"
          placeholder="Search apps…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          autoFocus
        />
      </header>

      <ul className="ot-add-widget__list">
        {filtered.length === 0 ? (
          <li className="ot-add-widget__empty">
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
              <li key={app.catalogId ?? app.appId}>
                <button
                  type="button"
                  className="ot-add-widget__card"
                  onClick={() => onSelect(app)}
                  title={tooltip}
                >
                  <span className="ot-add-widget__card-head">
                    <span className="ot-add-widget__card-title">{title}</span>
                    {app.source && (
                      <span
                        className={`ot-add-widget__card-source ot-add-widget__card-source--${app.source}`}
                      >
                        {app.source === "appd" ? "App Directory" : "Local"}
                      </span>
                    )}
                    {engineCount > 1 && (
                      <span className="ot-add-widget__card-badge" aria-hidden>
                        ▾
                      </span>
                    )}
                  </span>
                  {app.description && (
                    <span className="ot-add-widget__card-desc">{app.description}</span>
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
