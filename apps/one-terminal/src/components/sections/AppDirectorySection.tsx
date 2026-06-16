/**
 * AppDirectorySection
 *
 * App Menu drawer section for pointing the launcher at an App Directory
 * endpoint. The catalog is the union of this endpoint and the local
 * widgets.config.json; this section only controls the remote endpoint.
 *
 * The field defaults to the built-in endpoint from the terminal config
 * (`appDirectoryUrl`); saving an override persists it via
 * `appDirectorySettings` and the catalog re-fetches live (useWidgetCatalog
 * subscribes to the store). "Reset to default" clears the override.
 */

import { useEffect, useState } from "react";
import { getTerminalConfig } from "../../lib/terminalConfig";
import {
  type AppDirectorySettings,
  loadAppDir,
  saveAppDir,
  subscribe,
} from "../../settings/appDirectorySettings";
import "./AppDirectorySection.css";

/** Empty (use default) or a syntactically valid http(s) URL. */
function isValid(url: string): boolean {
  const trimmed = url.trim();
  if (!trimmed) return true;
  try {
    const parsed = new URL(trimmed);
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

export function AppDirectorySection() {
  const [settings, setSettings] = useState<AppDirectorySettings>(() => loadAppDir());
  const [draft, setDraft] = useState(settings.urlOverride);
  const [defaultUrl, setDefaultUrl] = useState("");

  // Keep in sync if another surface saves an override.
  useEffect(() => subscribe(setSettings), []);
  useEffect(() => setDraft(settings.urlOverride), [settings.urlOverride]);

  useEffect(() => {
    getTerminalConfig()
      .then((cfg) => setDefaultUrl(cfg.appDirectoryUrl))
      .catch(() => {});
  }, []);

  const valid = isValid(draft);
  const dirty = draft.trim() !== settings.urlOverride.trim();
  const effective = draft.trim() || defaultUrl || "(none configured)";

  const handleSave = () => {
    if (!valid) return;
    saveAppDir({ urlOverride: draft.trim() });
  };

  const handleReset = () => {
    setDraft("");
    saveAppDir({ urlOverride: "" });
  };

  return (
    <div className="ot-app-directory">
      <header className="ot-app-directory__head">
        <h2 className="ot-app-directory__title">App Directory</h2>
        <p className="ot-app-directory__subtitle">
          Choose which App Directory the launcher fetches widgets from. Widgets from this endpoint
          are combined with any local widgets. Leave blank to use the built-in default.
        </p>
      </header>

      <label className="ot-app-directory__field" htmlFor="app-directory-url">
        <span className="ot-app-directory__field-label">Endpoint URL</span>
        <input
          id="app-directory-url"
          type="url"
          className="ot-app-directory__input"
          placeholder={defaultUrl || "https://apps.example.com/v2/apps"}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleSave();
          }}
          spellCheck={false}
          autoComplete="off"
        />
        {!valid && (
          <span className="ot-app-directory__error">Enter a valid http(s) URL, or leave blank.</span>
        )}
      </label>

      <div className="ot-app-directory__actions">
        <button
          type="button"
          className="ot-app-directory__btn ot-app-directory__btn--primary"
          onClick={handleSave}
          disabled={!valid || !dirty}
        >
          Save
        </button>
        <button
          type="button"
          className="ot-app-directory__btn"
          onClick={handleReset}
          disabled={!settings.urlOverride.trim() && !draft.trim()}
        >
          Reset to default
        </button>
      </div>

      <section className="ot-app-directory__effective" aria-live="polite">
        <span className="ot-app-directory__effective-label">Active endpoint</span>
        <span className="ot-app-directory__effective-url">{effective}</span>
      </section>
    </div>
  );
}
