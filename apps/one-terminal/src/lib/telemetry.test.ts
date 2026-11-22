import { mockIPC } from "@tauri-apps/api/mocks";
import { describe, expect, it } from "vitest";
import { uiTransport } from "./telemetry";

describe("uiTransport", () => {
  it("sends the batch to wm_telemetry_log", async () => {
    const calls: Array<[string, unknown]> = [];
    mockIPC((cmd, args) => {
      calls.push([cmd, args]);
    });
    const batch = { records: [{ level: "error" as const, message: "boom" }] };
    await uiTransport(batch);
    expect(calls).toEqual([["wm_telemetry_log", { batch }]]);
  });
});
