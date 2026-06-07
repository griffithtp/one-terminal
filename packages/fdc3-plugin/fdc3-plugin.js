/**
 * fdc3-plugin.js  —  FDC3 2.2 Desktop Agent shim (WebSocket transport)
 *
 * Include this file in any app window that needs `window.fdc3`.  It opens a
 * WebSocket to the CDA FDC3 Bus server and speaks the fdc3:* protocol.
 *
 * Usage (ES module):
 *   import { DesktopAgentClient } from './fdc3-plugin.js';
 *   const fdc3 = await DesktopAgentClient.connect('my-app-id');
 *
 * Usage (auto-init script tag):
 *   <script src="fdc3-plugin.js"></script>
 *   <!-- window.fdc3 is set automatically; await window.fdc3Ready -->
 *
 * Protocol
 * ────────
 * All messages are JSON text frames sent over WebSocket to
 * ws://localhost:7891/fdc3.
 *
 * Outbound  (app → CDA):  fdc3:* JSON, camelCase fields
 * Inbound   (CDA → app):  fdc3:* JSON, camelCase fields
 *
 * Every request carries a unique `requestId`; the CDA echoes it in the
 * reply so the plugin can resolve the matching pending Promise.
 * Push messages (contextBroadcast, intentDelivery) carry no requestId.
 */

// ── Constants ─────────────────────────────────────────────────────────────────

// URL resolution order (first non-empty wins):
//   1. <meta name="ot-fdc3-bus-url"> in the host page
//   2. window.OT_FDC3_AGENT_URL — injected by Terminal at panel-load time when
//      a fdc3.agentUrl is configured (Standalone variant)
//   3. window.OT_FDC3_BUS_URL — legacy override
//   4. ws://localhost:7891/fdc3 — Enterprise-bundled Desktop Agent default
//
// An empty string anywhere in the chain (e.g. window.OT_FDC3_AGENT_URL = "")
// short-circuits to NoAgentConfigured: connect() will reject so widgets can
// surface a clear "no FDC3 agent" state instead of timing out.
const _WS_META = document.querySelector?.('meta[name="ot-fdc3-bus-url"]')?.content;
const _AGENT_INJECTED = typeof window !== 'undefined' ? window.OT_FDC3_AGENT_URL : undefined;
const _LEGACY = typeof window !== 'undefined' ? window.OT_FDC3_BUS_URL : undefined;
// Detect explicit "no agent" intent: the host injected the variable but left it
// empty. (`undefined` means the host never set it — fall through to defaults.)
const _NO_AGENT = _AGENT_INJECTED === '';
const WS_URL = _WS_META || _AGENT_INJECTED || _LEGACY || 'ws://localhost:7891/fdc3';
const REPLY_TIMEOUT = 8_000;   // ms before a request promise rejects
const PROVIDER      = 'one-terminal-desktop-agent';
const FDC3_VERSION  = '2.2';

// ── Helpers ───────────────────────────────────────────────────────────────────

function newId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `${Math.random().toString(36).slice(2)}-${Date.now().toString(36)}`;
}

// ── DesktopAgentClient ────────────────────────────────────────────────────────

/**
 * Implements the FDC3 2.2 `DesktopAgent` interface over a WebSocket transport.
 * Construct via the static `connect()` factory so the async handshake is
 * awaited before any API calls are made.
 */
export class DesktopAgentClient {
  // private fields
  #ws;                // WebSocket
  #openPromise;       // Promise<void> resolves when WS is open
  #instanceId;        // assigned by CDA on successful hello
  #appId;
  #currentChannelId;  // id of the joined user channel (null if none)
  #knownChannels;     // ChannelSummary[] from welcome message

  // pending request Promises: requestId → { resolve, reject, timer }
  #pending = new Map();

  // registered context listeners: contextType | '*' → Set<handler>
  #contextListeners = new Map();

  // registered intent listeners: intent → Set<handler>
  #intentListeners = new Map();

  /** Private — use DesktopAgentClient.connect() */
  constructor(appId) {
    this.#appId            = appId;
    this.#currentChannelId = null;
    this.#knownChannels    = [];

    this.#openPromise = new Promise((resolve, reject) => {
      this.#ws = new WebSocket(WS_URL);

      this.#ws.onopen    = () => resolve();
      this.#ws.onerror   = () => reject(new Error('[fdc3] WebSocket connection to CDA failed'));
      this.#ws.onmessage = (ev) => {
        try {
          this.#onMessage(JSON.parse(ev.data));
        } catch (e) {
          console.error('[fdc3] parse error', e);
        }
      };
      this.#ws.onclose = () => this.#onClose();
    });
  }

  // ── Factory ────────────────────────────────────────────────────────────────

  /**
   * Perform the CDA handshake and return a ready agent.
   *
   * @param {string} [appId]  Logical app name; defaults to `document.title`.
   * @returns {Promise<DesktopAgentClient>}
   */
  static async connect(appId) {
    if (_NO_AGENT) {
      throw new Error('NoAgentConfigured: window.OT_FDC3_AGENT_URL is empty');
    }
    const id = appId
      ?? new URLSearchParams(globalThis.location?.search ?? '').get('fdc3AppId')
      ?? (typeof document !== 'undefined' ? document.title : null)
      ?? 'unknown-app';

    const client = new DesktopAgentClient(id);
    await client.#handshake();
    return client;
  }

  // ── Internal transport ────────────────────────────────────────────────────

  /** Send a fire-and-forget JSON message over the WebSocket. */
  #post(msg) {
    if (this.#ws && this.#ws.readyState === WebSocket.OPEN) {
      this.#ws.send(JSON.stringify(msg));
    }
  }

  /**
   * Send a message and wait for the CDA to reply with the same `requestId`.
   * Rejects after REPLY_TIMEOUT ms.
   */
  #request(msg) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(msg.requestId);
        reject(new Error(`[fdc3] request '${msg.type}' timed out`));
      }, REPLY_TIMEOUT);
      this.#pending.set(msg.requestId, { resolve, reject, timer });
      this.#post(msg);
    });
  }

  /** Wait for WS open, send fdc3:hello, and await fdc3:welcome. */
  async #handshake() {
    await this.#openPromise;
    const requestId = newId();
    const reply = await this.#request({
      type:      'fdc3:hello',
      appId:     this.#appId,
      requestId,
    });
    this.#instanceId    = reply.instanceId;
    this.#knownChannels = reply.channels ?? [];
  }

  /** Called when the WebSocket closes unexpectedly. */
  #onClose() {
    this.#pending.forEach(({ reject, timer }) => {
      clearTimeout(timer);
      reject(new Error('[fdc3] WebSocket connection closed'));
    });
    this.#pending.clear();
  }

  /** Route an inbound WebSocket message. */
  #onMessage(data) {
    if (!data || typeof data.type !== 'string') return;

    // ── Push: context broadcast ───────────────────────────────────────────
    if (data.type === 'fdc3:contextBroadcast') {
      const { channelId, context, sourceAppId } = data;
      if (!context) return;
      // Deliver only if on the same channel
      if (this.#currentChannelId && this.#currentChannelId !== channelId) return;
      const meta = sourceAppId ? { source: { appId: sourceAppId } } : undefined;
      this.#contextListeners.get(context.type)?.forEach((h) => this.#safeCall(h, context, meta));
      this.#contextListeners.get('*')?.forEach((h) => this.#safeCall(h, context, meta));
      return;
    }

    // ── Push: intent delivery ─────────────────────────────────────────────
    if (data.type === 'fdc3:intentDelivery') {
      const { intent, context, sourceInstanceId, targetInstanceId } = data;
      // Only process if addressed to this instance (or no target specified)
      if (targetInstanceId && targetInstanceId !== this.#instanceId) return;
      const meta = { source: { appId: sourceInstanceId ?? '' }, requestId: data.deliveryId };
      this.#intentListeners.get(intent)?.forEach((h) => this.#safeCall(h, context, meta));
      return;
    }

    // ── Request-reply: resolve matching pending Promise ───────────────────
    const { requestId } = data;
    if (requestId && this.#pending.has(requestId)) {
      const { resolve, reject, timer } = this.#pending.get(requestId);
      clearTimeout(timer);
      this.#pending.delete(requestId);
      if (data.type === 'fdc3:error') {
        reject(Object.assign(new Error(data.message ?? data.code ?? 'fdc3 error'), { code: data.code }));
      } else {
        resolve(data);
      }
    }
  }

  /** Invoke a handler, swallowing exceptions. */
  #safeCall(handler, ...args) {
    try { handler(...args); } catch (e) { console.error('[fdc3] listener error', e); }
  }

  // ── FDC3 2.2 — Meta ───────────────────────────────────────────────────────

  async getInfo() {
    return {
      fdc3Version:      FDC3_VERSION,
      provider:         PROVIDER,
      providerVersion:  '0.1.0',
      optionalFeatures: {
        OriginatingAppMetadata:    false,
        UserChannelMembershipAPIs: true,
      },
      appMetadata: { appId: this.#appId, instanceId: this.#instanceId },
    };
  }

  // ── FDC3 2.2 — Context broadcasting ──────────────────────────────────────

  async broadcast(context) {
    if (!this.#currentChannelId) {
      throw new Error('fdc3: broadcast called before joining a user channel');
    }
    this.#post({
      type:       'fdc3:broadcast',
      instanceId: this.#instanceId,
      channelId:  this.#currentChannelId,
      context,
    });
  }

  async addContextListener(contextType, handler) {
    const key = contextType ?? '*';
    if (!this.#contextListeners.has(key)) {
      this.#contextListeners.set(key, new Set());
    }
    this.#contextListeners.get(key).add(handler);
    return {
      unsubscribe: () => { this.#contextListeners.get(key)?.delete(handler); },
    };
  }

  // ── FDC3 2.2 — Intents ────────────────────────────────────────────────────

  async raiseIntent(intent, context, app) {
    const requestId = newId();
    const reply = await this.#request({
      type:             'fdc3:raiseIntent',
      instanceId:       this.#instanceId,
      intent,
      context,
      targetInstanceId: app?.instanceId ?? null,
      requestId,
    });
    return {
      intent:    reply.intent ?? intent,
      source:    { appId: reply.handlerAppId ?? 'unknown', instanceId: reply.handlerInstanceId },
      version:   FDC3_VERSION,
      getResult: async () => undefined,
    };
  }

  async raiseIntentForContext(_context, _app) {
    throw new Error('fdc3: raiseIntentForContext is not supported in this transport shim');
  }

  async addIntentListener(intent, handler) {
    if (!this.#intentListeners.has(intent)) {
      this.#intentListeners.set(intent, new Set());
    }
    this.#intentListeners.get(intent).add(handler);

    // Register with CDA so it can route matching intents here
    this.#post({
      type:       'fdc3:addIntentListener',
      instanceId: this.#instanceId,
      intent,
    });

    return {
      unsubscribe: () => {
        this.#intentListeners.get(intent)?.delete(handler);
        this.#post({
          type:       'fdc3:removeIntentListener',
          instanceId: this.#instanceId,
          intent,
        });
      },
    };
  }

  async findIntent(intent, _context, _resultType) {
    return { intent: { name: intent, displayName: intent }, apps: [] };
  }

  async findIntentsByContext(_context, _resultType) {
    return [];
  }

  // ── FDC3 2.2 — Channels ───────────────────────────────────────────────────

  async getUserChannels() {
    return this.#knownChannels.map((ch) => this.#wrapChannel(ch));
  }

  async getSystemChannels() {
    return this.getUserChannels();
  }

  async joinUserChannel(channelId) {
    const requestId = newId();
    await this.#request({
      type:       'fdc3:joinChannel',
      instanceId: this.#instanceId,
      channelId,
      requestId,
    });
    this.#currentChannelId = channelId;
  }

  async leaveCurrentChannel() {
    if (!this.#currentChannelId) return;
    this.#post({
      type:       'fdc3:leaveChannel',
      instanceId: this.#instanceId,
    });
    this.#currentChannelId = null;
  }

  async getCurrentChannel() {
    if (!this.#currentChannelId) return null;
    const raw = this.#knownChannels.find((c) => c.channelId === this.#currentChannelId);
    return raw
      ? this.#wrapChannel(raw)
      : this.#wrapChannel({ channelId: this.#currentChannelId });
  }

  async getCurrentContext(_contextType) {
    return null;
  }

  // ── FDC3 2.2 — App open / find ────────────────────────────────────────────

  async open(app, context) {
    const appId = typeof app === 'string' ? app : app?.appId;
    const requestId = newId();
    const reply = await this.#request({
      type:      'fdc3:open',
      instanceId: this.#instanceId,
      appId,
      context:   context ?? null,
      requestId,
    });
    return { appId: reply.appId ?? appId };
  }

  async findInstances(_app) {
    return [];
  }

  // ── Identity ──────────────────────────────────────────────────────────────

  getIdentity() {
    return { appId: this.#appId, instanceId: this.#instanceId };
  }

  // ── Lifecycle ─────────────────────────────────────────────────────────────

  disconnect() {
    this.#post({ type: 'fdc3:goodbye', instanceId: this.#instanceId });
    this.#ws?.close();
    this.#contextListeners.clear();
    this.#intentListeners.clear();
    this.#pending.forEach(({ reject, timer }) => {
      clearTimeout(timer);
      reject(new Error('[fdc3] agent disconnected'));
    });
    this.#pending.clear();
  }

  // ── Private helpers ───────────────────────────────────────────────────────

  #wrapChannel(raw) {
    const id = raw.channelId ?? raw.id ?? raw;
    return {
      id,
      type: 'user',
      displayMetadata: {
        name:  raw.displayName ?? id,
        color: raw.color ?? undefined,
      },
      broadcast: (ctx) => {
        if (this.#currentChannelId !== id) {
          return this.joinUserChannel(id).then(() => this.broadcast(ctx));
        }
        return this.broadcast(ctx);
      },
      getCurrentContext:  (_type) => this.getCurrentContext(_type),
      addContextListener: (type, handler) => this.addContextListener(type, handler),
    };
  }
}

// ── Auto-initialisation ───────────────────────────────────────────────────────
//
// When loaded as a plain <script>, automatically set window.fdc3.
// `window.fdc3Ready` resolves with the DesktopAgentClient once the
// handshake completes.

if (typeof window !== 'undefined') {
  const _ready = DesktopAgentClient.connect()
    .then((client) => {
      window.fdc3 = client;
      window.dispatchEvent(new CustomEvent('fdc3Ready', { detail: client }));
      return client;
    })
    .catch((err) => {
      console.warn('[fdc3-plugin] Could not connect to CDA hub:', err.message);
    });

  /** Resolves with the DesktopAgentClient once the handshake completes. */
  window.fdc3Ready = _ready;
}
