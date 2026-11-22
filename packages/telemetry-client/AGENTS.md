# `@one-terminal/telemetry` (`packages/telemetry-client`) — agent context

Browser client that relays errors and logs from OneTerminal pages to the Rust process
that owns them. The Rust side, `ot_telemetry::relay`, holds the only copy of the rules
(limits, rate limit, sanitising); the client-side caps here only keep payloads small.
Rules: [docs/conventions/react.md](../../docs/conventions/react.md#errors-and-telemetry),
[docs/conventions/security.md](../../docs/conventions/security.md).

## Boundaries

- **No React, Tauri or FDC3 in `src/index.ts`.** The caller supplies the transport
  (`invoke("wm_telemetry_log" | "cda_telemetry_log", { batch })` in the UIs). React lives
  only in the `./react` subpath (`TelemetryErrorBoundary`), with React as an optional
  peer dependency.
- **Wire format = `ot_telemetry::relay::RecordBatch`.** Change both sides together.
- **A `traceId` is recorded, never joined** as a span parent on the Rust side.
- **No loops:** what the transport logs _synchronously_ (IPC-layer noise) is not
  captured; records from the page while a send is awaited are queued. Transport failures
  go to the original console at most once a minute.
- **One instance per webview:** apps start it through their `startTelemetry()`, which
  disposes a previous instance (Vite HMR) so console wrappers don't stack. The same
  `Error` object is reported once (React logs boundary-caught errors through the console
  too).

## Layout

| Path            | Contents                                                              |
| --------------- | --------------------------------------------------------------------- |
| `src/index.ts`  | `createTelemetry`: batching, queue cap, error/rejection/console hooks |
| `src/react.tsx` | `TelemetryErrorBoundary`                                              |

The UIs import the TypeScript source through the npm workspace (Vite compiles it); there
is no build step.

## Test

```sh
npm run test:telemetry-client
npm -w @one-terminal/telemetry run typecheck
```

## Ships to scaffolded workspaces

Extracted into the scaffolder's `shared` overlay (the Terminal UI imports it) — see
[packages/create-one-terminal/AGENTS.md](../create-one-terminal/AGENTS.md).
