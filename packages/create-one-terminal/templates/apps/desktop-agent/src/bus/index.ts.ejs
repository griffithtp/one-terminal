/**
 * FDC3 CDA Bus — BroadcastChannel bridge between launched web apps and the CDA.
 *
 * Apps loaded inside Tauri webview windows post FdcBusMessage objects on the
 * shared "fdc3-cda-bus" BroadcastChannel.  The CDA frontend (useFdcBus hook)
 * subscribes to these messages and bridges them to the Tauri backend via
 * invoke().  Replies travel back on the same channel, correlated by requestId.
 *
 *   launched app → BroadcastChannel → CDA frontend → invoke()  (request path)
 *   Tauri event  → CDA frontend → BroadcastChannel → app       (push path)
 */

// ── Inbound messages (app → CDA hub) ─────────────────────────────────────────

/** Sent once per app window to register with the CDA and receive a welcome. */
export interface BusHello {
  type: "fdc3:hello";
  appId: string;
  requestId: string;
}

/** Broadcast a context on the caller's current channel. */
export interface BusBroadcast {
  type: "fdc3:broadcast";
  instanceId: string;
  channelId: string;
  context: unknown;
}

/** Raise an intent, optionally targeting a specific spoke instance. */
export interface BusRaiseIntent {
  type: "fdc3:raiseIntent";
  instanceId: string;
  intent: string;
  context: unknown;
  targetInstanceId: string | null;
  requestId: string;
}

/** Join a named CDA user channel. */
export interface BusJoinChannel {
  type: "fdc3:joinChannel";
  instanceId: string;
  channelId: string;
  requestId: string;
}

/** Leave the current channel. */
export interface BusLeaveChannel {
  type: "fdc3:leaveChannel";
  instanceId: string;
}

/** Register this window as a handler for a named intent. */
export interface BusAddIntentListener {
  type: "fdc3:addIntentListener";
  instanceId: string;
  intent: string;
}

/** Deregister an intent listener. */
export interface BusRemoveIntentListener {
  type: "fdc3:removeIntentListener";
  instanceId: string;
  intent: string;
}

/** Ask the CDA to launch an App Directory app by appId. */
export interface BusOpen {
  type: "fdc3:open";
  instanceId: string;
  appId: string;
  context: unknown;
  requestId: string;
}

/** Graceful disconnect notification. */
export interface BusGoodbye {
  type: "fdc3:goodbye";
  instanceId: string;
}

export type InboundBusMessage =
  | BusHello
  | BusBroadcast
  | BusRaiseIntent
  | BusJoinChannel
  | BusLeaveChannel
  | BusAddIntentListener
  | BusRemoveIntentListener
  | BusOpen
  | BusGoodbye;

// ── Outbound messages (CDA hub → app) ────────────────────────────────────────

/** Lightweight channel descriptor included in the welcome payload. */
export interface ChannelSummary {
  channelId: string;
  displayName: string;
  color: string;
}

/** Reply to fdc3:hello — carries the assigned instanceId and channel list. */
export interface BusWelcome {
  type: "fdc3:welcome";
  instanceId: string;
  channels: ChannelSummary[];
  requestId: string;
}

/** Reply to fdc3:joinChannel. */
export interface BusChannelJoined {
  type: "fdc3:channelJoined";
  channelId: string;
  requestId: string;
}

/** Reply to fdc3:raiseIntent — successful resolution. */
export interface BusIntentResolved {
  type: "fdc3:intentResolved";
  intent: string;
  handlerAppId: string;
  handlerInstanceId: string;
  requestId: string;
}

/** Reply to fdc3:open — carries the opened app's identity. */
export interface BusOpenResult {
  type: "fdc3:openResult";
  appId: string;
  requestId: string;
}

/** Generic error reply — echoes the requestId of the failed request. */
export interface BusError {
  type: "fdc3:error";
  code: string;
  message: string;
  requestId: string;
}

/**
 * Push: context broadcast delivered to all bus apps on the originating channel.
 * No requestId — routed by context type to registered addContextListener handlers.
 */
export interface BusContextBroadcast {
  type: "fdc3:contextBroadcast";
  channelId: string;
  context: unknown;
  sourceAppId: string;
}

/**
 * Push: intent delivery addressed to a specific bus-connected instance.
 * The plugin checks `targetInstanceId` to avoid delivering to other windows.
 */
export interface BusIntentDelivery {
  type: "fdc3:intentDelivery";
  intent: string;
  context: unknown;
  sourceInstanceId: string;
  targetInstanceId: string;
  deliveryId: string;
}

export type OutboundBusMessage =
  | BusWelcome
  | BusChannelJoined
  | BusIntentResolved
  | BusOpenResult
  | BusError
  | BusContextBroadcast
  | BusIntentDelivery;

export type BusMessage = InboundBusMessage | OutboundBusMessage;

// ── FdcBus class ─────────────────────────────────────────────────────────────

export type InboundBusHandler = (msg: InboundBusMessage) => void;

const INBOUND_TYPES = new Set<string>([
  "fdc3:hello",
  "fdc3:broadcast",
  "fdc3:raiseIntent",
  "fdc3:joinChannel",
  "fdc3:leaveChannel",
  "fdc3:addIntentListener",
  "fdc3:removeIntentListener",
  "fdc3:open",
  "fdc3:goodbye",
]);

export class FdcBus {
  private channel: BroadcastChannel;
  private handler: InboundBusHandler;

  constructor(handler: InboundBusHandler) {
    this.channel = new BroadcastChannel("fdc3-cda-bus");
    this.handler = handler;
    this.channel.onmessage = (ev: MessageEvent) => {
      const data = ev.data as Record<string, unknown> | null;
      if (data && typeof data.type === "string" && INBOUND_TYPES.has(data.type)) {
        this.handler(data as unknown as InboundBusMessage);
      }
    };
  }

  /** Publish an outbound message to all bus listeners (app windows). */
  publish(msg: OutboundBusMessage): void {
    this.channel.postMessage(msg);
  }

  /** Close the BroadcastChannel. Call when the CDA frontend unmounts. */
  destroy(): void {
    this.channel.close();
  }
}
