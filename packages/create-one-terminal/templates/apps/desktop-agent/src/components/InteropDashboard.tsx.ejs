import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { IntentHandlerInfo, ChannelInfo, PeerInfo } from "../types";

// ── FDC3 2.2 context templates ─────────────────────────────────────────────────

const TEMPLATES: Record<string, object> = {
  "fdc3.instrument": {
    type: "fdc3.instrument",
    name: "EUR/USD",
    id: { RIC: "EUR=", ISIN: "EUR" },
  },
  "fdc3.contact": {
    type: "fdc3.contact",
    name: "John Trader",
    id: { email: "j.trader@bank.com" },
  },
  "fdc3.organization": {
    type: "fdc3.organization",
    name: "Acme Corp",
    id: { LEI: "9ABCDE1234567890XY12" },
  },
  "fx.rate": {
    type: "fx.rate",
    name: "EUR/USD",
    baseCurrency: "EUR",
    quoteCurrency: "USD",
    bid: 1.08421,
    ask: 1.08434,
    rate: 1.08427,
  },
};

const INTENTS = [
  "ViewChart",
  "ViewQuote",
  "ViewOrders",
  "StartCall",
  "SendChatMessage",
  "CreateInteraction",
];

type Tab = "broadcast" | "intent";

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  channels: ChannelInfo[];
  peers: PeerInfo[];
}

export function InteropDashboard({ channels, peers }: Props) {
  const [tab, setTab] = useState<Tab>("broadcast");
  const [contextJson, setContextJson] = useState(
    JSON.stringify(TEMPLATES["fdc3.instrument"], null, 2)
  );
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [targetChannel, setTargetChannel] = useState<string>("");

  // Intent state
  const [intent, setIntent] = useState(INTENTS[0]);
  const [handlers, setHandlers] = useState<IntentHandlerInfo[]>([]);
  const [selectedHandler, setSelectedHandler] = useState<string | null>(null);

  // Feedback messages
  const [feedback, setFeedback] = useState<{ ok: boolean; msg: string } | null>(null);

  const flash = useCallback((ok: boolean, msg: string) => {
    setFeedback({ ok, msg });
    setTimeout(() => setFeedback(null), 3500);
  }, []);

  // Reload handlers when tab switches to intent or intent name changes
  useEffect(() => {
    if (tab !== "intent") return;
    invoke<IntentHandlerInfo[]>("cda_list_intent_handlers").then(setHandlers).catch(console.error);
  }, [tab, intent]);

  const parseContext = useCallback((): object | null => {
    try {
      const v = JSON.parse(contextJson);
      setJsonError(null);
      return v;
    } catch (e) {
      setJsonError(String(e));
      return null;
    }
  }, [contextJson]);

  // ── Broadcast ───────────────────────────────────────────────────────────────

  const handleBroadcast = useCallback(async () => {
    const ctx = parseContext();
    if (!ctx) return;
    const ch = targetChannel || channels[0]?.channelId;
    if (!ch) {
      flash(false, "No channel selected");
      return;
    }
    try {
      const count = await invoke<number>("cda_broadcast_from_dashboard", {
        channelId: ch,
        context: ctx,
      });
      flash(true, `Broadcast to ${ch} — ${count} recipient${count !== 1 ? "s" : ""}`);
    } catch (e) {
      flash(false, String(e));
    }
  }, [parseContext, targetChannel, channels, flash]);

  // ── Raise Intent ────────────────────────────────────────────────────────────

  const handleRaiseIntent = useCallback(async () => {
    const ctx = parseContext();
    if (!ctx) return;
    try {
      const reqId = await invoke<string>("cda_raise_intent_from_dashboard", {
        intent,
        context: ctx,
        targetInstanceId: selectedHandler ?? null,
      });
      flash(true, `Intent '${intent}' dispatched — reqId: ${reqId.slice(0, 8)}…`);
    } catch (e) {
      flash(false, String(e));
    }
  }, [parseContext, intent, selectedHandler, flash]);

  // ── Derived: group handlers by local vs. each peer ─────────────────────────

  const localHandlers = handlers.filter((h) => !h.desktopAgentId);
  const peerHandlerMap = new Map<string, IntentHandlerInfo[]>();
  for (const h of handlers.filter((h) => h.desktopAgentId)) {
    const key = h.desktopAgentId!;
    if (!peerHandlerMap.has(key)) peerHandlerMap.set(key, []);
    peerHandlerMap.get(key)!.push(h);
  }

  return (
    <section className="panel id">
      <h2 className="panel__title">Interop Dashboard</h2>

      {/* ── Tab strip ─────────────────────────────────────────────────────── */}
      <div className="id__tabs">
        <button
          className={`id__tab ${tab === "broadcast" ? "id__tab--active" : ""}`}
          onClick={() => setTab("broadcast")}
        >
          Broadcast
        </button>
        <button
          className={`id__tab ${tab === "intent" ? "id__tab--active" : ""}`}
          onClick={() => setTab("intent")}
        >
          Raise Intent
        </button>
      </div>

      <div className="id__body">
        {/* ── Context JSON editor (shared) ─────────────────────────────────── */}
        <div className="id__section">
          <div className="id__row id__row--wrap">
            <label className="id__label">FDC3 Context</label>
            <div className="id__templates">
              {Object.keys(TEMPLATES).map((t) => (
                <button
                  key={t}
                  className="btn btn--sm"
                  onClick={() => setContextJson(JSON.stringify(TEMPLATES[t], null, 2))}
                >
                  {t}
                </button>
              ))}
            </div>
          </div>
          <textarea
            className={`id__editor ${jsonError ? "id__editor--error" : ""}`}
            value={contextJson}
            onChange={(e) => setContextJson(e.target.value)}
            spellCheck={false}
            rows={10}
          />
          {jsonError && <p className="id__error">{jsonError}</p>}
        </div>

        {/* ── Broadcast tab ────────────────────────────────────────────────── */}
        {tab === "broadcast" && (
          <div className="id__section">
            <div className="id__row">
              <label className="id__label" htmlFor="id-channel">
                Channel
              </label>
              <select
                id="id-channel"
                className="id__select"
                value={targetChannel}
                onChange={(e) => setTargetChannel(e.target.value)}
              >
                {channels.map((c) => (
                  <option key={c.channelId} value={c.channelId}>
                    {c.displayName} ({c.members.length} member{c.members.length !== 1 ? "s" : ""})
                  </option>
                ))}
              </select>
            </div>
            <button className="btn id__submit" onClick={handleBroadcast}>
              Broadcast Context
            </button>
          </div>
        )}

        {/* ── Raise Intent tab ─────────────────────────────────────────────── */}
        {tab === "intent" && (
          <div className="id__section">
            <div className="id__row">
              <label className="id__label" htmlFor="id-intent">
                Intent
              </label>
              <select
                id="id-intent"
                className="id__select"
                value={intent}
                onChange={(e) => {
                  setIntent(e.target.value);
                  setSelectedHandler(null);
                }}
              >
                {INTENTS.map((i) => (
                  <option key={i} value={i}>
                    {i}
                  </option>
                ))}
              </select>
            </div>

            {/* Intent Resolver — grouped by DesktopAgentIdentifier */}
            <div className="id__resolver">
              <p className="id__resolver-title">
                Intent Resolver
                {handlers.length === 0 && <span className="muted"> — no handlers registered</span>}
              </p>

              {/* Local handlers group */}
              {localHandlers.length > 0 && (
                <div className="id__resolver-group">
                  <span className="id__resolver-group-label">This CDA</span>
                  {localHandlers.map((h) => (
                    <HandlerRow
                      key={h.instanceId}
                      handler={h}
                      selected={selectedHandler === h.instanceId}
                      onSelect={() =>
                        setSelectedHandler(selectedHandler === h.instanceId ? null : h.instanceId)
                      }
                    />
                  ))}
                </div>
              )}

              {/* Per-peer handler groups */}
              {Array.from(peerHandlerMap.entries()).map(([agentId, peerHandlers]) => (
                <div key={agentId} className="id__resolver-group">
                  <span className="id__resolver-group-label id__resolver-group-label--peer">
                    {agentId}
                    <span className="id__peer-badge">bridge</span>
                  </span>
                  {peerHandlers.map((h) => (
                    <HandlerRow
                      key={h.instanceId}
                      handler={h}
                      selected={selectedHandler === h.instanceId}
                      onSelect={() =>
                        setSelectedHandler(selectedHandler === h.instanceId ? null : h.instanceId)
                      }
                    />
                  ))}
                </div>
              ))}

              {/* No handlers but peers exist → show peers as potential forwarders */}
              {handlers.length === 0 && peers.length > 0 && (
                <p className="id__empty-hint">
                  {peers.length} peer bridge{peers.length !== 1 ? "s" : ""} connected — intent will
                  be forwarded if left unresolved.
                </p>
              )}
            </div>

            <p className="id__resolve-hint">
              {selectedHandler
                ? `Targeting instance ${selectedHandler.slice(0, 8)}…`
                : "No target selected — auto-resolve will be used."}
            </p>

            <button className="btn id__submit" onClick={handleRaiseIntent}>
              Raise Intent
            </button>
          </div>
        )}

        {/* ── Feedback banner ──────────────────────────────────────────────── */}
        {feedback && (
          <div className={`id__feedback ${feedback.ok ? "id__feedback--ok" : "id__feedback--err"}`}>
            {feedback.msg}
          </div>
        )}
      </div>
    </section>
  );
}

// ── HandlerRow ────────────────────────────────────────────────────────────────

function HandlerRow({
  handler,
  selected,
  onSelect,
}: {
  handler: IntentHandlerInfo;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      className={`id__handler ${selected ? "id__handler--selected" : ""}`}
      onClick={onSelect}
      title={handler.instanceId}
    >
      <span className="id__handler-app">{handler.displayName ?? handler.appId}</span>
      <span className="id__handler-id muted">{handler.instanceId.slice(0, 8)}…</span>
    </button>
  );
}
