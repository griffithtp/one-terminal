import { AbstractMessaging } from "@finos/fdc3-agent-proxy";
import type { AppIdentifier } from "@finos/fdc3-standard";

type Messaging = InstanceType<typeof AbstractMessaging>;
type Message = Parameters<Messaging["post"]>[0];
type Listener = Parameters<Messaging["register"]>[0];

export function newUuid(): string {
  // randomUUID only exists in secure contexts; apps may be served over plain http.
  if (typeof crypto?.randomUUID === "function") return crypto.randomUUID();
  return `${Math.random().toString(36).slice(2)}-${Date.now().toString(36)}`;
}

/**
 * DACP timestamps are strings on the wire, but FDC3 `ContextMetadata.timestamp`
 * is a `Date` and the proxy passes the value straight through to app handlers.
 */
function reviveTimestamps(message: Record<string, any>): void {
  for (const holder of [
    message.meta,
    message.payload?.metadata,
    message.payload?.resultMetadata,
  ]) {
    if (typeof holder?.timestamp === "string") holder.timestamp = new Date(holder.timestamp);
  }
}

/** Connects the official `DesktopAgentProxy` to the Desktop Agent over one WebSocket. */
export class WsMessaging extends AbstractMessaging {
  private readonly listeners = new Map<string, Listener>();

  constructor(
    private readonly socket: WebSocket,
    appIdentifier: AppIdentifier
  ) {
    super(appIdentifier);
    socket.addEventListener("message", (event) => this.onMessage(event));
  }

  private onMessage(event: MessageEvent): void {
    let message: Record<string, any>;
    try {
      message = JSON.parse(event.data);
    } catch (e) {
      console.error("[fdc3-plugin] Unparseable message from Desktop Agent:", e);
      return;
    }
    reviveTimestamps(message);
    // Copy first: a listener's action may unregister it.
    for (const listener of [...this.listeners.values()]) {
      if (listener.filter(message as never)) listener.action(message as never);
    }
  }

  createUUID(): string {
    return newUuid();
  }

  async post(message: Message): Promise<void> {
    if (this.socket.readyState === WebSocket.OPEN) this.socket.send(JSON.stringify(message));
  }

  register(listener: Listener): void {
    if (listener.id == null) throw new Error("Listener has no id");
    this.listeners.set(listener.id, listener);
  }

  unregister(id: string): void {
    this.listeners.delete(id);
  }

  createMeta(): ReturnType<Messaging["createMeta"]> {
    return {
      requestUuid: newUuid(),
      timestamp: new Date(),
      source: this.getAppIdentifier(),
    };
  }

  async disconnect(): Promise<void> {
    await this.post({
      type: "WCP6Goodbye",
      meta: { timestamp: new Date() },
    } as Message);
    this.socket.close();
  }
}
