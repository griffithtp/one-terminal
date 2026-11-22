/**
 * The widget transport: posts batches to the widget telemetry endpoint of the
 * process hosting the page (`ot_telemetry::ingress`), authenticated by the
 * window's token. Both arrive as `window.OT_TELEMETRY = { url, token }`.
 *
 * The body is `text/plain` JSON `{token, records}`, so `fetch` and
 * `navigator.sendBeacon` send it without a CORS preflight.
 */
import type { TelemetryBatch, TelemetryRecord, Transport } from "./index";

/** What the hosting process injects. */
export interface WidgetEndpoint {
  url: string;
  token: string;
}

/** The endpoint's body cap (`ingress::MAX_BODY_BYTES`). */
export const MAX_BODY_BYTES = 16 * 1024;
/** Kept below the cap for the envelope and the token. */
const RECORD_BUDGET_BYTES = MAX_BODY_BYTES - 3 * 1024;
/** A record that alone exceeds the budget keeps this much of its message. */
const OVERSIZED_MESSAGE_CHARS = 1_024;

const encoder = new TextEncoder();
const byteLength = (s: string) => encoder.encode(s).length;

/** `window.OT_TELEMETRY`, when the page runs in a OneTerminal widget window. */
export function injectedEndpoint(scope: unknown = globalThis): WidgetEndpoint | undefined {
  const value = (scope as { OT_TELEMETRY?: Partial<WidgetEndpoint> }).OT_TELEMETRY;
  return typeof value?.url === "string" && typeof value.token === "string"
    ? { url: value.url, token: value.token }
    : undefined;
}

/**
 * Split `records` into request bodies under the endpoint's cap. A record too
 * large on its own is shortened (no stack, message clipped) rather than lost.
 */
export function bodies(endpoint: WidgetEndpoint, records: TelemetryRecord[]): string[] {
  const out: string[] = [];
  let chunk: string[] = [];
  let size = 0;
  const push = () => {
    if (chunk.length > 0)
      out.push(`{"token":${JSON.stringify(endpoint.token)},"records":[${chunk.join(",")}]}`);
    chunk = [];
    size = 0;
  };
  for (const record of records) {
    let json = JSON.stringify(record);
    if (byteLength(json) > RECORD_BUDGET_BYTES) json = JSON.stringify(shorten(record));
    const bytes = byteLength(json) + 1;
    if (size + bytes > RECORD_BUDGET_BYTES) push();
    chunk.push(json);
    size += bytes;
  }
  push();
  return out;
}

function shorten(record: TelemetryRecord): TelemetryRecord {
  const message = record.message.slice(0, OVERSIZED_MESSAGE_CHARS);
  const { attributes: _dropped, ...rest } = record;
  return {
    ...rest,
    message,
    ...(record.error && {
      error: {
        name: record.error.name.slice(0, 128),
        message: record.error.message.slice(0, OVERSIZED_MESSAGE_CHARS),
      },
    }),
  };
}

/** Post each batch with `fetch` (`keepalive`, so a send survives navigation). */
export function httpTransport(
  endpoint: WidgetEndpoint,
  fetchFn: typeof fetch = (...args) => fetch(...args)
): Transport {
  return async (batch: TelemetryBatch) => {
    for (const body of bodies(endpoint, batch.records)) {
      const response = await fetchFn(endpoint.url, {
        method: "POST",
        body,
        keepalive: true,
        credentials: "omit",
      });
      if (!response.ok) throw new Error(`telemetry endpoint answered ${response.status}`);
    }
  };
}

/**
 * Hand each batch to `navigator.sendBeacon`, which the browser delivers even
 * as the page unloads. Returns false when the browser refused one.
 */
export function beaconTransport(
  endpoint: WidgetEndpoint,
  send: (url: string, body: string) => boolean = (url, body) => navigator.sendBeacon(url, body)
): (batch: TelemetryBatch) => boolean {
  return (batch) => bodies(endpoint, batch.records).every((body) => send(endpoint.url, body));
}
