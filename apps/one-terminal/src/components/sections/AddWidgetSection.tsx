/**
 * AddWidgetSection
 *
 * Drawer section that replaces the header's app-launcher row. Lists every
 * app from the App Directory with a search filter; clicking a card delegates
 * to the parent's `onSelect` which both launches the app (engine picker /
 * download flow happens via useAppLaunch) and closes the drawer.
 *
 * Also renders a built-in "Custom Web Widget" card that isn't sourced from
 * the App Directory — selecting it prompts for a URL, validates it to
 * `http`/`https` only, and synthesizes an `AppRecord` for the same
 * `onSelect` launch path.
 */

import { useMemo, useState } from "react";
import type { AppRecord, EngineBinding } from "../../types";
import { GENERIC_WEB_WIDGET_APP_ID } from "../../lib/genericWebWidget";
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

/**
 * Normalize a user-entered address to an `http`/`https` URL, or return
 * `null` if it can't be made into one. This is the only gate between free
 * text and a native webview navigation — it must reject `javascript:`,
 * `file:`, `data:`, and any other non-web scheme.
 */
function normalizeWebUrl(input: string): string | null {
  const trimmed = input.trim();
  if (!trimmed) return null;
  const candidate = /^[a-zA-Z][a-zA-Z\d+.-]*:\/\//.test(trimmed) ? trimmed : `https://${trimmed}`;
  try {
    const parsed = new URL(candidate);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return null;
    return parsed.toString();
  } catch {
    return null;
  }
}

function CustomWebWidgetForm({ onSelect }: { onSelect: (app: AppRecord) => void }) {
  const [url, setUrl] = useState("");
  const [title, setTitle] = useState("");
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const normalized = normalizeWebUrl(url);
    if (!normalized) {
      setError("Enter a valid http(s) web address.");
      return;
    }
    const hostname = (() => {
      try {
        return new URL(normalized).hostname;
      } catch {
        return normalized;
      }
    })();
    const record: AppRecord = {
      appId: GENERIC_WEB_WIDGET_APP_ID,
      name: "Custom Web Widget",
      type: "web",
      title: title.trim() || hostname,
      details: { url: normalized },
      categories: [],
    };
    onSelect(record);
  };

  return (
    <form className="ot-add-widget__custom-form" onSubmit={handleSubmit}>
      <label className="ot-add-widget__custom-label" htmlFor="ot-add-widget-url">
        Web address
      </label>
      <input
        id="ot-add-widget-url"
        type="text"
        className="ot-add-widget__custom-input"
        placeholder="example.com"
        value={url}
        onChange={(e) => {
          setUrl(e.target.value);
          setError(null);
        }}
        autoFocus
      />
      <label className="ot-add-widget__custom-label" htmlFor="ot-add-widget-title">
        Title (optional)
      </label>
      <input
        id="ot-add-widget-title"
        type="text"
        className="ot-add-widget__custom-input"
        placeholder="Widget title"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
      />
      {error && <p className="ot-add-widget__custom-error">{error}</p>}
      <button type="submit" className="ot-add-widget__custom-submit">
        Add widget
      </button>
    </form>
  );
}

export function AddWidgetSection({ apps, enginesFor, onSelect }: Props) {
  const [query, setQuery] = useState("");
  const [customFormOpen, setCustomFormOpen] = useState(false);

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
        <li>
          {customFormOpen ? (
            <div className="ot-add-widget__card ot-add-widget__card--custom-open">
              <span className="ot-add-widget__card-head">
                <span className="ot-add-widget__card-title">Custom Web Widget</span>
              </span>
              <CustomWebWidgetForm
                onSelect={(app) => {
                  setCustomFormOpen(false);
                  onSelect(app);
                }}
              />
              <button
                type="button"
                className="ot-add-widget__custom-cancel"
                onClick={() => setCustomFormOpen(false)}
              >
                Cancel
              </button>
            </div>
          ) : (
            <button
              type="button"
              className="ot-add-widget__card"
              onClick={() => setCustomFormOpen(true)}
              title="Launch any web address as a new tab"
            >
              <span className="ot-add-widget__card-head">
                <span className="ot-add-widget__card-title">Custom Web Widget</span>
              </span>
              <span className="ot-add-widget__card-desc">
                Launch a widget from any web address you enter.
              </span>
            </button>
          )}
        </li>

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
