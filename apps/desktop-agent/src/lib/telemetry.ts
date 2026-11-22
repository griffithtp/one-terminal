import { invoke } from "@tauri-apps/api/core";
import { createTelemetry, type Telemetry, type TelemetryBatch } from "@one-terminal/telemetry";

/** Sends a batch to the Desktop Agent process, which logs it (and exports it when configured). */
export function uiTransport(batch: TelemetryBatch): Promise<void> {
  return invoke("cda_telemetry_log", { batch });
}

/**
 * Start the dashboard's telemetry: uncaught errors, unhandled rejections and
 * `console.error` / `console.warn` go to the Desktop Agent's logs. Call once,
 * first, in the entry point.
 */
export function startTelemetry(): Telemetry {
  // One instance per webview: when the entry module re-runs (Vite HMR in
  // dev), drop the previous one so console wrappers and listeners don't stack.
  const slot = globalThis as { [INSTANCE]?: Telemetry };
  void slot[INSTANCE]?.dispose();
  const telemetry = createTelemetry({ transport: uiTransport, captureConsole: true });
  slot[INSTANCE] = telemetry;
  return telemetry;
}

const INSTANCE = Symbol.for("one-terminal.telemetry");
