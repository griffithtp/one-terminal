import { getCurrentWindow } from "@tauri-apps/api/window";
import { Fdc3Agent } from "./agent";
import type { Fdc3Agent as Fdc3AgentType } from "./agent";
import { BridgeAgentProxy } from "./bridge-agent";

export * from "./types";
export { Fdc3Agent, BridgeAgentProxy };

// ── Augment the global Window type ───────────────────────────────────────────

declare global {
  interface Window {
    fdc3: Fdc3AgentType | BridgeAgentProxy;
    getAgent?: (params?: { appId?: string }) => Promise<Fdc3AgentType | BridgeAgentProxy>;
  }
}

// ── Bootstrap options ─────────────────────────────────────────────────────────

export interface InitFdc3Options {
  /**
   * WebSocket URL for the bridge backplane.
   * @default "ws://127.0.0.1:4000/v2/bridge"
   */
  bridgeUrl?: string;

  /**
   * Set to `false` to skip bridge connection and use local-only IPC.
   * @default true
   */
  connectBridge?: boolean;
}

/**
 * Creates an `Fdc3Agent`, optionally wraps it in a `BridgeAgentProxy`, assigns
 * the result to `window.fdc3`, and returns it. Call once before rendering the
 * React tree.
 *
 * If the bridge WebSocket is available, `window.fdc3` is a `BridgeAgentProxy`
 * that enables cross-agent context broadcasting and intent routing. If the
 * bridge is unavailable, falls back silently to a plain `Fdc3Agent`.
 */
export async function initFdc3(
  appId: string,
  options: InitFdc3Options = {}
): Promise<Fdc3AgentType | BridgeAgentProxy> {
  const { bridgeUrl, connectBridge = true } = options;

  const windowLabel = getCurrentWindow().label;
  const agent = await Fdc3Agent.create(appId, windowLabel);

  if (connectBridge) {
    try {
      const url = bridgeUrl ?? "ws://127.0.0.1:4000/v2/bridge";
      const proxy = await BridgeAgentProxy.connect(agent, url);
      window.fdc3 = proxy;
      console.info(`[fdc3] Bridge connected — desktopAgentId: ${proxy.desktopAgentId}`);
      return proxy;
    } catch (e) {
      console.warn("[fdc3] Bridge unavailable, using local agent only:", e);
    }
  }

  window.fdc3 = agent;
  return agent;
}
