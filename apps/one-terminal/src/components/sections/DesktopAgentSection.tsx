/**
 * DesktopAgentSection
 *
 * App Menu drawer section for choosing which FDC3 Desktop Agent this
 * Terminal's widgets connect to (Plan 16). The list and the active choice
 * come from Rust (`wm_agent_list`); switching calls `wm_agent_select`, which
 * persists the choice and recreates every panel webview so widgets reconnect.
 *
 * Read-only when the agent is forced by OT_FDC3_AGENT_URL or the admin set
 * `fdc3.allowUserOverride: false`.
 */

import { useMemo, useState } from "react";
import {
  type AgentEntry,
  type AgentListing,
  isProd,
  selectAgent,
  useDesktopAgents,
  useReachability,
} from "../../lib/desktopAgents";
import "./DesktopAgentSection.css";

const SOURCE_TEXT: Record<AgentListing["source"], string> = {
  env: "Set by the OT_FDC3_AGENT_URL environment variable",
  user: "Your choice",
  config: "Default from terminal configuration",
  builtin: "Built-in default",
};

function groupName(a: AgentEntry): string {
  if (a.env) return a.env.toUpperCase();
  return a.builtin ? "Built-in" : "Other";
}

export function DesktopAgentSection() {
  const { listing, setListing } = useDesktopAgents();
  const reach = useReachability(listing?.agents);
  // Target awaiting confirmation; `reset` clears the saved choice instead
  // of saving this agent as the user's pick.
  const [pending, setPending] = useState<{ agent: AgentEntry; reset: boolean } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const groups = useMemo(() => {
    const out = new Map<string, AgentEntry[]>();
    for (const a of listing?.agents ?? []) {
      const g = groupName(a);
      out.set(g, [...(out.get(g) ?? []), a]);
    }
    return [...out.entries()];
  }, [listing]);

  if (!listing) {
    return <div className="ot-agent">Loading…</div>;
  }

  const readOnly = listing.locked || !listing.allowOverride;
  const active = listing.agents.find((a) => a.id === listing.selectedId);
  const defaultAgent = listing.agents.find((a) => a.id === listing.defaultId);

  const apply = (id: string | null) => {
    setBusy(true);
    setError(null);
    selectAgent(id)
      .then((l) => {
        setListing(l);
        setPending(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  };

  return (
    <div className="ot-agent">
      <header className="ot-agent__head">
        <h2 className="ot-agent__title">Desktop Agent</h2>
        <p className="ot-agent__subtitle">
          Choose which FDC3 Desktop Agent your widgets connect to. Your choice is remembered when
          the Terminal restarts.
        </p>
      </header>

      {listing.notice && <div className="ot-agent__banner">{listing.notice}</div>}
      {listing.locked && (
        <div className="ot-agent__banner">
          The agent is set by the <code>OT_FDC3_AGENT_URL</code> environment variable. Unset it to
          choose an agent here.
        </div>
      )}
      {!listing.locked && !listing.allowOverride && (
        <div className="ot-agent__banner">
          Your administrator manages the Desktop Agent for this Terminal.
        </div>
      )}

      <section className="ot-agent__active" aria-live="polite">
        <span className="ot-agent__label">Active agent</span>
        <span className="ot-agent__active-name">
          {active?.name ?? listing.selectedId}
          {isProd(active) && <span className="ot-agent__tag ot-agent__tag--prod">PROD</span>}
        </span>
        {active?.url && <span className="ot-agent__url">{active.url}</span>}
        <span className="ot-agent__source">{SOURCE_TEXT[listing.source]}</span>
      </section>

      {groups.map(([group, agents]) => (
        <section key={group} className="ot-agent__group">
          <span className="ot-agent__label">{group}</span>
          <ul className="ot-agent__list" role="radiogroup" aria-label={`${group} agents`}>
            {agents.map((a) => {
              const isActive = a.id === listing.selectedId;
              const disabled = readOnly || busy || !a.supported || isActive;
              const r = reach[a.id] ?? "unknown";
              return (
                <li key={a.id}>
                  <button
                    type="button"
                    role="radio"
                    aria-checked={isActive}
                    className={
                      "ot-agent__row" +
                      (isActive ? " ot-agent__row--active" : "") +
                      (a.id === pending?.agent.id ? " ot-agent__row--pending" : "")
                    }
                    disabled={disabled}
                    onClick={() => setPending({ agent: a, reset: false })}
                    title={
                      a.supported ? undefined : `Needs a '${a.kind}' connector (not available yet)`
                    }
                  >
                    <span
                      className={`ot-agent__dot ot-agent__dot--${a.url ? r : "unknown"}`}
                      aria-label={
                        a.url
                          ? r === "up"
                            ? "reachable"
                            : r === "down"
                              ? "unreachable"
                              : "checking"
                          : undefined
                      }
                    />
                    <span className="ot-agent__row-main">
                      <span className="ot-agent__row-name">
                        {a.name}
                        {isProd(a) && (
                          <span className="ot-agent__tag ot-agent__tag--prod">PROD</span>
                        )}
                        {a.id === listing.defaultId && (
                          <span className="ot-agent__tag">Default</span>
                        )}
                        {isActive && (
                          <span className="ot-agent__tag ot-agent__tag--active">Active</span>
                        )}
                        {!a.supported && (
                          <span className="ot-agent__tag">{a.kind} · unavailable</span>
                        )}
                      </span>
                      {a.url && <span className="ot-agent__url">{a.url}</span>}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      ))}

      {pending && (
        <div
          className={
            "ot-agent__confirm" + (isProd(pending.agent) ? " ot-agent__confirm--prod" : "")
          }
        >
          <p>
            {pending.reset ? "Reset to the default, " : "Switch to "}
            <strong>{pending.agent.name}</strong>? Every widget will reload and reconnect, and
            unsaved in-widget state will be lost.
          </p>
          {isProd(pending.agent) && (
            <p className="ot-agent__warn">
              This is a production agent. Context you share will reach production apps.
            </p>
          )}
          <div className="ot-agent__actions">
            <button
              type="button"
              className="ot-agent__btn ot-agent__btn--primary"
              disabled={busy}
              onClick={() => apply(pending.reset ? null : pending.agent.id)}
            >
              {busy ? "Switching…" : "Switch"}
            </button>
            <button
              type="button"
              className="ot-agent__btn"
              disabled={busy}
              onClick={() => setPending(null)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {!readOnly && listing.source === "user" && !pending && (
        <div className="ot-agent__actions">
          <button
            type="button"
            className="ot-agent__btn"
            disabled={busy}
            onClick={() =>
              // Already on the default agent: just forget the saved choice
              // (no reload). Otherwise confirm, since widgets will reload.
              listing.selectedId === listing.defaultId || !defaultAgent
                ? apply(null)
                : setPending({ agent: defaultAgent, reset: true })
            }
          >
            Reset to default
          </button>
        </div>
      )}

      {error && (
        <div className="ot-agent__error" role="alert">
          {error}
        </div>
      )}
    </div>
  );
}
