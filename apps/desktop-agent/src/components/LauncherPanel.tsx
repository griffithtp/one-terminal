import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useAppDirectory } from "../hooks/useAppDirectory";
import { useEngineOverride } from "../hooks/useEngineOverride";
import type { AppRecord } from "../types";

// ── TerminalCard ───────────────────────────────────────────────────────────────

function TerminalCard() {
  const [launching, setLaunching] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);

  const handleOpen = async () => {
    setLaunching(true);
    setFeedback(null);
    try {
      await invoke("cda_open_terminal");
      setFeedback("Launched");
    } catch (e) {
      setFeedback(String(e));
    } finally {
      setLaunching(false);
    }
  };

  return (
    <div className="lp__card lp__card--pinned">
      <div className="lp__card-header">
        <span className="lp__card-name">Terminal</span>
        <span className="lp__card-built-in">built-in</span>
      </div>
      <p className="lp__card-desc">Open a new OneTerminal window</p>
      <div className="lp__card-footer">
        <button className="lp__launch-btn" onClick={handleOpen} disabled={launching}>
          {launching ? "Opening…" : "Open"}
        </button>
      </div>
      {feedback && (
        <div
          className={`lp__feedback ${feedback === "Launched" ? "lp__feedback--ok" : "lp__feedback--err"}`}
        >
          {feedback}
        </div>
      )}
    </div>
  );
}

// ── AppDirectoryCard ───────────────────────────────────────────────────────────

function AppDirectoryCard() {
  const [opening, setOpening] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [appDirUrl, setAppDirUrl] = useState<string | null>(null);

  useEffect(() => {
    invoke<string>("cda_get_app_dir_url")
      .then(setAppDirUrl)
      .catch(() => null);
  }, []);

  const handleOpen = async () => {
    setOpening(true);
    setFeedback(null);
    try {
      await invoke("cda_open_app_directory");
      setFeedback("Opened");
    } catch (e) {
      setFeedback(String(e));
    } finally {
      setOpening(false);
    }
  };

  return (
    <div className="lp__card lp__card--pinned">
      <div className="lp__card-header">
        <span className="lp__card-name">App Directory</span>
        <span className="lp__card-built-in">built-in</span>
      </div>
      <p className="lp__card-desc">Browse and manage app registrations</p>
      <div className="lp__card-footer">
        {appDirUrl && <span className="lp__card-url">{appDirUrl}</span>}
        <button className="lp__launch-btn" onClick={handleOpen} disabled={opening}>
          {opening ? "Opening…" : "Open"}
        </button>
      </div>
      {feedback && (
        <div
          className={`lp__feedback ${feedback === "Opened" ? "lp__feedback--ok" : "lp__feedback--err"}`}
        >
          {feedback}
        </div>
      )}
    </div>
  );
}

// ── AppCard ────────────────────────────────────────────────────────────────────

interface AppCardProps {
  app: AppRecord;
}

function AppCard({ app }: AppCardProps) {
  const [launching, setLaunching] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const { override } = useEngineOverride();

  const handleLaunch = async () => {
    const url = app.details?.url;
    if (!url) {
      setFeedback("No URL defined for this app");
      return;
    }
    setLaunching(true);
    setFeedback(null);
    try {
      await invoke("cda_launch_app", {
        appId: app.appId,
        appUrl: url,
        appType: app.type,
        appTitle: app.title ?? app.name,
        engineOverride: import.meta.env.DEV ? (override ?? null) : null,
      });
      setFeedback(
        override && import.meta.env.DEV
          ? `Launched (engine override: ${override.family}@${override.version})`
          : "Launched"
      );
    } catch (e) {
      setFeedback(String(e));
    } finally {
      setLaunching(false);
    }
  };

  return (
    <div className="lp__card">
      <div className="lp__card-header">
        <span className="lp__card-name">{app.title ?? app.name}</span>
        {app.version && <span className="lp__card-version">v{app.version}</span>}
      </div>

      {app.description && <p className="lp__card-desc">{app.description}</p>}

      {app.categories.length > 0 && (
        <div className="lp__card-tags">
          {app.categories.map((c) => (
            <span key={c} className="lp__tag">
              {c}
            </span>
          ))}
        </div>
      )}

      {app.interop?.intents?.listensFor && (
        <div className="lp__card-intents">
          <span className="lp__card-intents-label">Handles:</span>
          {Object.keys(app.interop.intents.listensFor).map((intent) => (
            <span key={intent} className="lp__intent-badge">
              {intent}
            </span>
          ))}
        </div>
      )}

      <div className="lp__card-footer">
        {app.details?.url && <span className="lp__card-url">{app.details.url}</span>}
        <button
          className="lp__launch-btn"
          onClick={handleLaunch}
          disabled={launching || !app.details?.url}
        >
          {launching ? "Launching…" : "Launch"}
        </button>
      </div>

      {feedback && (
        <div
          className={`lp__feedback ${feedback === "Launched" ? "lp__feedback--ok" : "lp__feedback--err"}`}
        >
          {feedback}
        </div>
      )}
    </div>
  );
}

// ── LauncherPanel ──────────────────────────────────────────────────────────────

export function LauncherPanel() {
  const { apps, loading, error, load, refresh } = useAppDirectory();

  // Load on first mount
  useEffect(() => {
    load();
  }, [load]);

  return (
    <section className="lp">
      <div className="lp__toolbar">
        <h2 className="lp__title">App Launcher</h2>
        <button className="lp__refresh-btn" onClick={refresh} disabled={loading}>
          {loading ? "Loading…" : "Refresh"}
        </button>
      </div>

      {error && <div className="lp__error">{error}</div>}

      {!loading && apps.length === 0 && !error && (
        <div className="lp__empty">
          No apps found. Is the App Directory service running on port 3005?
        </div>
      )}

      <div className="lp__grid">
        <TerminalCard />
        <AppDirectoryCard />
        {apps.map((app: AppRecord) => (
          <AppCard key={app.appId} app={app} />
        ))}
      </div>
    </section>
  );
}
