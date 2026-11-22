# Telemetry: logs, traces and metrics

This page explains what OneTerminal records, where it goes, and how to send it to an OpenTelemetry collector. It is for operators running OneTerminal and for developers of widgets and engine hosts.

- [What is recorded](#what-is-recorded)
- [Local log files](#local-log-files)
- [Sending telemetry to a collector](#sending-telemetry-to-a-collector)
- [Configuration](#configuration)
- [Processes and child processes](#processes-and-child-processes)
- [Traces](#traces)
- [Metrics](#metrics)
- [Logs from the UIs and from widgets](#logs-from-the-uis-and-from-widgets)
- [Privacy](#privacy)
- [Try it locally](#try-it-locally)
- [Troubleshooting](#troubleshooting)

---

## What is recorded

The Desktop Agent, the Terminal and `app-directory-server` record three signals. `tauri-webview-host` only writes local logs (see [Processes and child processes](#processes-and-child-processes)):

| Signal  | What                                                                                                | Where it goes                                      |
| ------- | --------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Logs    | The process's own log lines, plus errors and logs relayed from its UI and from the widgets it hosts | Local log file always; collector when export is on |
| Traces  | Spans for FDC3 requests, app launches and process startup, linked across processes                  | Collector only                                     |
| Metrics | Counters and histograms for FDC3 traffic, launches, windows and popouts                             | Collector only                                     |

**Nothing leaves the machine unless you configure a collector.** Local log files are always written.

---

## Local log files

Each process writes one JSON-lines file per day:

| OS      | Directory                                                                     |
| ------- | ----------------------------------------------------------------------------- |
| macOS   | `~/Library/Logs/one-terminal/`                                                |
| Windows | `%LOCALAPPDATA%\one-terminal\logs\`                                           |
| Linux   | `$XDG_STATE_HOME/one-terminal/logs/` (or `~/.local/state/one-terminal/logs/`) |

Files are named `<service>.<YYYY-MM-DD>.log` (`desktop-agent.2026-11-21.log`) and the last 7 days are kept (`telemetry.logRetentionDays`). Every process with a console also prints readable text to stderr. Windows release builds have no console, so there the file is the only record.

One line looks like this:

```json
{
  "timestamp": "2026-11-21T22:51:22.915Z",
  "level": "INFO",
  "target": "desktop_agent_lib",
  "pid": 19141,
  "message": "App Directory: 2 app(s) loaded at startup",
  "trace_id": "…",
  "span_id": "…"
}
```

`trace_id` and `span_id` appear when the line was written inside a span, so a log line can be looked up next to its trace. A process started by another one (with `TRACEPARENT`) writes `trace_id` and `parent_span_id` on lines outside any span of its own, linking them to the span that launched it.

The file is written by a background thread with a bounded buffer. A process that is force-killed (`taskkill /F`, `kill -9`) loses its last buffered lines; a normal quit flushes them.

### Watching the logs

Follow today's file while you work (macOS shown; use your OS's directory from the table above):

```sh
tail -f ~/Library/Logs/one-terminal/desktop-agent.$(date +%F).log
```

On Windows (PowerShell):

```powershell
Get-Content "$env:LOCALAPPDATA\one-terminal\logs\desktop-agent.$(Get-Date -Format yyyy-MM-dd).log" -Wait
```

With [`jq`](https://jqlang.org/), show only errors and warnings, or only what the UIs and widgets reported:

```sh
tail -f ~/Library/Logs/one-terminal/one-terminal.$(date +%F).log | jq -c 'select(.level == "ERROR" or .level == "WARN")'
tail -f ~/Library/Logs/one-terminal/one-terminal.$(date +%F).log | jq -c 'select(.target | startswith("ot_relay"))'
```

Widget records are in the log of the process hosting the widget: `one-terminal` for Terminal panels, `desktop-agent` for windows the Desktop Agent opens.

---

## Sending telemetry to a collector

Set an OTLP/HTTP endpoint and the Desktop Agent, the Terminal and `app-directory-server` export all three signals:

```sh
OT_OTLP_ENDPOINT=http://localhost:4318 npm run dev:desktop-agent
```

or put it in the [config file](#configuration) as `telemetry.otlp.endpoint`. The Terminals the Desktop Agent starts inherit the setting.

The endpoint is the collector's base URL: `/v1/traces`, `/v1/metrics` and `/v1/logs` are appended. The first of these wins, per signal:

1. `OT_OTLP_ENDPOINT`
2. `OTEL_EXPORTER_OTLP_<SIGNAL>_ENDPOINT` (a full URL, used as-is)
3. `OTEL_EXPORTER_OTLP_ENDPOINT`
4. `telemetry.otlp.endpoint`

`OTEL_SDK_DISABLED=true` turns export off whatever else is set. The other standard `OTEL_*` variables that are honoured are listed in [AGENTS.md → Environment variables](../AGENTS.md#environment-variables). Also read are `OTEL_EXPORTER_OTLP_TIMEOUT`, `OTEL_METRIC_EXPORT_INTERVAL` (ms, minimum 1000) and `OTEL_EXPORTER_OTLP_PROTOCOL`. `OTEL_TRACES_SAMPLER` accepts `always_on`, `always_off`, `traceidratio` and their `parentbased_` forms.

Only `http/protobuf` is supported (no gRPC). An `http://` endpoint on another host logs a warning: use `https://` for anything that isn't on this machine.

Exporting never slows the apps down: data is sent from background threads, and if the collector is unreachable the queues drop data instead of blocking. Quitting waits at most 2 seconds for the last export.

Each process logs its effective settings at startup, with secrets removed:

```
config telemetry.otlp.traces = http://localhost:4318/v1/traces (env OT_OTLP_ENDPOINT)
```

---

## Configuration

The `telemetry` block goes in `agent.config.json` (Desktop Agent) or `terminal.config.json` (Terminal). Every key is optional; these are the defaults:

```jsonc
"telemetry": {
  "logLevel": null,              // null = "info,tao=warn,wry=warn"; same syntax as OT_LOG
  "logDir": null,                // null = the per-OS directory above
  "logRetentionDays": 7,
  "serviceNamespace": "one-terminal",
  "resourceAttributes": {},      // e.g. { "deployment.environment": "uat" }
  "otlp": {
    "endpoint": null,            // e.g. "https://collector.example.com"; null = no export
    "protocol": "http/protobuf",
    "headers": {},               // sent with every export (auth); never logged
    "timeoutMs": 10000,
    "sampleRatio": 1.0,          // fraction of new traces kept
    "metricsIntervalMs": 60000,
    "signals": { "traces": true, "metrics": true, "logs": true }
  },
  "widgets": { "enabled": true } // accept logs from widget pages (see below)
}
```

`app-directory-server` and `tauri-webview-host` have no config file; they read the environment only.

**Log level.** `OT_LOG` (then `RUST_LOG`, then `logLevel`) takes a default level and `target=level` pairs, e.g. `OT_LOG=info,desktop_agent_lib::fdc3_bus=debug`. Without a default level, `warn` is assumed. Spans are recorded whatever the level, so `OT_LOG=warn` quietens logs without turning tracing off.

**Sampling** is parent-based: a process launched from a sampled span is always traced, and `sampleRatio` applies only to traces that start in that process.

---

## Processes and child processes

Each process reports under its own `service.name` — `desktop-agent`, `one-terminal`, `tauri-webview-host`, `app-directory-server` — with `service.namespace`, `service.version`, `service.instance.id` and `os.type`.

When a process starts another, it passes down what that child needs and nothing more:

| Child                                     | Receives                                                                                                                                  |
| ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Terminal (started by the Desktop Agent)   | Trace context, the agent's log level and directory, and its export settings including headers                                             |
| `tauri-webview-host`, `electron-host`     | Trace context, log level and directory, and the [widget endpoint](#widgets) URL and its window's token. No export settings or credentials |
| Engine plugin binaries (third-party code) | Trace context only. Export credentials are removed from its environment                                                                   |

So a Terminal started by the Desktop Agent uses the **agent's** `logLevel` and `logDir`, not its own config file's. Start the Terminal on its own (`npm run dev:terminal`) to use its own settings.

`tauri-webview-host` writes local logs but never exports. Its lines carry `trace_id` and `parent_span_id` of the launch that started it; to get them into a collector, you can for example point the collector's [`filelog` receiver](https://github.com/open-telemetry/opentelemetry-collector-contrib/tree/main/receiver/filelogreceiver) at `tauri-webview-host.*.log` in the log directory (JSON parser, `timestamp` field). The Electron host and plugin binaries don't write OneTerminal log files.

---

## Traces

| Span              | Process                 | Covers                                                                                                                                                                                      |
| ----------------- | ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fdc3.connect`    | Desktop Agent           | A widget's FDC3 handshake, with its app id and outcome                                                                                                                                      |
| _message type_    | Desktop Agent           | One FDC3 request, named after its type (`broadcastRequest`, `raiseIntentRequest`, …; `other` for unknown types), with `fdc3.message.type` and `fdc3.request_uuid`. Heartbeats aren't traced |
| `fdc3.launch`     | Desktop Agent           | Opening an app for `fdc3.open` or an intent                                                                                                                                                 |
| `app.launch`      | Desktop Agent, Terminal | Starting an app: in-process, in `tauri-webview-host`, in Electron or as a plugin binary. The Terminal records it only for out-of-process hosts                                              |
| `process.startup` | Desktop Agent, Terminal | Startup up to the end of setup. A Terminal the Desktop Agent restores at its own startup is a child of the agent's `process.startup`                                                        |

An app opened through FDC3 therefore shows up as **one trace**: the request span (`openRequest`, `raiseIntentRequest`) → `fdc3.launch` → `app.launch`. `app.launch` follows only for apps that open in a OneTerminal window or engine host (`onlineNative`); the Desktop Agent opens `web` apps in the default browser, and their trace ends at `fdc3.launch`. A `tauri-webview-host` window doesn't appear as a span; its log lines carry the same `trace_id`. Terminals restored when the Desktop Agent starts appear under the agent's `process.startup`; a Terminal opened later from its window or tray starts its own trace.

**FDC3 `traceId`.** Contexts delivered to widgets carry `metadata.traceId`. When the sending app didn't supply one, it is the trace id of the request span, so you can search for it in your tracing backend. An app-supplied `traceId` is passed on unchanged and recorded as the `fdc3.trace_id` attribute — never used as a span parent, because any local page can connect to the bus and must not be able to join or steer other apps' traces.

---

## Metrics

Attributes are deliberately few: app ids and URLs are never metric attributes.

| Metric                          | Type            | Attributes                                                                                    |
| ------------------------------- | --------------- | --------------------------------------------------------------------------------------------- |
| `ot.fdc3.messages`              | counter         | `fdc3.message.type`, `outcome` (`ok` / `error` / `unhandled`)                                 |
| `ot.fdc3.message.duration`      | histogram (s)   | `fdc3.message.type`                                                                           |
| `ot.fdc3.connections`           | up-down counter | —                                                                                             |
| `ot.fdc3.handshake.failures`    | counter         | `reason` (`closed` / `invalid`)                                                               |
| `ot.app.launches`               | counter         | `ot.launch.kind` (`in_process` / `tauri_host` / `electron_host` / `plugin_binary`), `outcome` |
| `ot.app.launch.duration`        | histogram (s)   | `ot.launch.kind`                                                                              |
| `ot.terminal.windows`           | up-down counter | —                                                                                             |
| `ot.terminal.popouts`           | up-down counter | —                                                                                             |
| `ot.popout.duration`            | histogram (s)   | `op` (`open` / `dock`)                                                                        |
| `ot.telemetry.relay.dropped`    | counter         | `reason` (`batch_too_large` / `rate_limited`), `origin` (`ui` / `widget`)                     |
| `ot.telemetry.ingress.rejected` | counter         | `reason` (`too_large` / `malformed` / `unauthorized`)                                         |

Prometheus-style backends show them with `_` for `.` and a unit or `_total` suffix (`ot_fdc3_messages_total`).

---

## Logs from the UIs and from widgets

Browser code can't write to the log file itself; it sends records to the process that hosts it, which checks and limits them and writes them as ordinary log lines (target `ot_relay::ui` or `ot_relay::widget`) with:

| Field                                                         | Value                                         |
| ------------------------------------------------------------- | --------------------------------------------- |
| `ot.origin`                                                   | `ui` or `widget`                              |
| `ot.window`, `ot.app_id`, `ot.instance_id`                    | Set by the hosting process, never by the page |
| `client.timestamp`                                            | The page's own clock                          |
| `attributes`                                                  | The record's attributes, as one JSON string   |
| `fdc3.trace_id`                                               | The FDC3 `traceId` the page passed, if any    |
| `exception.type`, `exception.message`, `exception.stacktrace` | For errors                                    |

Limits per sender: 20 records per batch and 10 records a second (bursts of 50). Records beyond either limit are dropped and counted (`ot.telemetry.relay.dropped`). Messages are cut at 2,048 characters, stacks at 8,192 and attribute values at 256, with `…` added; control characters are removed, and credentials and query strings in URLs are stripped. Widget requests over 16 KiB are refused (HTTP 413).

### The Terminal and Desktop Agent UIs

The framework UIs report uncaught errors, unhandled promise rejections and `console.error` / `console.warn` automatically, through `@one-terminal/telemetry` over Tauri IPC. See [docs/conventions/react.md](conventions/react.md#errors-and-telemetry).

### Widgets

A widget opened by OneTerminal — in a Terminal panel or popout, a Desktop Agent window, `tauri-webview-host` or the Electron host — finds `window.OT_TELEMETRY = { url, token }`: the address of its host process's widget endpoint and a token for its window. Load `ot-telemetry.js`, served next to `fdc3-plugin.js`:

```js
import { getAgent } from "./fdc3-plugin.js";
import { startWidgetTelemetry } from "./ot-telemetry.js";

// Reports uncaught errors and unhandled rejections; with captureConsole,
// console.error and console.warn too. `undefined` outside OneTerminal.
const telemetry = startWidgetTelemetry({ captureConsole: true });

const fdc3 = await getAgent();
await fdc3.addContextListener("fdc3.instrument", (context, metadata) => {
  try {
    show(context);
  } catch (e) {
    // Relate the record to the broadcast that delivered the context.
    telemetry?.error(e, { traceId: metadata?.traceId });
  }
});

telemetry?.log("info", "chart ready", { attributes: { "chart.series": 3 } });
```

| Call                                             | Does                                                                               |
| ------------------------------------------------ | ---------------------------------------------------------------------------------- |
| `startWidgetTelemetry(options?)`                 | Starts once per page and returns the instance; `undefined` when not in OneTerminal |
| `log(level, message, { traceId?, attributes? })` | `level` is `error`, `warn`, `info` or `debug`; up to 8 attributes, keys `a-z0-9_.` |
| `error(err, { traceId?, attributes? })`          | Reports an `Error` (with its stack) or any thrown value                            |
| `flush()`                                        | Sends what is queued now (records otherwise go within a second)                    |
| `dispose()`                                      | Removes the hooks and sends what is queued                                         |

Records are batched, sent with `fetch`, and handed to `navigator.sendBeacon` when the page is closed. The page needs nothing else: no IPC access, no CSP change beyond allowing `connect-src http://127.0.0.1:*` if your page sets a strict `connect-src` (the port is random per process, so a fixed port won't do).

The endpoint is set by `telemetry.widgets.enabled` (on by default). Each Terminal and Desktop Agent process runs its own, on a random `127.0.0.1` port, and accepts a request only with a token it issued for one of its windows. A widget's records therefore appear in the log of the process that hosts it, named by its window and app — whatever the page claims.

**Writing an engine host?** A new host must give its page `window.OT_TELEMETRY` the way `tauri-webview-host` (initialization script) and `electron-host` (`preload.js`) do: read `OT_TELEMETRY_URL` and `OT_TELEMETRY_TOKEN` from its environment and expose exactly those two values to the main frame.

---

## Privacy

- **Opt-in.** Without an endpoint nothing is sent anywhere; logs stay in the per-user log directory.
- **Redaction.** URLs in OneTerminal's own log lines and spans have credentials and query strings removed; OTLP headers and widget tokens are never logged; FDC3 context payloads aren't logged.
- **Relayed records are what the page wrote.** A widget's `log` or `error` message can contain anything the widget puts in it. Treat widget records like application logs, and don't log personal data from widgets.
- **`OT_LOG=trace`** turns on trace-level logs from dependencies, which may include message bodies. Use it for local debugging only, never with export on.

---

## Try it locally

This walkthrough sends everything to Grafana's all-in-one OpenTelemetry image (a collector plus Tempo for traces, Loki for logs and Prometheus for metrics) on your machine. It needs Docker.

**1. Start the collector** in its own terminal (it stays in the foreground; `Ctrl+C` stops it and discards the data):

```sh
docker run --rm -p 3000:3000 -p 4318:4318 grafana/otel-lgtm
```

**2. Start the Desktop Agent and a Terminal with export on**, each in its own terminal:

```sh
OT_OTLP_ENDPOINT=http://localhost:4318 OTEL_METRIC_EXPORT_INTERVAL=5000 npm run dev:desktop-agent
OT_OTLP_ENDPOINT=http://localhost:4318 OTEL_METRIC_EXPORT_INTERVAL=5000 npm run dev:terminal
```

`OTEL_METRIC_EXPORT_INTERVAL=5000` sends metrics every 5 seconds instead of every 60, which is handy while experimenting. The Desktop Agent's built-in App Directory serves the sample apps.

> In development, a Terminal you open **from the Desktop Agent** runs the `one-terminal` binary already in `target/debug/` and loads its UI from the Terminal's Vite server. `npm run dev:desktop-agent` builds neither, so run `npm run dev:terminal` as above (it does both), or first run `cargo build -p one-terminal` and `npm -w @one-terminal/one-terminal run dev`. Otherwise you get an old Terminal without telemetry, or a blank window.

**3. Check that export is on.** Among each process's first lines (in its terminal or [log file](#watching-the-logs)):

```
config telemetry.otlp.traces = http://localhost:4318/v1/traces (env OT_OTLP_ENDPOINT)
widget telemetry endpoint on http://127.0.0.1:<port>/v1/records
```

`telemetry.otlp = off` means the endpoint wasn't picked up.

**4. Make something happen in the Terminal:**

- Add **Ticker Plant** and **Chart Viewer** as panels from the Terminal's launcher, and put both on the same channel. (Opening them from the Desktop Agent window opens them in your default browser instead — they are `web` apps — where they get no widget telemetry.)
- In the ticker, broadcast an instrument and click **ViewChart**. The chart logs each instrument it receives, with its FDC3 `traceId`.
- In the chart panel, right-click → **Inspect** → Console. `window.OT_TELEMETRY` should show `{ url, token }`. For a widget error, run `setTimeout(() => { throw new Error("telemetry test"); })`; an error thrown directly at the console prompt isn't reported, because browsers don't treat it as a page error.

The widget records appear in the Terminal's log straight away (`ot_relay::widget` lines in `one-terminal.<date>.log`).

**5. Look at it in Grafana** at <http://localhost:3000>, under **Explore**:

| Data source | What to look for                                                                                                                                                                                                                                                     |
| ----------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Loki        | `{service_name="one-terminal"} \| ot_origin="widget"` for widget records, with `ot_app_id`, `fdc3_trace_id` and, for errors, `exception_type` / `exception_message`. Loki stores the record's attributes with `_` for `.`, after the `\|` (they aren't index labels) |
| Tempo       | Service `desktop-agent`: `broadcastRequest`, `raiseIntentRequest` and the other request spans, and `fdc3.connect` for each widget's handshake. Paste a widget record's `fdc3_trace_id` into Tempo's trace search to open the request that delivered that context     |
| Prometheus  | `ot_fdc3_messages_total`, `ot_fdc3_message_duration_seconds_bucket`, `ot_fdc3_connections`, `ot_terminal_windows`, `ot_terminal_popouts`                                                                                                                             |

What to expect in Tempo:

- An app the Desktop Agent opens for an intent or `fdc3.open` shows as `raiseIntentRequest` (or `openRequest`) → `fdc3.launch`. The trace continues with `app.launch` only for apps that run in a OneTerminal window or engine host (`onlineNative` apps); `web` apps open in the default browser, so the trace ends at `fdc3.launch`.
- A Terminal opened from the Desktop Agent window or tray starts its own trace (`process.startup`, service `one-terminal`). Terminals restored when the Desktop Agent starts appear under the agent's `process.startup`.

`ot_app_launches_total` appears once something has gone through `app.launch`.

---

## Troubleshooting

| Symptom                       | Check                                                                                                                                                                                                                                                                                 |
| ----------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Nothing reaches the collector | The startup lines `config telemetry.otlp.* = …` name the endpoint and where it came from; `telemetry.otlp = off` means none was found or `OTEL_SDK_DISABLED` is set                                                                                                                   |
| Only some processes export    | `tauri-webview-host` never exports; plugin binaries and the Electron host don't either                                                                                                                                                                                                |
| No log file                   | A warning at startup names the directory that couldn't be created; the process then logs to stderr only                                                                                                                                                                               |
| Widget records missing        | `window.OT_TELEMETRY` is undefined in the page (not opened by OneTerminal, or `telemetry.widgets.enabled` is false); the page's console shows `[telemetry] could not send logs` (at most once a minute) when the endpoint refuses it; `ot.telemetry.ingress.rejected` counts refusals |
| Too many or too few log lines | `OT_LOG` (see [Configuration](#configuration)); a Terminal started by the Desktop Agent uses the agent's level                                                                                                                                                                        |
