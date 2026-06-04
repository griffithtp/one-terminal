/**
 * WidgetCatalog
 *
 * Search-filtered grid of App Directory entries. Shared by:
 *   - [EmptyDashboardPicker](./EmptyDashboardPicker.tsx) — full-canvas
 *     surface when a dashboard has no widgets yet.
 *   - The overlay's "View all widgets" modal (OverlayApp) — opened from
 *     the kebab's Add Widget submenu when the user wants the bigger view.
 *
 * The catalog is pure presentation: it owns the search input + filter, but
 * leaves the click action to the caller via `onSelect`.
 */

import { useMemo, useState } from "react";
import type { AppRecord, EngineBinding } from "../types";

export interface WidgetCatalogProps {
  apps: AppRecord[];
  enginesFor: (app: AppRecord) => EngineBinding[];
  onSelect: (app: AppRecord) => void;
  /** Class applied to the outer wrapper. Lets parent decide layout (modal
   *  vs full-canvas) by swapping the BEM block. */
  variant: "empty-picker" | "all-widgets-modal";
  /** Optional heading copy. When omitted, no header text is rendered (the
   *  modal variant typically supplies its own title via a separate header). */
  title?: string;
  subtitle?: string;
  /** Whether the search input should grab focus on mount. */
  autoFocus?: boolean;
}

export function WidgetCatalog({
  apps,
  enginesFor,
  onSelect,
  variant,
  title,
  subtitle,
  autoFocus = true,
}: WidgetCatalogProps) {
  const [query, setQuery] = useState("");
  const block = variant === "empty-picker" ? "wm-empty-picker" : "wm-all-widgets";

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
    <div className={block}>
      {(title || subtitle) && (
        <div className={`${block}__head`}>
          {title && <h2 className={`${block}__title`}>{title}</h2>}
          {subtitle && <p className={`${block}__subtitle`}>{subtitle}</p>}
          <input
            type="search"
            className={`${block}__search`}
            placeholder="Search apps…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            autoFocus={autoFocus}
          />
        </div>
      )}
      {!title && !subtitle && (
        <input
          type="search"
          className={`${block}__search`}
          placeholder="Search apps…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          autoFocus={autoFocus}
        />
      )}

      <ul className={`${block}__list`}>
        {filtered.length === 0 ? (
          <li className={`${block}__empty`}>
            {apps.length === 0 ? "Loading apps…" : "No apps match your search."}
          </li>
        ) : (
          filtered.map((app) => {
            const engineCount = enginesFor(app).length;
            const cardTitle = app.title ?? app.name;
            const tooltip =
              engineCount > 1
                ? `${app.description ?? `Launch ${cardTitle}`} — choose engine (${engineCount} available)`
                : (app.description ?? `Launch ${cardTitle}`);
            return (
              <li key={app.appId}>
                <button
                  type="button"
                  className={`${block}__card`}
                  onClick={() => onSelect(app)}
                  title={tooltip}
                >
                  <span className={`${block}__card-head`}>
                    <span className={`${block}__card-title`}>{cardTitle}</span>
                    {engineCount > 1 && (
                      <span className={`${block}__card-badge`} aria-hidden>
                        ▾
                      </span>
                    )}
                  </span>
                  {app.description && (
                    <span className={`${block}__card-desc`}>{app.description}</span>
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
