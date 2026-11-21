# Desktop Agent (Rust) — agent context

The Desktop Agent is the FDC3 3.0 broker, engine launcher and (by default) host of the
in-process App Directory. Rules: [docs/conventions/fdc3.md](../../../docs/conventions/fdc3.md),
[docs/conventions/rust.md](../../../docs/conventions/rust.md),
[docs/conventions/security.md](../../../docs/conventions/security.md).
User-facing FDC3 docs: [docs/fdc3.md](../../../docs/fdc3.md).

## Module map

| Path                         | Responsibility                                                                                                                                     |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/broker/`                | **All FDC3 rules.** Synchronous, no sockets, no Tauri. `Broker::handle` applies one DACP message and returns `Effect`s. Tests in `broker/tests.rs` |
| `src/fdc3_bus/`              | WebSocket server on `127.0.0.1:<OT_FDC3_BUS_PORT>/fdc3`. Moves frames, runs the WCP4/WCP5 handshake, performs `Effect`s. No FDC3 logic             |
| `src/engines/`               | Engine catalog, runtime cache (`<app_data>/engines/<family>/<version>/`), launch routing (`router.rs`)                                             |
| `src/app_dir/`               | Slim FDC3 3.0 AppD types (`AppRecord`), `fdc3Version` range matching, `AppDirectoryCache` the broker resolves against                              |
| `src/fdc3_bus/instrument.rs` | Spans (`fdc3.request` per message, `fdc3.launch`), FDC3 trace-id ↔ span linking, bus metrics. The broker knows nothing about telemetry             |
| `src/metrics.rs`             | App-launch metrics (`ot.app.launches`, `ot.app.launch.duration`)                                                                                   |
| `src/config.rs`              | `AgentConfig::load_quiet()` (layering below), including the `telemetry` block                                                                      |
| `src/lib.rs`                 | App setup, tray, process lifecycle (spawned child PIDs, kill on quit), embedded App Directory mount                                                |
| `../src/`                    | React dashboard UI for the Desktop Agent window                                                                                                    |

## Broker / bus boundary (do not blur it)

- New FDC3 behaviour goes in `broker/`, expressed as state changes plus `Effect`s. If you
  need I/O, add an `Effect` variant and perform it in `fdc3_bus/` — never import tokio
  networking, axum or Tauri into `broker/`.
- Response-before-event ordering matters to the official `DesktopAgentProxy`; keep it.
- Every broker rule change gets a test in `broker/tests.rs` and must keep
  `npm run test:fdc3-plugin` green.

## Engine families

`ot_core::engine::EngineFamily`: `Webview2`, `Wkwebview`, `Electron`, `Custom(String)`.
It is **not `Copy`** — `.clone()` when building structs from a `&EngineBinding`. Engine
plugin manifests and `LaunchMode`: see [packages/ot-core/AGENTS.md](../../../packages/ot-core/AGENTS.md).

## Config loading — `src/config.rs`, `AgentConfig::load()`

Layers, later overriding earlier (objects merge per key, arrays replace; files may be partial):

1. Compiled-in defaults
2. Bundled `agent.config.json` (next to binary or `resources/`)
3. User `agent.config.json` in the per-user `one-terminal` config dir — macOS `~/Library/Application Support/one-terminal/`, Windows `%APPDATA%\one-terminal\`, Linux `$XDG_CONFIG_HOME/one-terminal/` (or `~/.config/one-terminal/`)
4. Env var overrides (`OT_*`)

Unknown keys are warned about and ignored. Every effective setting and its source layer is
logged at startup. A new setting needs: a default, a JSON key, an `OT_*` override if it is
operationally useful, a row in the root `AGENTS.md` env-var table, and a test.

User channels are seeded at runtime from `AgentConfig.user_channels` — `ChannelManager::new()` creates an empty map.

## Process lifecycle is OS-specific

`lib.rs` kills spawned children with `taskkill /PID [/F]` on Windows and `kill -TERM|-KILL`
elsewhere. Any change to spawning or shutdown must handle both.

## Test

```sh
cargo test -p desktop-agent broker
cargo test -p desktop-agent engines::router
npm run test:fdc3-plugin
```

## Ships to scaffolded workspaces

Extracted into the scaffolder's `enterprise` overlay — see
[packages/create-one-terminal/AGENTS.md](../../../packages/create-one-terminal/AGENTS.md).
