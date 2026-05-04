import type {
  AppIdentifier,
  AppIntent,
  Channel,
  Context,
  ContextHandler,
  ImplementationMetadata,
  IntentHandler,
  IntentResolution,
  Listener,
  BridgeAppIdentifier,
  BmpMessage,
  BmpAgentIdentifier,
} from "./types";
import { Fdc3Agent } from "./agent";

// ── WCP handshake timeout ────────────────────────────────────────────────────

const CONNECT_TIMEOUT_MS = 1_500;

// ── Helpers ──────────────────────────────────────────────────────────────────

function randomUuid(): string {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  return `${Math.random().toString(16).slice(2)}-${Date.now().toString(16)}`;
}

function makeMeta(
  source?: BmpAgentIdentifier,
  destination?: BmpAgentIdentifier
): BmpMessage["meta"] {
  return {
    requestUuid: randomUuid(),
    timestamp: Date.now(),
    ...(source ? { source } : {}),
    ...(destination ? { destination } : {}),
  };
}

// ── BridgeAgentProxy ─────────────────────────────────────────────────────────

/**
 * Wraps an `Fdc3Agent` with a WebSocket connection to the FDC3 2.2 Bridge
 * backplane, enabling cross-agent context broadcasting and intent routing.
 *
 * Usage:
 *   const proxy = await BridgeAgentProxy.connect(agent);
 *   window.fdc3 = proxy;
 *
 * All local operations (channel membership, non-bridged intents) are delegated
 * to the inner `Fdc3Agent` via Tauri IPC. Only cross-agent operations travel
 * over the WebSocket.
 *
 * If the bridge is unavailable, `connect` rejects and the caller can fall back
 * to the plain `Fdc3Agent`.
 */
export class BridgeAgentProxy {
  /** Bridge-assigned unique id for this Desktop Agent connection. */
  readonly desktopAgentId: string;

  private readonly _inner: Fdc3Agent;
  private readonly _ws: WebSocket;

  // Handlers registered via addContextListener — keyed by context type
  // ("__wildcard__" for null / any-type listeners).
  private readonly _contextHandlers = new Map<string, Set<ContextHandler>>();

  // Handlers registered via addIntentListener — keyed by intent name.
  private readonly _intentHandlers = new Map<string, Set<IntentHandler>>();

  // Pending raiseIntent promises — keyed by requestUuid.
  private readonly _pendingIntents = new Map<
    string,
    { resolve: (r: IntentResolution) => void; reject: (e: Error) => void }
  >();

  private constructor(
    inner: Fdc3Agent,
    ws: WebSocket,
    desktopAgentId: string
  ) {
    this._inner = inner;
    this._ws = ws;
    this.desktopAgentId = desktopAgentId;

    ws.onmessage = (event: MessageEvent) => {
      try {
        const msg: BmpMessage = JSON.parse(event.data as string);
        this._dispatch(msg);
      } catch {
        // ignore malformed messages
      }
    };
  }

  // ── Factory ───────────────────────────────────────────────────────────────

  /**
   * Open a WebSocket to the bridge, complete the WCP2 handshake, and return
   * a ready `BridgeAgentProxy`.
   *
   * Rejects if the bridge is unavailable or the handshake does not complete
   * within `CONNECT_TIMEOUT_MS`.
   */
  static async connect(
    inner: Fdc3Agent,
    wsUrl = "ws://127.0.0.1:4000/v2/bridge"
  ): Promise<BridgeAgentProxy> {
    const ws = new WebSocket(wsUrl);

    // Wait for the socket to open
    await Promise.race([
      new Promise<void>((resolve, reject) => {
        ws.onopen = () => resolve();
        ws.onerror = () => reject(new Error("Bridge WS connection failed"));
      }),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error("Bridge connection timed out")),
          CONNECT_TIMEOUT_MS
        )
      ),
    ]);

    // Send WCPHello
    const identity = inner.getIdentity();
    const hello: BmpMessage = {
      type: "WCPHello",
      meta: makeMeta({
        appId: identity.appId,
        instanceId: identity.instanceId,
      }),
      payload: {
        appId: identity.appId,
        instanceId: identity.instanceId ?? null,
      },
    };
    ws.send(JSON.stringify(hello));

    // Await WCPHelloResponse
    const response = await Promise.race([
      new Promise<BmpMessage>((resolve, reject) => {
        ws.onmessage = (e: MessageEvent) => {
          try {
            resolve(JSON.parse(e.data as string));
          } catch {
            reject(new Error("Invalid WCPHelloResponse"));
          }
        };
        ws.onerror = () => reject(new Error("Bridge handshake error"));
      }),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error("WCPHelloResponse timed out")),
          CONNECT_TIMEOUT_MS
        )
      ),
    ]);

    if (response.type !== "WCPHelloResponse") {
      ws.close();
      throw new Error(
        `Unexpected bridge response during handshake: ${response.type}`
      );
    }

    const desktopAgentId = response.payload.desktopAgentId as string;
    return new BridgeAgentProxy(inner, ws, desktopAgentId);
  }

  // ── Inbound message dispatch ───────────────────────────────────────────────

  private _dispatch(msg: BmpMessage): void {
    switch (msg.type) {
      case "broadcastBridge": {
        const ctx = msg.payload.context as Context | undefined;
        if (!ctx) return;
        const source = msg.meta.source as AppIdentifier | undefined;
        const meta = source ? { source } : undefined;

        // Deliver to typed handlers
        this._contextHandlers.get(ctx.type)?.forEach((h) => h(ctx, meta));
        // Deliver to wildcard handlers
        this._contextHandlers
          .get("__wildcard__")
          ?.forEach((h) => h(ctx, meta));
        break;
      }

      case "raiseIntentBridge": {
        const intent = msg.payload.intent as
          | { name: string }
          | string
          | undefined;
        const intentName =
          typeof intent === "string" ? intent : intent?.name ?? "";
        const ctx = msg.payload.context as Context | undefined;
        if (!ctx || !intentName) return;

        const source = msg.meta.source as AppIdentifier | undefined;
        this._intentHandlers.get(intentName)?.forEach((h) =>
          h(ctx, {
            source: source ?? { appId: "" },
            requestId: msg.meta.requestUuid,
          })
        );
        break;
      }

      case "raiseIntentResponse": {
        const uuid = msg.meta.responseUuid;
        if (!uuid) return;
        const pending = this._pendingIntents.get(uuid);
        if (pending) {
          this._pendingIntents.delete(uuid);
          const data = msg.payload as unknown;
          pending.resolve(data as IntentResolution);
        }
        break;
      }

      case "bridgeError": {
        const uuid = msg.meta.responseUuid;
        if (!uuid) return;
        const pending = this._pendingIntents.get(uuid);
        if (pending) {
          this._pendingIntents.delete(uuid);
          pending.reject(
            new Error(
              (msg.payload.message as string | undefined) ?? "Bridge error"
            )
          );
        }
        break;
      }
    }
  }

  // ── Bridge-aware FDC3 methods ─────────────────────────────────────────────

  /**
   * Broadcasts context to the local broker AND forwards it to all external
   * agents on the same user channel via the bridge.
   */
  async broadcast(context: Context): Promise<void> {
    await this._inner.broadcast(context);

    const channel = await this._inner.getCurrentChannel();
    if (!channel) return;

    const identity = this._inner.getIdentity();
    const bmp: BmpMessage = {
      type: "broadcastRequest",
      meta: makeMeta({
        appId: identity.appId,
        instanceId: identity.instanceId,
        desktopAgent: this.desktopAgentId,
      }),
      payload: {
        channelId: channel.id,
        context,
      },
    };
    this._send(bmp);
  }

  /**
   * Registers a context listener with the local broker AND notifies the bridge
   * so that external agents can deliver matching context to this instance.
   */
  addContextListener(
    contextType: string | null,
    handler: ContextHandler
  ): Listener {
    const innerListener = this._inner.addContextListener(contextType, handler);

    const key = contextType ?? "__wildcard__";
    if (!this._contextHandlers.has(key)) {
      this._contextHandlers.set(key, new Set());
    }
    this._contextHandlers.get(key)!.add(handler);

    this._send({
      type: "addContextListenerRequest",
      meta: makeMeta(),
      payload: { contextType },
    });

    return {
      unsubscribe: () => {
        innerListener.unsubscribe();
        this._contextHandlers.get(key)?.delete(handler);
      },
    };
  }

  /**
   * Registers an intent listener with the local broker AND notifies the bridge
   * so that external agents can raise this intent against this instance.
   */
  addIntentListener(intent: string, handler: IntentHandler): Listener {
    const innerListener = this._inner.addIntentListener(intent, handler);

    if (!this._intentHandlers.has(intent)) {
      this._intentHandlers.set(intent, new Set());
    }
    this._intentHandlers.get(intent)!.add(handler);

    this._send({
      type: "addIntentListenerRequest",
      meta: makeMeta(),
      payload: { intent },
    });

    return {
      unsubscribe: () => {
        innerListener.unsubscribe();
        this._intentHandlers.get(intent)?.delete(handler);
        this._send({
          type: "removeIntentListenerRequest",
          meta: makeMeta(),
          payload: { intent },
        });
      },
    };
  }

  /**
   * Raises an intent. If `target` has a `desktopAgent` field pointing to a
   * different agent, routes the request via the bridge WS. Otherwise delegates
   * to the inner agent for local resolution.
   */
  async raiseIntent(
    intent: string,
    context: Context,
    target?: BridgeAppIdentifier
  ): Promise<IntentResolution> {
    if (target?.desktopAgent && target.desktopAgent !== this.desktopAgentId) {
      return this._raiseBridgeIntent(intent, context, target);
    }
    return this._inner.raiseIntent(intent, context, target);
  }

  async raiseIntentForContext(
    context: Context,
    target?: BridgeAppIdentifier
  ): Promise<IntentResolution> {
    if (target?.desktopAgent && target.desktopAgent !== this.desktopAgentId) {
      return this._raiseBridgeIntent(
        context.type,
        context,
        target
      );
    }
    return this._inner.raiseIntentForContext(context, target);
  }

  private _raiseBridgeIntent(
    intent: string,
    context: Context,
    target?: BridgeAppIdentifier
  ): Promise<IntentResolution> {
    return new Promise<IntentResolution>((resolve, reject) => {
      const requestUuid = randomUuid();
      this._pendingIntents.set(requestUuid, { resolve, reject });

      const identity = this._inner.getIdentity();
      const destination: BmpAgentIdentifier | undefined = target
        ? {
            appId: target.appId,
            instanceId: target.instanceId,
            desktopAgent: target.desktopAgent,
          }
        : undefined;

      this._send({
        type: "raiseIntentRequest",
        meta: {
          requestUuid,
          timestamp: Date.now(),
          source: {
            appId: identity.appId,
            instanceId: identity.instanceId,
            desktopAgent: this.desktopAgentId,
          },
          ...(destination ? { destination } : {}),
        },
        payload: {
          intent: { name: intent },
          context,
          ...(target ? { target } : {}),
        },
      });

      // Reject if no response within 10 s
      setTimeout(() => {
        if (this._pendingIntents.has(requestUuid)) {
          this._pendingIntents.delete(requestUuid);
          reject(new Error(`raiseIntent '${intent}' timed out over bridge`));
        }
      }, 10_000);
    });
  }

  // ── Channel methods (notify bridge of membership changes) ────────────────

  async joinUserChannel(channelId: string): Promise<void> {
    await this._inner.joinUserChannel(channelId);
    this._send({
      type: "joinUserChannelRequest",
      meta: makeMeta(),
      payload: { channelId },
    });
  }

  async leaveCurrentChannel(): Promise<void> {
    await this._inner.leaveCurrentChannel();
    this._send({
      type: "leaveCurrentChannelRequest",
      meta: makeMeta(),
      payload: {},
    });
  }

  // ── Full delegation to inner agent ───────────────────────────────────────

  getIdentity() {
    return this._inner.getIdentity();
  }
  getInfo(): Promise<ImplementationMetadata> {
    return this._inner.getInfo();
  }
  getSystemChannels(): Promise<Channel[]> {
    return this._inner.getSystemChannels();
  }
  getUserChannels(): Promise<Channel[]> {
    return this._inner.getUserChannels();
  }
  getCurrentChannel(): Promise<Channel | null> {
    return this._inner.getCurrentChannel();
  }
  getCurrentContext(contextType?: string): Promise<Context | null> {
    return this._inner.getCurrentContext(contextType);
  }
  getOrCreateChannel(id: string): Promise<Channel> {
    return this._inner.getOrCreateChannel(id);
  }
  createPrivateChannel(): Promise<Channel> {
    return this._inner.createPrivateChannel();
  }
  findIntent(intent: string, contextType?: string): Promise<AppIntent> {
    return this._inner.findIntent(intent, contextType);
  }
  findIntentsForContext(contextType: string): Promise<AppIntent[]> {
    return this._inner.findIntentsForContext(contextType);
  }

  // ── Internal send ────────────────────────────────────────────────────────

  private _send(msg: BmpMessage): void {
    if (this._ws.readyState === WebSocket.OPEN) {
      this._ws.send(JSON.stringify(msg));
    }
  }
}
