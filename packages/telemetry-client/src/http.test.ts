import { afterEach, describe, expect, it, vi } from "vitest";
import { beaconTransport, bodies, httpTransport, injectedEndpoint, MAX_BODY_BYTES } from "./http";
import { createTelemetry, type TelemetryRecord } from "./index";
import { startWidgetTelemetry } from "./widget";

const endpoint = { url: "http://127.0.0.1:5000/v1/records", token: "ab.cd" };
const record = (message: string): TelemetryRecord => ({ level: "error", message });

describe("bodies", () => {
  it("wraps records with the token", () => {
    const [body] = bodies(endpoint, [record("a"), record("b")]);
    expect(JSON.parse(body)).toEqual({ token: "ab.cd", records: [record("a"), record("b")] });
  });

  it("splits batches to stay under the endpoint's body cap", () => {
    const records = Array.from({ length: 20 }, (_, i) => record(`${i}`.padEnd(2_000, "x")));
    const out = bodies(endpoint, records);
    expect(out.length).toBeGreaterThan(1);
    for (const body of out)
      expect(new TextEncoder().encode(body).length).toBeLessThan(MAX_BODY_BYTES);
    expect(out.flatMap((b) => JSON.parse(b).records)).toHaveLength(20);
  });

  it("shortens a record too large on its own instead of dropping it", () => {
    const huge: TelemetryRecord = {
      level: "error",
      message: "m".repeat(20_000),
      error: { name: "Error", message: "e".repeat(20_000), stack: "s".repeat(20_000) },
    };
    const [body] = bodies(endpoint, [huge]);
    const [sent] = JSON.parse(body).records;
    expect(new TextEncoder().encode(body).length).toBeLessThan(MAX_BODY_BYTES);
    expect(sent.message.length).toBe(1_024);
    expect(sent.error.stack).toBeUndefined();
  });
});

describe("httpTransport", () => {
  it("posts text bodies without credentials and rejects on an error status", async () => {
    const fetchFn = vi.fn(async () => new Response(null, { status: 204 }));
    await httpTransport(endpoint, fetchFn)({ records: [record("a")] });
    expect(fetchFn).toHaveBeenCalledWith(
      endpoint.url,
      expect.objectContaining({ method: "POST", keepalive: true, credentials: "omit" })
    );
    const init = (fetchFn.mock.calls[0] as unknown as [string, RequestInit])[1];
    expect(typeof init.body).toBe("string");
    expect(init.headers).toBeUndefined();

    const refused = httpTransport(endpoint, async () => new Response(null, { status: 401 }));
    await expect(refused({ records: [record("a")] })).rejects.toThrow("401");
  });
});

describe("unload", () => {
  it("hands queued records to the unload transport on pagehide", () => {
    vi.useFakeTimers();
    const sent: string[] = [];
    const target = new EventTarget() as unknown as Window;
    const transport = vi.fn(async () => {});
    const telemetry = createTelemetry({
      transport,
      unloadTransport: beaconTransport(endpoint, (_url, body) => {
        sent.push(body);
        return true;
      }),
      target,
      console: { error: vi.fn(), warn: vi.fn() },
    });
    telemetry.log("warn", "leaving");
    target.dispatchEvent(new Event("pagehide"));
    expect(sent.map((b) => JSON.parse(b).records[0].message)).toEqual(["leaving"]);
    expect(transport).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});

describe("startWidgetTelemetry", () => {
  afterEach(() => {
    delete (globalThis as { OT_TELEMETRY?: unknown }).OT_TELEMETRY;
  });

  it("does nothing outside a OneTerminal widget window", () => {
    expect(injectedEndpoint()).toBeUndefined();
    expect(startWidgetTelemetry()).toBeUndefined();
  });

  it("starts once with the injected endpoint", () => {
    (globalThis as { OT_TELEMETRY?: unknown }).OT_TELEMETRY = endpoint;
    expect(injectedEndpoint()).toEqual(endpoint);
    const first = startWidgetTelemetry();
    expect(first).toBeDefined();
    expect(startWidgetTelemetry()).toBe(first);
    void first?.dispose();
  });
});
