import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import type { AppRecord, CdaIntentNeedsResolutionEvent } from "../types";

// ── Internal state ─────────────────────────────────────────────────────────────

interface ResolverState {
  intent: string;
  context: Record<string, unknown>;
  sourceInstanceId: string;
  requestId: string;
  candidates: AppRecord[];
}

// ── IntentResolverModal ────────────────────────────────────────────────────────

/**
 * Renders a modal overlay whenever the CDA backend emits
 * `cda:intent_needs_resolution`.  The user picks a candidate app; the modal
 * then launches it via `cda_launch_app`.
 *
 * Mount this component at the root of App so it is always active.
 */
export function IntentResolverModal() {
  const [state, setState] = useState<ResolverState | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [launching, setLaunching] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<CdaIntentNeedsResolutionEvent>(
      "cda:intent_needs_resolution",
      (event) => {
        const { intent, context, sourceInstanceId, requestId, candidates } = event.payload;
        setState({ intent, context, sourceInstanceId, requestId, candidates });
        setSelected(null);
        setFeedback(null);
      }
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!state) return null;

  const handleLaunch = async () => {
    if (!selected) return;
    const candidate = state.candidates.find((c) => c.appId === selected);
    if (!candidate?.details?.url) {
      setFeedback("Selected app has no launch URL");
      return;
    }
    setLaunching(true);
    setFeedback(null);
    try {
      await invoke("cda_launch_app", {
        appId: candidate.appId,
        appUrl: candidate.details.url,
        appType: candidate.type,
        appTitle: candidate.title ?? candidate.name,
      });
      setFeedback(`"${candidate.name}" launched — connect it and retry the intent`);
    } catch (e) {
      setFeedback(String(e));
    } finally {
      setLaunching(false);
    }
  };

  const handleDismiss = () => {
    setState(null);
    setFeedback(null);
  };

  return (
    <div className="irm__overlay" role="dialog" aria-modal="true">
      <div className="irm__dialog">
        <header className="irm__header">
          <span className="irm__title">Intent Resolver</span>
          <button className="irm__close" onClick={handleDismiss} aria-label="Close">
            ✕
          </button>
        </header>

        <div className="irm__body">
          <p className="irm__info">
            No running handler found for intent{" "}
            <strong className="irm__intent">{state.intent}</strong>. Choose an app to launch:
          </p>

          <ul className="irm__candidates">
            {state.candidates.map((c) => (
              <li
                key={c.appId}
                className={`irm__candidate ${selected === c.appId ? "irm__candidate--selected" : ""}`}
                onClick={() => setSelected(c.appId)}
              >
                <div className="irm__candidate-name">{c.title ?? c.name}</div>
                {c.description && <div className="irm__candidate-desc">{c.description}</div>}
                <div className="irm__candidate-meta">
                  {c.details?.url && <span className="irm__candidate-url">{c.details.url}</span>}
                  {c.interop?.intents?.listensFor &&
                    Object.keys(c.interop.intents.listensFor).map((intent) => (
                      <span key={intent} className="irm__intent-badge">
                        {intent}
                      </span>
                    ))}
                </div>
              </li>
            ))}
          </ul>

          {feedback && (
            <div className={`irm__feedback ${launching ? "" : "irm__feedback--done"}`}>
              {feedback}
            </div>
          )}
        </div>

        <footer className="irm__footer">
          <button className="irm__dismiss-btn" onClick={handleDismiss}>
            Cancel
          </button>
          <button
            className="irm__launch-btn"
            onClick={handleLaunch}
            disabled={!selected || launching}
          >
            {launching ? "Launching…" : "Launch App"}
          </button>
        </footer>
      </div>
    </div>
  );
}
