import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type {
  ChannelInfo,
  CdaConnectedEvent,
  CdaContextEvent,
  CdaDisconnectedEvent,
  CdaIntentEvent,
  CdaPeerConnectedEvent,
  CdaPeerDisconnectedEvent,
  PeerInfo,
  WindowHandle,
} from "../types";

export function useCdaEvents() {
  const [connections, setConnections] = useState<WindowHandle[]>([]);
  const [channels, setChannels] = useState<ChannelInfo[]>([]);
  const [peers, setPeers] = useState<PeerInfo[]>([]);
  const [activityLog, setActivityLog] = useState<string[]>([]);

  const log = useCallback((msg: string) => {
    const ts = new Date().toISOString().slice(11, 23); // HH:MM:SS.mmm
    setActivityLog((prev) => [`[${ts}] ${msg}`, ...prev].slice(0, 200));
  }, []);

  const refresh = useCallback(async () => {
    const [conns, chans, prs] = await Promise.all([
      invoke<WindowHandle[]>("cda_list_connections"),
      invoke<ChannelInfo[]>("cda_list_channels"),
      invoke<PeerInfo[]>("cda_list_peers"),
    ]);
    setConnections(conns);
    setChannels(chans);
    setPeers(prs);
  }, []);

  useEffect(() => {
    refresh();

    const unlistenAll = Promise.all([
      listen<CdaConnectedEvent>("cda:connected", (e) => {
        refresh();
        log(`connected  ${e.payload.appId} (${e.payload.instanceId.slice(0, 8)}…)`);
      }),
      listen<CdaDisconnectedEvent>("cda:disconnected", (e) => {
        refresh();
        log(`disconnected  ${e.payload.appId} (${e.payload.instanceId.slice(0, 8)}…)`);
      }),
      listen<CdaContextEvent>("cda:context", (e) => {
        refresh();
        const { channelId, sourceAppId, recipientCount } = e.payload;
        const payloadContext = JSON.stringify(e.payload.context);
        log(`broadcast  ${sourceAppId} → ${channelId}  (${recipientCount} recipient${recipientCount !== 1 ? "s" : ""}) >> ${payloadContext}`);
      }),
      listen<CdaIntentEvent>("cda:intent", (e) => {
        const { intent, sourceInstanceId, handlerInstanceId } = e.payload;
        log(`intent  ${intent}  ${sourceInstanceId.slice(0, 8)}… → ${handlerInstanceId.slice(0, 8)}…`);
      }),
      listen<CdaPeerConnectedEvent>("cda:peer_connected", (e) => {
        refresh();
        log(`peer connected  ${e.payload.desktopAgentId} (${e.payload.url})`);
      }),
      listen<CdaPeerDisconnectedEvent>("cda:peer_disconnected", (e) => {
        refresh();
        log(`peer disconnected  ${e.payload.desktopAgentId}`);
      }),
    ]);

    return () => {
      unlistenAll.then((fns) => fns.forEach((fn) => fn()));
    };
  }, [refresh, log]);

  const disconnectSpoke = useCallback(
    async (instanceId: string) => {
      await invoke("cda_disconnect_spoke", { instanceId });
      refresh();
    },
    [refresh],
  );

  return { connections, channels, peers, activityLog, refresh, disconnectSpoke };
}
