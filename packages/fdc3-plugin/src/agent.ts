import {
  DefaultAppSupport,
  DefaultChannelSupport,
  DefaultHeartbeatSupport,
  DefaultIntentSupport,
  DesktopAgentProxy,
} from "@finos/fdc3-agent-proxy";
import { LogLevel, type AppIdentifier } from "@finos/fdc3-standard";
import { newUuid, WsMessaging } from "./WsMessaging";

/** Replies from the Desktop Agent itself. */
const MESSAGE_EXCHANGE_TIMEOUT_MS = 10_000;
/** Must exceed the Desktop Agent's own launch timeout, so its specific error wins. */
const APP_LAUNCH_TIMEOUT_MS = 25_000;

type ChannelSupportArgs = ConstructorParameters<typeof DefaultChannelSupport>;
type IntentSupportArgs = ConstructorParameters<typeof DefaultIntentSupport>;
export type IntentResolver = IntentSupportArgs[1];

/** The Terminal owns channel selection UI, so the proxy needs no selector. */
const noChannelSelector: ChannelSupportArgs[1] = {
  async updateChannel() {},
  setChannelChangeCallback() {},
  async connect() {},
  async disconnect() {},
};

export interface ConnectOptions {
  url: string;
  appId: string;
  intentResolver: IntentResolver;
}

export interface ConnectedAgent {
  agent: DesktopAgentProxy;
  messaging: WsMessaging;
}

function openSocket(url: string, appId: string): Promise<WebSocket> {
  const target = new URL(url);
  target.searchParams.set("appId", appId);
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(target);
    socket.addEventListener("open", () => resolve(socket), { once: true });
    socket.addEventListener(
      "error",
      () => reject(new Error(`WebSocket connection to ${url} failed: is the Desktop Agent running?`)),
      { once: true }
    );
  });
}

/** Validate this app's identity, returning the instance the agent assigned it. */
function validateIdentity(socket: WebSocket): Promise<AppIdentifier> {
  const connectionAttemptUuid = newUuid();
  const href = globalThis.location?.href ?? "about:blank";
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      socket.removeEventListener("message", onMessage);
      reject(new Error("Timed out validating identity with the Desktop Agent"));
    }, MESSAGE_EXCHANGE_TIMEOUT_MS);

    function onMessage(event: MessageEvent) {
      const message = JSON.parse(event.data);
      if (message.meta?.connectionAttemptUuid !== connectionAttemptUuid) return;
      clearTimeout(timer);
      socket.removeEventListener("message", onMessage);
      if (message.type === "WCP5ValidateAppIdentityResponse") {
        resolve({ appId: message.payload.appId, instanceId: message.payload.instanceId });
      } else {
        reject(new Error(message.payload?.message ?? "Identity validation failed"));
      }
    }

    socket.addEventListener("message", onMessage);
    socket.send(
      JSON.stringify({
        type: "WCP4ValidateAppIdentity",
        meta: { connectionAttemptUuid, timestamp: new Date() },
        payload: { identityUrl: href, actualUrl: href },
      })
    );
  });
}

/** Connect to a Desktop Agent and return a ready FDC3 3.0 `DesktopAgent`. */
export async function connectAgent(options: ConnectOptions): Promise<ConnectedAgent> {
  const socket = await openSocket(options.url, options.appId);
  const identity = await validateIdentity(socket);
  const messaging = new WsMessaging(socket, identity);

  const heartbeat = new DefaultHeartbeatSupport(messaging);
  const channels = new DefaultChannelSupport(
    messaging,
    noChannelSelector,
    MESSAGE_EXCHANGE_TIMEOUT_MS
  );
  const intents = new DefaultIntentSupport(
    messaging,
    options.intentResolver,
    MESSAGE_EXCHANGE_TIMEOUT_MS,
    APP_LAUNCH_TIMEOUT_MS
  );
  const apps = new DefaultAppSupport(messaging, MESSAGE_EXCHANGE_TIMEOUT_MS, APP_LAUNCH_TIMEOUT_MS);
  const agent = new DesktopAgentProxy(
    heartbeat,
    channels,
    intents,
    apps,
    [heartbeat, channels, options.intentResolver],
    LogLevel.WARN
  );
  await agent.connect();
  return { agent, messaging };
}
