/**
 * useFdcBus — CDA-side bridge between the fdc3-plugin BroadcastChannel and
 * the Tauri backend.
 *
 * Responsibilities
 * ────────────────
 * 1. Receive inbound `fdc3:*` messages from launched app windows.
 * 2. Bridge them to Tauri invoke() calls (broadcast, raiseIntent, etc.).
 * 3. Listen for Tauri push events (cda:context, cda:intent) and relay
 *    them back to app windows via the BroadcastChannel.
 * 4. Track bus-connected app instances: channel membership and intent listeners.
 *
 * Mount this hook once at the App root — it is always active.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useRef } from "react";
import {
  FdcBus,
  type BusAddIntentListener,
  type BusBroadcast,
  type BusGoodbye,
  type BusHello,
  type BusJoinChannel,
  type BusLeaveChannel,
  type BusOpen,
  type BusRaiseIntent,
  type BusRemoveIntentListener,
  type InboundBusMessage,
  type OutboundBusMessage,
} from "../bus";
import type { CdaContextEvent, CdaIntentEvent, ChannelInfo } from "../types";

// ── Internal state types ───────────────────────────────────────────────────────

interface BusInstance {
  appId: string;
  instanceId: string;
  currentChannelId: string | null;
  /** Intent names this instance has registered as a handler. */
  intents: Set<string>;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useFdcBus() {
  /**
   * Map of bus instanceId → BusInstance.
   * Stored in a ref so handler closures always see the latest state without
   * triggering React re-renders.
   */
  const instances = useRef<Map<string, BusInstance>>(new Map());

  useEffect(() => {
    const bus = new FdcBus(handleInbound);

    // ── Tauri event relays ───────────────────────────────────────────────────

    // Relay context broadcasts from the backend to all bus app windows
    const unlistenContext = listen<CdaContextEvent>("cda:context", (event) => {
      const { channelId, context, sourceAppId } = event.payload;
      bus.publish({
        type: "fdc3:contextBroadcast",
        channelId,
        context,
        sourceAppId,
      });
    });

    // Relay intent deliveries to the correct bus app window
    const unlistenIntent = listen<CdaIntentEvent>("cda:intent", (event) => {
      const { intent, handlerInstanceId, sourceInstanceId, requestId } = event.payload;
      // Only relay if the handler is a bus-connected instance
      if (instances.current.has(handlerInstanceId)) {
        bus.publish({
          type: "fdc3:intentDelivery",
          intent,
          context: {}, // backend event does not carry context; full delivery comes from TCP
          sourceInstanceId,
          targetInstanceId: handlerInstanceId,
          deliveryId: requestId,
        });
      }
    });

    // ── Inbound message handler ─────────────────────────────────────────────

    function handleInbound(msg: InboundBusMessage) {
      switch (msg.type) {
        case "fdc3:hello":
          return onHello(msg, bus);
        case "fdc3:broadcast":
          return onBroadcast(msg);
        case "fdc3:raiseIntent":
          return onRaiseIntent(msg, bus);
        case "fdc3:joinChannel":
          return onJoinChannel(msg, bus);
        case "fdc3:leaveChannel":
          return onLeaveChannel(msg);
        case "fdc3:addIntentListener":
          return onAddIntentListener(msg);
        case "fdc3:removeIntentListener":
          return onRemoveIntentListener(msg);
        case "fdc3:open":
          return onOpen(msg, bus);
        case "fdc3:goodbye":
          return onGoodbye(msg);
      }
    }

    // ── Handler implementations ─────────────────────────────────────────────

    function onHello(msg: BusHello, bus: FdcBus) {
      const instanceId = `bus-${crypto.randomUUID()}`;
      instances.current.set(instanceId, {
        appId: msg.appId,
        instanceId,
        currentChannelId: null,
        intents: new Set(),
      });

      // Fetch the current channel list to include in the welcome
      invoke<ChannelInfo[]>("cda_list_channels")
        .then((channels) => {
          const reply: OutboundBusMessage = {
            type: "fdc3:welcome",
            instanceId,
            channels: channels.map((c) => ({
              channelId: c.channelId,
              displayName: c.displayName,
              color: c.color,
            })),
            requestId: msg.requestId,
          };
          bus.publish(reply);
        })
        .catch((e) => {
          bus.publish({
            type: "fdc3:error",
            code: "HandshakeFailed",
            message: String(e),
            requestId: msg.requestId,
          });
        });
    }

    function onBroadcast(msg: BusBroadcast) {
      // Fire-and-forget — no reply needed
      invoke("cda_broadcast_from_dashboard", {
        channelId: msg.channelId,
        context: msg.context,
      }).catch((e) => console.error("[cda bus] broadcast error", e));
    }

    function onRaiseIntent(msg: BusRaiseIntent, bus: FdcBus) {
      invoke<string>("cda_raise_intent_from_dashboard", {
        intent: msg.intent,
        context: msg.context,
        targetInstanceId: msg.targetInstanceId,
      })
        .then((requestId) => {
          // Best-effort resolution info — full handler details come via cda:intent event
          bus.publish({
            type: "fdc3:intentResolved",
            intent: msg.intent,
            handlerAppId: "unknown",
            handlerInstanceId: requestId,
            requestId: msg.requestId,
          });
        })
        .catch((e) => {
          bus.publish({
            type: "fdc3:error",
            code: "NoIntentHandlers",
            message: String(e),
            requestId: msg.requestId,
          });
        });
    }

    function onJoinChannel(msg: BusJoinChannel, bus: FdcBus) {
      const inst = instances.current.get(msg.instanceId);
      if (inst) inst.currentChannelId = msg.channelId;

      bus.publish({
        type: "fdc3:channelJoined",
        channelId: msg.channelId,
        requestId: msg.requestId,
      });
    }

    function onLeaveChannel(msg: BusLeaveChannel) {
      const inst = instances.current.get(msg.instanceId);
      if (inst) inst.currentChannelId = null;
    }

    function onAddIntentListener(msg: BusAddIntentListener) {
      const inst = instances.current.get(msg.instanceId);
      inst?.intents.add(msg.intent);
    }

    function onRemoveIntentListener(msg: BusRemoveIntentListener) {
      const inst = instances.current.get(msg.instanceId);
      inst?.intents.delete(msg.intent);
    }

    function onOpen(msg: BusOpen, bus: FdcBus) {
      // Resolve appId → URL + type via the app directory cache, then launch
      invoke<
        { appId: string; type: string; name: string; title?: string; details?: { url?: string } }[]
      >("cda_list_app_directory")
        .then((apps) => {
          const app = apps.find((a) => a.appId === msg.appId);
          const url = app?.details?.url;
          if (!url) throw new Error(`App '${msg.appId}' has no launch URL`);
          return invoke("cda_launch_app", {
            appId: app.appId,
            appUrl: url,
            appType: app.type,
            appTitle: app.title ?? app.name,
          });
        })
        .then(() => {
          bus.publish({
            type: "fdc3:openResult",
            appId: msg.appId,
            requestId: msg.requestId,
          });
        })
        .catch((e) => {
          bus.publish({
            type: "fdc3:error",
            code: "AppNotFound",
            message: String(e),
            requestId: msg.requestId,
          });
        });
    }

    function onGoodbye(msg: BusGoodbye) {
      instances.current.delete(msg.instanceId);
    }

    // ── Cleanup ─────────────────────────────────────────────────────────────

    return () => {
      bus.destroy();
      unlistenContext.then((fn) => fn());
      unlistenIntent.then((fn) => fn());
    };
  }, []);
}
