import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createTelemetry, MAX_MESSAGE_CHARS, type TelemetryBatch } from "./index";

function fakeConsole() {
  return { error: vi.fn(), warn: vi.fn() };
}

function setup(overrides: Partial<Parameters<typeof createTelemetry>[0]> = {}) {
  const batches: TelemetryBatch[] = [];
  const transport = vi.fn(async (batch: TelemetryBatch) => {
    batches.push(batch);
  });
  const target = new EventTarget() as unknown as Window;
  const output = fakeConsole();
  const telemetry = createTelemetry({ transport, target, console: output, ...overrides });
  return { telemetry, transport, batches, target, output };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("batching", () => {
  it("sends queued records after the flush interval", async () => {
    const { telemetry, batches } = setup();
    telemetry.log("info", "one");
    telemetry.log("warn", "two", { traceId: "t-1", attributes: { "ot.kind": "x" } });
    expect(batches).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(1_000);
    expect(batches).toHaveLength(1);
    expect(batches[0].records.map((r) => r.message)).toEqual(["one", "two"]);
    expect(batches[0].records[1]).toMatchObject({
      level: "warn",
      traceId: "t-1",
      attributes: { "ot.kind": "x" },
    });
    expect(batches[0].records[0].timestamp).toMatch(/Z$/);
  });

  it("sends a full batch at once and splits larger queues", async () => {
    const { telemetry, batches } = setup({ maxBatch: 3 });
    for (let i = 0; i < 7; i++) telemetry.log("info", `r${i}`);
    await vi.advanceTimersByTimeAsync(0);
    expect(batches.map((b) => b.records.length)).toEqual([3, 3, 1]);
  });

  it("drops the oldest records beyond the queue cap", async () => {
    const { telemetry, batches } = setup({ maxQueue: 5, maxBatch: 100 });
    for (let i = 0; i < 8; i++) telemetry.log("info", `r${i}`);
    await telemetry.flush();
    expect(batches[0].records.map((r) => r.message)).toEqual(["r3", "r4", "r5", "r6", "r7"]);
  });

  it("truncates long messages", async () => {
    const { telemetry, batches } = setup();
    telemetry.log("info", "x".repeat(MAX_MESSAGE_CHARS + 100));
    await telemetry.flush();
    expect(batches[0].records[0].message.length).toBe(MAX_MESSAGE_CHARS + 1);
  });

  it("queues records logged while a send is in flight", async () => {
    let release!: () => void;
    const batches: TelemetryBatch[] = [];
    const transport = vi.fn(
      (batch: TelemetryBatch) =>
        new Promise<void>((resolve) => {
          batches.push(batch);
          release = resolve;
        })
    );
    const { telemetry } = setup({ transport });
    telemetry.log("info", "first");
    const sending = telemetry.flush();
    telemetry.error(new Error("during the send"));
    release(); // first batch done; the same flush then sends the queued error
    await vi.advanceTimersByTimeAsync(0);
    release();
    await sending;
    expect(batches.map((b) => b.records.map((r) => r.message))).toEqual([
      ["first"],
      ["during the send"],
    ]);
  });

  it("keeps one send in flight", async () => {
    let release!: () => void;
    const transport = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          release = resolve;
        })
    );
    const { telemetry } = setup({ transport, maxBatch: 1 });
    telemetry.log("info", "a");
    telemetry.log("info", "b");
    const first = telemetry.flush();
    const second = telemetry.flush();
    expect(transport).toHaveBeenCalledTimes(1);
    release();
    await vi.advanceTimersByTimeAsync(0);
    release();
    await Promise.all([first, second]);
    expect(transport).toHaveBeenCalledTimes(2);
  });
});

describe("errors", () => {
  it("describes Error objects and other thrown values", async () => {
    const { telemetry, batches } = setup();
    telemetry.error(new TypeError("bad"));
    telemetry.error("plain string");
    await telemetry.flush();
    const [typed, plain] = batches[0].records;
    expect(typed).toMatchObject({ level: "error", message: "bad", error: { name: "TypeError" } });
    expect(typed.error?.stack).toContain("TypeError");
    expect(plain.error).toEqual({ name: "NonError", message: "plain string" });
  });

  it("reports window errors and unhandled rejections", async () => {
    const { target, batches } = setup();
    target.dispatchEvent(Object.assign(new Event("error"), { error: new Error("boom") }));
    target.dispatchEvent(
      Object.assign(new Event("unhandledrejection"), { reason: new Error("nope") })
    );
    await vi.advanceTimersByTimeAsync(1_000);
    expect(batches[0].records.map((r) => r.message)).toEqual(["boom", "nope"]);
    expect(batches[0].records[1].attributes).toEqual({ "error.unhandled_rejection": true });
  });

  it("reports the same Error object once", async () => {
    const { telemetry, batches } = setup();
    const err = new Error("twice");
    telemetry.error(err);
    telemetry.error(err);
    await telemetry.flush();
    expect(batches[0].records).toHaveLength(1);
  });

  it("stops listening after dispose", async () => {
    const { telemetry, target, batches } = setup();
    await telemetry.dispose();
    target.dispatchEvent(Object.assign(new Event("error"), { error: new Error("late") }));
    await vi.advanceTimersByTimeAsync(1_000);
    expect(batches).toHaveLength(0);
  });
});

describe("console capture", () => {
  it("is off by default", async () => {
    const { output, batches } = setup();
    output.error("not captured");
    await vi.advanceTimersByTimeAsync(1_000);
    expect(batches).toHaveLength(0);
  });

  it("forwards console.error / warn and still prints them", async () => {
    const output = fakeConsole();
    const original = output.error;
    const { batches } = setup({ captureConsole: true, console: output });
    output.warn("careful", { n: 1 });
    output.error("failed:", new Error("x"));
    await vi.advanceTimersByTimeAsync(1_000);
    expect(original).toHaveBeenCalledWith("failed:", expect.any(Error));
    expect(batches[0].records[0]).toMatchObject({ level: "warn", message: 'careful {"n":1}' });
    expect(batches[0].records[1]).toMatchObject({
      level: "error",
      message: "failed: Error: x",
      error: { message: "x" },
    });
  });

  it("never captures what the transport itself logs", async () => {
    const output = fakeConsole();
    const transport = vi.fn(async () => {
      output.error("transport noise");
    });
    const { telemetry } = setup({ captureConsole: true, console: output, transport });
    telemetry.log("info", "once");
    await telemetry.flush();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(transport).toHaveBeenCalledTimes(1);
  });

  it("reports a failing transport at most once a minute, without looping", async () => {
    const output = fakeConsole();
    const warn = output.warn; // the spy; capture replaces `output.warn` with a wrapper
    const transport = vi.fn(async () => {
      throw new Error("ipc down");
    });
    const { telemetry } = setup({ captureConsole: true, console: output, transport });
    telemetry.log("info", "a");
    await telemetry.flush();
    telemetry.log("info", "b");
    await telemetry.flush();
    expect(warn).toHaveBeenCalledTimes(1);
    expect(transport).toHaveBeenCalledTimes(2);
  });
});
