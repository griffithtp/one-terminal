/**
 * Relays errors and logs from a OneTerminal page to the Rust process that owns
 * it, which writes them to its log file and (when export is configured) to
 * OpenTelemetry. Framework-agnostic: no React, no Tauri, no FDC3 — the caller
 * supplies the transport (Tauri IPC for the framework UIs).
 *
 * The Rust side (`ot_telemetry::relay`) enforces every limit again; the
 * client-side caps here only keep payloads small.
 */

/** Matches `ot_telemetry::relay::Level`. */
export type TelemetryLevel = "error" | "warn" | "info" | "debug";

export type AttributeValue = string | number | boolean;

/** One record, as `ot_telemetry::relay::Record` expects it. */
export interface TelemetryRecord {
  level: TelemetryLevel;
  message: string;
  /** Client clock, ISO 8601. */
  timestamp?: string;
  /** An FDC3 `metadata.traceId` this record relates to (recorded, not joined). */
  traceId?: string;
  /** Lowercase `a-z0-9_.` keys; at most 8 are kept. */
  attributes?: Record<string, AttributeValue>;
  error?: { name: string; message: string; stack?: string };
}

/** What a transport sends: `ot_telemetry::relay::RecordBatch`. */
export interface TelemetryBatch {
  records: TelemetryRecord[];
}

/** Delivers one batch. A rejection is reported (rate-limited) and the batch dropped. */
export type Transport = (batch: TelemetryBatch) => Promise<unknown>;

export interface TelemetryOptions {
  transport: Transport;
  /** Longest a record waits before being sent. Default 1000 ms. */
  flushIntervalMs?: number;
  /** Records per batch; a full batch is sent at once. Default 20 (the relay's cap). */
  maxBatch?: number;
  /** Records held while sending; beyond this the oldest are dropped. Default 200. */
  maxQueue?: number;
  /** Report `window` `error` and `unhandledrejection` events. Default true. */
  captureErrors?: boolean;
  /** Also report `console.error` / `console.warn` calls. Default false. */
  captureConsole?: boolean;
  /** Event target and console to hook (tests). Default `window` / `console`. */
  target?: Pick<Window, "addEventListener" | "removeEventListener">;
  console?: Pick<Console, "error" | "warn">;
}

export interface LogOptions {
  traceId?: string;
  attributes?: Record<string, AttributeValue>;
}

export interface Telemetry {
  log(level: TelemetryLevel, message: string, options?: LogOptions): void;
  /** Report an error (an `Error`, or anything thrown). */
  error(error: unknown, options?: LogOptions): void;
  /** Send everything queued now. */
  flush(): Promise<void>;
  /** Remove hooks and timers, then flush. */
  dispose(): Promise<void>;
}

/** Client-side caps (the relay enforces its own; these keep IPC small). */
export const MAX_MESSAGE_CHARS = 2_048;
export const MAX_STACK_CHARS = 8_192;
/** At most one "transport failed" console line per this interval. */
const TRANSPORT_WARN_INTERVAL_MS = 60_000;

export function createTelemetry(options: TelemetryOptions): Telemetry {
  const {
    transport,
    flushIntervalMs = 1_000,
    maxBatch = 20,
    maxQueue = 200,
    captureErrors = true,
    captureConsole = false,
  } = options;
  const target = options.target ?? (typeof window === "undefined" ? undefined : window);
  const output = options.console ?? console;
  // Originals, used for our own failure messages so they never re-enter.
  const originalError = output.error.bind(output);
  const originalWarn = output.warn.bind(output);

  let queue: TelemetryRecord[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;
  let inFlight: Promise<void> | undefined;
  /**
   * True only while the transport function itself runs synchronously: what it
   * logs on the way (an IPC layer's console noise) is not captured, so it can't
   * loop. Records from the rest of the page while a send is awaited are queued.
   */
  let insideTransport = false;
  /** Errors already reported (React logs boundary-caught errors through console too). */
  const reported = new WeakSet<object>();
  let lastTransportWarning = -Infinity;
  const cleanups: Array<() => void> = [];

  function enqueue(record: TelemetryRecord) {
    queue.push(record);
    if (queue.length > maxQueue) queue = queue.slice(queue.length - maxQueue);
    if (queue.length >= maxBatch) {
      schedule(0);
    } else if (timer === undefined) {
      schedule(flushIntervalMs);
    }
  }

  function schedule(delay: number) {
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = undefined;
      void flush();
    }, delay);
  }

  async function sendOnce(): Promise<void> {
    const records = queue.slice(0, maxBatch);
    queue = queue.slice(records.length);
    if (records.length === 0) return;
    try {
      let sent: Promise<unknown>;
      insideTransport = true;
      try {
        sent = transport({ records });
      } finally {
        insideTransport = false;
      }
      await sent;
    } catch (e) {
      const now = Date.now();
      if (now - lastTransportWarning >= TRANSPORT_WARN_INTERVAL_MS) {
        lastTransportWarning = now;
        originalWarn("[telemetry] could not send logs:", e);
      }
    }
  }

  async function flush(): Promise<void> {
    if (inFlight) return inFlight;
    inFlight = (async () => {
      while (queue.length > 0) await sendOnce();
    })().finally(() => {
      inFlight = undefined;
    });
    return inFlight;
  }

  function log(level: TelemetryLevel, message: string, opts: LogOptions = {}) {
    if (insideTransport) return;
    enqueue({
      level,
      message: clip(message, MAX_MESSAGE_CHARS),
      timestamp: new Date().toISOString(),
      ...(opts.traceId !== undefined && { traceId: opts.traceId }),
      ...(opts.attributes !== undefined && { attributes: opts.attributes }),
    });
  }

  function error(err: unknown, opts: LogOptions = {}, message?: string) {
    if (insideTransport) return;
    if (typeof err === "object" && err !== null) {
      if (reported.has(err)) return;
      reported.add(err);
    }
    const info = describeError(err);
    enqueue({
      level: "error",
      message: clip(message ?? (info.message || info.name), MAX_MESSAGE_CHARS),
      timestamp: new Date().toISOString(),
      error: info,
      ...(opts.traceId !== undefined && { traceId: opts.traceId }),
      ...(opts.attributes !== undefined && { attributes: opts.attributes }),
    });
  }

  if (captureErrors && target) {
    const onError = (event: Event) => {
      const e = event as ErrorEvent;
      error(e.error ?? e.message ?? "Script error");
    };
    const onRejection = (event: Event) => {
      error((event as PromiseRejectionEvent).reason, {
        attributes: { "error.unhandled_rejection": true },
      });
    };
    // Best effort: an async transport may not finish before the page unloads.
    const onPageHide = () => void flush();
    target.addEventListener("error", onError);
    target.addEventListener("unhandledrejection", onRejection);
    target.addEventListener("pagehide", onPageHide);
    cleanups.push(() => {
      target.removeEventListener("error", onError);
      target.removeEventListener("unhandledrejection", onRejection);
      target.removeEventListener("pagehide", onPageHide);
    });
  }

  if (captureConsole) {
    const wrap = (level: "error" | "warn", original: (...args: unknown[]) => void) => {
      return (...args: unknown[]) => {
        original(...args);
        const err = args.find((a) => a instanceof Error);
        const message = clip(args.map(formatArg).join(" "), MAX_MESSAGE_CHARS);
        if (err !== undefined && level === "error") {
          // Keep the caller's context ("loading X failed: …") as the message.
          error(err, { attributes: { "log.source": "console" } }, message);
        } else {
          log(level, message, { attributes: { "log.source": "console" } });
        }
      };
    };
    output.error = wrap("error", originalError);
    output.warn = wrap("warn", originalWarn);
    cleanups.push(() => {
      output.error = originalError;
      output.warn = originalWarn;
    });
  }

  return {
    log,
    error: (err: unknown, opts?: LogOptions) => error(err, opts),
    flush,
    async dispose() {
      for (const cleanup of cleanups.splice(0)) cleanup();
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
      await flush();
    },
  };
}

function describeError(err: unknown): { name: string; message: string; stack?: string } {
  if (err instanceof Error) {
    return {
      name: err.name || "Error",
      message: clip(err.message, MAX_MESSAGE_CHARS),
      ...(err.stack !== undefined && { stack: clip(err.stack, MAX_STACK_CHARS) }),
    };
  }
  return { name: "NonError", message: clip(formatArg(err), MAX_MESSAGE_CHARS) };
}

function formatArg(arg: unknown): string {
  if (typeof arg === "string") return arg;
  if (arg instanceof Error) return `${arg.name}: ${arg.message}`;
  try {
    return JSON.stringify(arg) ?? String(arg);
  } catch {
    return String(arg);
  }
}

function clip(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, max)}…` : s;
}
