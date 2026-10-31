/**
 * fdc3-plugin.js: FDC3 3.0 for apps running under OneTerminal.
 *
 * Importing this module connects to the Desktop Agent, sets `window.fdc3` and
 * fires the standard `fdc3Ready` event. Apps should call `getAgent()`, either
 * this module's or the one from `@finos/fdc3`, which finds `window.fdc3`.
 *
 * Bus URL, first non-empty wins:
 *   1. <meta name="ot-fdc3-bus-url"> in the host page
 *   2. window.OT_FDC3_AGENT_URL, injected by the Terminal into its panels
 *   3. window.OT_FDC3_BUS_URL
 *   4. ws://localhost:7891/fdc3
 * An injected empty OT_FDC3_AGENT_URL means FDC3 is disabled for this Terminal.
 *
 * App id, first non-empty wins: <meta name="ot-fdc3-app-id">, then the
 * `fdc3AppId` query parameter, then the document title.
 */
import { AgentError, type DesktopAgent } from "@finos/fdc3-standard";
import { connectAgent } from "./agent";
import { createDialogResolver } from "./resolver";

declare global {
  interface Window {
    fdc3?: DesktopAgent;
    /** Internal: how the Terminal reaches this page's agent to change its channel. */
    __otFdc3Ready?: Promise<DesktopAgent | undefined>;
    OT_FDC3_AGENT_URL?: string;
    OT_FDC3_BUS_URL?: string;
    /** Injected by the Terminal: a channel id or null. Undefined outside a Terminal. */
    OT_FDC3_INITIAL_CHANNEL?: string | null;
  }
}

function meta(name: string): string | undefined {
  return document.querySelector<HTMLMetaElement>(`meta[name="${name}"]`)?.content || undefined;
}

function resolveAppId(): string {
  return (
    meta("ot-fdc3-app-id") ||
    new URLSearchParams(location.search).get("fdc3AppId") ||
    document.title ||
    "unknown-app"
  );
}

async function initialize(): Promise<DesktopAgent> {
  if (window.OT_FDC3_AGENT_URL === "" && !meta("ot-fdc3-bus-url")) {
    throw new Error(
      'FDC3 is disabled for this Terminal (fdc3.agentUrl is ""). Set fdc3.agentUrl in ' +
        "terminal.config.json or OT_FDC3_AGENT_URL to a ws:// URL, or remove the key to " +
        "use the bundled Desktop Agent."
    );
  }
  const url =
    meta("ot-fdc3-bus-url") ||
    window.OT_FDC3_AGENT_URL ||
    window.OT_FDC3_BUS_URL ||
    "ws://localhost:7891/fdc3";

  const { agent, messaging } = await connectAgent({
    url,
    appId: resolveAppId(),
    intentResolver: createDialogResolver(),
  });

  // Rejoin before announcing readiness, so a reload or dashboard switch doesn't
  // drop the panel off its channel and apps see the channel straight away.
  if (window.OT_FDC3_INITIAL_CHANNEL) {
    await agent.joinUserChannel(window.OT_FDC3_INITIAL_CHANNEL).catch((e: unknown) => {
      console.warn("[fdc3-plugin] Could not rejoin initial channel:", e);
    });
  }

  window.addEventListener("pagehide", (event) => {
    if (!event.persisted) void messaging.disconnect();
  });
  return agent;
}

const ready = initialize();

window.__otFdc3Ready = ready.then(
  (agent) => {
    window.fdc3 = agent;
    window.dispatchEvent(new Event("fdc3Ready"));
    return agent;
  },
  (e: unknown) => {
    console.warn("[fdc3-plugin] Could not connect to the Desktop Agent:", e);
    return undefined;
  }
);

/**
 * Resolves with this page's agent once connected. Rejects with
 * `AgentError.AgentNotFound`, with the reason in `cause`, if it couldn't connect.
 */
export async function getAgent(): Promise<DesktopAgent> {
  try {
    await window.__otFdc3Ready;
    return await ready;
  } catch (cause) {
    throw new Error(AgentError.AgentNotFound, { cause });
  }
}
