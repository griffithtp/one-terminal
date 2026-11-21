# `ot-telemetry` — agent context

Logging, tracing and opt-in OpenTelemetry export for every OneTerminal binary. Rules:
[docs/conventions/rust.md](../../docs/conventions/rust.md#logging),
[docs/conventions/security.md](../../docs/conventions/security.md).

## Boundaries (do not blur them)

- **No Tauri, no FDC3, no app types.** Binaries depend on this crate; this crate depends
  on nothing in the workspace. FDC3 and app code only call the `tracing` API.
- **Opt-in export.** Nothing leaves the machine unless `config::resolve` finds an
  endpoint (`OT_OTLP_ENDPOINT`, standard `OTEL_EXPORTER_OTLP_*ENDPOINT`, or
  `telemetry.otlp.endpoint`), and `OTEL_SDK_DISABLED=true` always wins. Local log files
  are always written.
- **OpenTelemetry API calls stay in `src/otlp.rs`.** The `opentelemetry*` crates are
  pre-1.0 and change between minors; an upgrade should touch only that file (bump all
  `opentelemetry*` crates together, `tracing-opentelemetry` one minor higher).
- **No reqwest 0.13.** The exporter's built-in clients pull reqwest 0.13, whose rustls
  feature switches the whole workspace to `aws-lc-rs` (a C/CMake build). `otlp.rs` plugs
  in the workspace's reqwest 0.12 blocking client instead. `cargo tree -i aws-lc-rs` must
  stay empty.
- **Secrets never print.** Endpoint URLs go through `redact::url`; headers are
  `Redacted<…>`. Keep the `summary_never_contains_secrets` test passing.

## Modules

| Module           | Responsibility                                                                               |
| ---------------- | -------------------------------------------------------------------------------------------- |
| `config.rs`      | `TelemetryConfig` (the `telemetry` JSON block) and pure `resolve()` — env precedence, opt-in |
| `logs.rs`        | Daily JSON-lines file per service + stderr text; trace ids on every line                     |
| `otlp.rs`        | Feature `otlp`: providers, OTLP/HTTP exporters, the reqwest 0.12 HTTP client                 |
| `propagation.rs` | `TRACEPARENT` parse/inject for child processes (works without `otlp`)                        |
| `paths.rs`       | Per-OS default log directory                                                                 |
| `redact.rs`      | `redact::url`, `Redacted<T>`                                                                 |
| `lib.rs`         | `init`, `Telemetry` (`shutdown` capped at `FLUSH_CAP`), panic hook, startup span             |

## Features

- `otlp` (default): export. `desktop-agent`, `one-terminal` and `app-directory-server`
  enable it.
- Without `otlp` (`tauri-webview-host`): local logs only, no network or TLS code, no
  export threads. Log lines still carry the `TRACEPARENT` trace id.

## Test

```sh
cargo test -p ot-telemetry
cargo test -p ot-telemetry --no-default-features
cargo clippy -p ot-telemetry --no-default-features --all-targets -- -D warnings
```

## Ships to scaffolded workspaces

Extracted into the scaffolder's `shared` overlay (every variant's Terminal uses it). The
standalone root `Cargo.toml` is hand-written in
`packages/create-one-terminal/overlay-overrides/standalone/Cargo.toml.ejs` — mirror
workspace dependency changes there.
