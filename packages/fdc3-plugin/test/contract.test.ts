// Drives the official DesktopAgentProxy against the real Rust FDC3 bus, and
// checks every frame in both directions against the published 3.0 schemas.
import type { Channel, Context, DesktopAgent, PrivateChannel } from "@finos/fdc3-standard";
import { afterAll, afterEach, beforeAll, describe, expect, inject, it, vi } from "vitest";
import { connectAgent, type ConnectedAgent, type IntentResolver } from "../src/agent";
import { schemaErrors } from "./schemas";

const instrument: Context = { type: "fdc3.instrument", id: { ticker: "AAPL" } };
const contact: Context = { type: "fdc3.contact", id: { email: "a@example.com" } };

// ── Frame capture ──────────────────────────────────────────────────────────────

const frames: { direction: "sent" | "received"; message: { type: string } }[] = [];
const NativeWebSocket = globalThis.WebSocket;

class RecordingWebSocket extends NativeWebSocket {
  private queue: Promise<void> = Promise.resolve();

  constructor(url: string | URL) {
    super(url);
    super.addEventListener("message", (e) =>
      frames.push({ direction: "received", message: JSON.parse(e.data) })
    );
  }

  // Browsers deliver each WebSocket message as its own task, so promise
  // continuations run between messages, and the proxy relies on that to register
  // a listener before the next frame. Node can dispatch several frames from one
  // chunk synchronously, so restore the browser's behaviour here.
  override addEventListener(type: string, listener: never, options?: never) {
    if (type !== "message") return super.addEventListener(type, listener, options);
    super.addEventListener(type, ((e: MessageEvent) => {
      this.queue = this.queue
        .then(() => new Promise<void>((r) => setImmediate(r)))
        .then(() => (listener as (e: MessageEvent) => void)(e));
    }) as never, options);
  }
  override send(data: string) {
    frames.push({ direction: "sent", message: JSON.parse(data) });
    super.send(data);
  }
}

beforeAll(() => {
  vi.stubGlobal("WebSocket", RecordingWebSocket);
});

afterAll(() => {
  vi.unstubAllGlobals();
  const failures = frames.flatMap(({ direction, message }) => {
    const errors = schemaErrors(message);
    return errors ? [`${direction} ${JSON.stringify(message)}\n  ${errors}`] : [];
  });
  if (failures.length) {
    console.error(failures.join("\n"));
  }
  expect(failures).toEqual([]);
  expect(frames.length).toBeGreaterThan(50);
});

// ── Helpers ────────────────────────────────────────────────────────────────────

const connections: ConnectedAgent[] = [];

/** A resolver that fails the test if it's reached, unless a test replaces it. */
const unexpectedResolver: IntentResolver = {
  async connect() {},
  async disconnect() {},
  async chooseIntent() {
    throw new Error("resolver was not expected to run");
  },
};

async function connect(appId: string, intentResolver = unexpectedResolver): Promise<DesktopAgent> {
  const connection = await connectAgent({ url: inject("busUrl"), appId, intentResolver });
  connections.push(connection);
  return connection.agent;
}

afterEach(async () => {
  await Promise.all(connections.splice(0).map((c) => c.messaging.disconnect()));
  // Let the agent process the disconnects, so state doesn't leak between tests.
  await new Promise((r) => setTimeout(r, 30));
});

function received<T>(): { handler: (value: T, ...rest: unknown[]) => void; next: Promise<T> } {
  let resolve!: (value: T) => void;
  const next = new Promise<T>((r) => (resolve = r));
  return { handler: (value) => resolve(value), next };
}

const settle = () => new Promise((r) => setTimeout(r, 50));

// ── Tests ──────────────────────────────────────────────────────────────────────

describe("schema validation", () => {
  it("rejects invalid messages, so passing frames mean something", () => {
    const meta = { requestUuid: "a", responseUuid: "b", timestamp: "2026-10-31T00:00:00.000Z" };
    const type = "getCurrentChannelResponse";
    expect(schemaErrors({ type, meta, payload: { channel: null } } as never)).toBeNull();
    expect(schemaErrors({ type, payload: { channel: null } } as never)).not.toBeNull();
    expect(schemaErrors({ type, meta, payload: { channel: 5 } } as never)).not.toBeNull();
    expect(schemaErrors({ type, meta, payload: { error: "AccessDenied" } } as never)).toBeNull();
    expect(schemaErrors({ type, meta, payload: { error: "Whatever" } } as never)).not.toBeNull();
    expect(schemaErrors({ type: "madeUp" })).not.toBeNull();
  });
});

describe("user channels", () => {
  it("lists, joins and leaves channels", async () => {
    const app = await connect("app");
    expect((await app.getUserChannels()).map((c) => c.id)).toEqual(["Red", "Blue"]);
    expect((await app.getUserChannels())[0].displayMetadata?.color).toBe("#E74C3C");
    expect(await app.getCurrentChannel()).toBeNull();

    await app.joinUserChannel("Red");
    expect((await app.getCurrentChannel())?.id).toBe("Red");
    await app.leaveCurrentChannel();
    expect(await app.getCurrentChannel()).toBeNull();
    await expect(app.joinUserChannel("Nope")).rejects.toThrow("NoChannelFound");
  });

  it("delivers broadcasts with metadata to listeners on the same channel only", async () => {
    const [sender, same, other] = await Promise.all([connect("s"), connect("a"), connect("b")]);
    await sender.joinUserChannel("Red");
    await same.joinUserChannel("Red");
    await other.joinUserChannel("Blue");
    const hit = vi.fn();
    const { handler, next } = received<Context>();
    const metadataSeen = vi.fn();
    await same.addContextListener(null, (c, m) => {
      metadataSeen(m);
      handler(c);
    });
    await other.addContextListener(null, hit);

    await sender.broadcast(instrument, { traceId: "trace-1" });

    expect(await next).toEqual(instrument);
    const metadata = metadataSeen.mock.calls[0][0];
    expect(metadata.source.appId).toBe("s");
    expect(metadata.traceId).toBe("trace-1");
    expect(metadata.timestamp).toBeInstanceOf(Date);
    await settle();
    expect(hit).not.toHaveBeenCalled();
  });

  it("filters by context type, including the array form", async () => {
    const [sender, app] = await Promise.all([connect("s"), connect("a")]);
    await sender.joinUserChannel("Red");
    await (await sender.getCurrentChannel())!.clearContext();
    await app.joinUserChannel("Red");
    const single = vi.fn();
    const array = vi.fn();
    await app.addContextListener("fdc3.contact", single);
    await app.addContextListener(["fdc3.instrument", "fdc3.contact"], array);

    await sender.broadcast(instrument);
    await settle();

    expect(single).not.toHaveBeenCalled();
    expect(array).toHaveBeenCalledOnce();
  });

  it("replays current context on join and stops delivery after unsubscribe", async () => {
    const [sender, app] = await Promise.all([connect("s"), connect("a")]);
    await sender.joinUserChannel("Red");
    await (await sender.getCurrentChannel())!.clearContext();
    await sender.broadcast(instrument);
    const seen = vi.fn();
    const listener = await app.addContextListener(null, seen);

    await app.joinUserChannel("Red");
    await settle();
    expect(seen).toHaveBeenCalledOnce();
    expect(seen.mock.calls[0][0]).toEqual(instrument);

    await listener.unsubscribe();
    await sender.broadcast(contact);
    await settle();
    expect(seen).toHaveBeenCalledOnce();
  });

  it("doesn't throw when broadcasting without a channel", async () => {
    const app = await connect("a");
    await expect(app.broadcast(instrument)).resolves.toBeUndefined();
  });

  it("reports userChannelChanged", async () => {
    const app = await connect("a");
    const { handler, next } = received<{ type: string; details: { currentChannelId: string } }>();
    await app.addEventListener("userChannelChanged", handler as never);
    await app.joinUserChannel("Blue");
    const event = await next;
    expect(event.type).toBe("userChannelChanged");
    expect(event.details.currentChannelId).toBe("Blue");
  });
});

describe("channel context", () => {
  it("stores context by type and clears it", async () => {
    const app = await connect("a");
    const channel = await app.getOrCreateChannel("contract-history");
    await channel.broadcast(instrument);
    await channel.broadcast(contact);

    expect(await channel.getCurrentContext()).toEqual(contact);
    expect(await channel.getCurrentContext("fdc3.instrument")).toEqual(instrument);
    const withMetadata = await (
      channel as Channel & {
        getCurrentContextWithMetadata(t?: string): Promise<{ metadata: { traceId: string } } | null>;
      }
    ).getCurrentContextWithMetadata("fdc3.contact");
    expect(withMetadata?.metadata.traceId).toBeTruthy();

    await channel.clearContext("fdc3.instrument");
    expect(await channel.getCurrentContext("fdc3.instrument")).toBeNull();
    expect(await channel.getCurrentContext("fdc3.contact")).toEqual(contact);
    await channel.clearContext();
    expect(await channel.getCurrentContext()).toBeNull();
  });

  it("delivers contextCleared events, and app channels keep their scope", async () => {
    const [a, b] = await Promise.all([connect("a"), connect("b")]);
    const channelA = await a.getOrCreateChannel("contract-clear");
    const channelB = await b.getOrCreateChannel("contract-clear");
    expect(channelB.type).toBe("app");
    const { handler, next } = received<{ details: { channelId: string; contextType: string | null } }>();
    await channelB.addEventListener("contextCleared", handler as never);
    const contextSeen = received<Context>();
    await channelB.addContextListener(null, contextSeen.handler);

    await channelA.broadcast(instrument);
    expect(await contextSeen.next).toEqual(instrument);
    await channelA.clearContext("fdc3.instrument");

    expect((await next).details).toEqual({
      channelId: "contract-clear",
      contextType: "fdc3.instrument",
    });
  });
});

describe("private channels", () => {
  it("connects a handler and raiser, with lifecycle events and access control", async () => {
    const [raiser, handler, outsider] = await Promise.all([
      connect("raiser"),
      connect("handler"),
      connect("outsider"),
    ]);
    let channel!: PrivateChannel;
    await handler.addIntentListener("OpenPrivate", async () => {
      channel = await handler.createPrivateChannel();
      return channel;
    });

    const resolution = await raiser.raiseIntent("OpenPrivate", instrument);
    const result = (await resolution.getResult()) as PrivateChannel;
    expect(result.type).toBe("private");
    expect(result.id).toBe(channel.id);

    await expect(outsider.getOrCreateChannel(channel.id)).rejects.toThrow("AccessDenied");

    const added = received<{ details: { contextType: string } }>();
    await channel.addEventListener("addContextListener", added.handler as never);
    const unsubscribed = received<unknown>();
    await channel.addEventListener("unsubscribe", unsubscribed.handler);
    const disconnected = received<unknown>();
    await channel.addEventListener("disconnect", disconnected.handler);

    const seen = received<Context>();
    await result.addContextListener("fdc3.instrument", seen.handler);
    expect((await added.next).details.contextType).toBe("fdc3.instrument");
    await channel.broadcast(instrument);
    expect(await seen.next).toEqual(instrument);

    await result.disconnect();
    await unsubscribed.next;
    await disconnected.next;
    await expect(result.broadcast(instrument)).rejects.toThrow("AccessDenied");
  });
});

describe("intents", () => {
  it("raises to the single running handler and returns a context result", async () => {
    const [raiser, handler] = await Promise.all([connect("raiser"), connect("handler")]);
    await handler.addIntentListener("DoThing", async (context, metadata) => {
      expect(context).toEqual(instrument);
      expect(metadata?.source?.appId).toBe("raiser");
      return contact;
    });

    const resolution = await raiser.raiseIntent("DoThing", instrument);

    expect(resolution.intent).toBe("DoThing");
    expect(resolution.source.appId).toBe("handler");
    expect(await resolution.getResult()).toEqual(contact);
    const metadata = await resolution.getResultMetadata();
    expect(metadata.source?.appId).toBe("handler");
    expect(metadata.timestamp).toBeInstanceOf(Date);
  });

  it("supports void results and fdc3.nothing for an omitted context", async () => {
    const [raiser, handler] = await Promise.all([connect("raiser"), connect("handler")]);
    const seen = vi.fn();
    await handler.addIntentListener("DoThing", (context) => {
      seen(context);
    });
    const resolution = await raiser.raiseIntent("DoThing");
    expect(await resolution.getResult()).toBeUndefined();
    expect(seen.mock.calls[0][0].type).toBe("fdc3.nothing");
  });

  it("uses the resolver when several handlers exist, and honours cancelling", async () => {
    const chooseIntent = vi.fn();
    const raiser = await connect("raiser", { ...unexpectedResolver, chooseIntent });
    const [one, two] = await Promise.all([connect("one"), connect("two")]);
    const hitOne = vi.fn();
    await one.addIntentListener("Multi", hitOne);
    await two.addIntentListener("Multi", vi.fn());
    const oneId = (await one.getInfo()).appMetadata.instanceId;

    chooseIntent.mockResolvedValueOnce({
      intent: "Multi",
      appId: { appId: "one", instanceId: oneId },
    });
    const resolution = await raiser.raiseIntent("Multi", instrument);
    expect(resolution.source.instanceId).toBe(oneId);
    expect(chooseIntent.mock.calls[0][0][0].apps).toHaveLength(2);
    await settle();
    expect(hitOne).toHaveBeenCalledOnce();

    chooseIntent.mockResolvedValueOnce(undefined);
    await expect(raiser.raiseIntent("Multi", instrument)).rejects.toThrow("UserCancelled");
  });

  it("launches a directory app and delivers once its listener registers", async () => {
    const raiser = await connect("raiser");
    const pending = raiser.raiseIntent("ViewChart", instrument);
    await settle();

    const chart = await connect("chart");
    const { handler, next } = received<Context>();
    await chart.addIntentListener("ViewChart", handler);

    expect(await next).toEqual(instrument);
    expect((await pending).source.appId).toBe("chart");
  });

  it("rejects with the specified errors", async () => {
    const raiser = await connect("raiser");
    await expect(raiser.raiseIntent("NoSuchIntent", instrument)).rejects.toThrow("NoAppsFound");
    await expect(
      raiser.raiseIntent("ViewChart", instrument, { appId: "nope" })
    ).rejects.toThrow("TargetAppUnavailable");
    await expect(
      raiser.raiseIntent("ViewChart", instrument, { appId: "chart", instanceId: "gone" })
    ).rejects.toThrow("TargetInstanceUnavailable");
    await expect(
      raiser.raiseIntent("ViewChart", instrument, null, false)
    ).rejects.toThrow("TargetInstanceUnavailable");
  });

  it("rejects a conflicting intent listener", async () => {
    const app = await connect("a");
    await app.addIntentListener("Dup", vi.fn());
    await expect(app.addIntentListener("Dup", vi.fn())).rejects.toThrow("IntentListenerConflict");
    await app.addIntentListenerWithContext("Filtered", "fdc3.contact", vi.fn());
    await expect(
      app.addIntentListenerWithContext("Filtered", ["fdc3.instrument"], vi.fn())
    ).resolves.toBeDefined();
  });

  it("finds intents by name and by context, and raises for context", async () => {
    const [asker, chart] = await Promise.all([connect("asker"), connect("chart")]);
    await chart.addIntentListener("ViewChart", vi.fn());
    const chartId = (await chart.getInfo()).appMetadata.instanceId;

    const found = await asker.findIntent("ViewChart", instrument);
    expect(found.intent.displayName).toBe("ViewChart (display)");
    expect(found.apps.map((a) => a.instanceId)).toEqual([undefined, chartId]);
    await expect(asker.findIntent("NoSuchIntent")).rejects.toThrow("NoAppsFound");

    const quoteOnly = await asker.findIntent("ViewQuote", undefined, "fdc3.instrument");
    expect(quoteOnly.apps).toEqual([{ appId: "quote" }]);
    const channelResult = await asker.findIntent("StartChat", undefined, "channel");
    expect(channelResult.apps).toEqual([{ appId: "chat" }]);

    const byContext = await asker.findIntentsByContext(instrument);
    expect(byContext.map((a) => a.intent.name).sort()).toEqual(["ViewChart", "ViewQuote"]);

  });

  it("raises for a context that only one running app handles", async () => {
    const [asker, chat] = await Promise.all([connect("asker"), connect("chat")]);
    const { handler, next } = received<Context>();
    await chat.addIntentListenerWithContext("StartChat", "fdc3.contact", handler);

    const resolution = await asker.raiseIntentForContext(contact);

    expect(resolution.intent).toBe("StartChat");
    expect(resolution.source.appId).toBe("chat");
    expect(await next).toEqual(contact);
  });

  it("settles getResult if the handler goes away", async () => {
    const raiser = await connect("raiser");
    const handlerConnection = await connectAgent({
      url: inject("busUrl"),
      appId: "handler",
      intentResolver: unexpectedResolver,
    });
    await handlerConnection.agent.addIntentListener("Never", () => new Promise(() => {}));
    const resolution = await raiser.raiseIntent("Never", instrument);

    await handlerConnection.messaging.disconnect();

    await expect(resolution.getResult()).resolves.toBeUndefined();
  });
});

describe("apps", () => {
  it("opens an app and delivers context to its listener", async () => {
    const opener = await connect("opener");
    const pending = opener.open({ appId: "plain" }, instrument);
    await settle();

    const opened = await connect("plain");
    const { handler, next } = received<Context>();
    await opened.addContextListener("fdc3.instrument", handler);

    expect(await next).toEqual(instrument);
    const identifier = await pending;
    expect(identifier.appId).toBe("plain");
    expect(identifier.instanceId).toBe((await opened.getInfo()).appMetadata.instanceId);
  });

  it("opens an app without context as soon as it connects", async () => {
    const opener = await connect("opener");
    const pending = opener.open({ appId: "plain" });
    await settle();
    await connect("plain");
    expect((await pending).appId).toBe("plain");
  });

  it("rejects opening an unknown app", async () => {
    const opener = await connect("opener");
    await expect(opener.open({ appId: "nope" })).rejects.toThrow("AppNotFound");
  });

  it("reports 3.0 implementation metadata", async () => {
    const app = await connect("chart");
    const info = await app.getInfo();
    expect(info.fdc3Version).toBe("3.0");
    expect(info.provider).toBe("one-terminal-desktop-agent");
    expect(info.appMetadata.appId).toBe("chart");
    expect(info.appMetadata.title).toBe("chart title");
  });

  it("finds instances and app metadata", async () => {
    const [asker, one, two] = await Promise.all([connect("asker"), connect("twin"), connect("twin")]);
    const ids = await Promise.all([one, two].map(async (a) => (await a.getInfo()).appMetadata.instanceId));
    const found = await asker.findInstances({ appId: "twin" });
    expect(found.map((a) => a.instanceId).sort()).toEqual([...ids].sort());
    expect((await asker.getAppMetadata({ appId: "chart" })).title).toBe("chart title");
    await expect(asker.getAppMetadata({ appId: "nope" })).rejects.toThrow("TargetAppUnavailable");
  });

  it("rejects close for an app the agent didn't open", async () => {
    const app = await connect("a");
    await expect(app.close()).rejects.toThrow("ErrorOnClose");
  });
});
