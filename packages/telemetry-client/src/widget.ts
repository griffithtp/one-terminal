/**
 * Entry point for widget pages (bundled into `ot-telemetry.js`). In a
 * OneTerminal widget window — one with `window.OT_TELEMETRY` — errors and
 * logs go to the widget telemetry endpoint of the process hosting the page;
 * anywhere else `startWidgetTelemetry` returns `undefined` and does nothing.
 *
 * ```js
 * import { startWidgetTelemetry } from "./ot-telemetry.js";
 * const telemetry = startWidgetTelemetry();
 * telemetry?.log("info", "ready");
 * // In an FDC3 handler, relate a record to the context's trace:
 * telemetry?.error(err, { traceId: metadata?.traceId });
 * ```
 */
import { createTelemetry, type Telemetry, type TelemetryOptions } from "./index";
import { beaconTransport, httpTransport, injectedEndpoint } from "./http";

export type { Telemetry, TelemetryLevel, LogOptions } from "./index";

export type WidgetTelemetryOptions = Omit<TelemetryOptions, "transport" | "unloadTransport">;

const INSTANCE = Symbol.for("one-terminal.widget-telemetry");

/**
 * Start sending this page's errors (and, with `captureConsole`, console
 * warnings and errors) to OneTerminal. Calling it again returns the same
 * instance. `undefined` outside a OneTerminal widget window.
 */
export function startWidgetTelemetry(options: WidgetTelemetryOptions = {}): Telemetry | undefined {
  const scope = globalThis as { [INSTANCE]?: Telemetry };
  if (scope[INSTANCE]) return scope[INSTANCE];
  const endpoint = injectedEndpoint();
  if (!endpoint) return undefined;
  const telemetry = createTelemetry({
    ...options,
    transport: httpTransport(endpoint),
    ...(typeof navigator !== "undefined" &&
      typeof navigator.sendBeacon === "function" && { unloadTransport: beaconTransport(endpoint) }),
  });
  scope[INSTANCE] = telemetry;
  return telemetry;
}
