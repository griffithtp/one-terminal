# OneTerminal — Claude Code Context

## Framework identity

- Product name: **OneTerminal**
- Main apps: `one-terminal` (window manager), `desktop-agent` (FDC3 broker + engine launcher)
- Shared Rust crate: `ot-core` (package `packages/ot-core`)
- Shared Rust Tauri plugin: `ot-fdc3` (package `packages/ot-fdc3`)
- Shared browser JS: `fdc3-plugin` (package `packages/fdc3-plugin`)
- All env vars are prefixed `OT_*` (not `FX_*`)

## Workspace layout

```
Cargo workspace root: Cargo.toml
npm workspace root:   package.json

apps/one-terminal/src-tauri   — Tauri 2 window manager
apps/desktop-agent/src-tauri  — Tauri 2 desktop agent
apps/tauri-webview-host/      — thin host binary for pinned WebView2/WKWebView
apps/electron-host/           — thin Electron host for Electron-engine apps
apps/app-directory/           — Express AppD API + React management UI
packages/ot-core/             — shared Rust crate (no Tauri deps)
packages/ot-fdc3/             — shared Tauri plugin: FDC3 2.2 TCP spoke client
packages/fdc3-plugin/         — browser FDC3 agent (fdc3-plugin.js)
packages/fdc3-client/         — TypeScript FDC3 types + Fdc3Agent (requires ot-fdc3)
apps/sample-ticker/           — demo browser app, port 3010
apps/sample-chart-viewer/     — demo browser app, port 3011
```

## Key port defaults (all overridable via env vars)

| Service | Port | Env var |
|---|---|---|
| App Directory | 3005 | — |
| TCP Broker | 7890 | `OT_TCP_PORT` |
| FDC3 Bus WS | 7891 | `OT_FDC3_BUS_PORT` |
| DACP Bridge | 4475 | `OT_DACP_PORT` |
| sample-ticker | 3010 | — |
| sample-chart-viewer | 3011 | — |

## Engine families

`ot_core::engine::EngineFamily` enum:
- `Webview2` — Windows system WebView2
- `Wkwebview` — macOS system WebKit
- `Electron` — Electron via `apps/electron-host`
- `Custom(String)` — open-world variant for plugin engines

`EngineFamily` is NOT `Copy` (contains a `String`). Always `.clone()` when building structs from a `&EngineBinding`.

## Engine plugin manifests

Loaded at `EngineRuntimeStore::new(root)` from `<root>/plugins/*/manifest.json`.

`LaunchMode` variants (in `ot_core::plugin`):
- `InProcess`
- `SpawnTauriHost`
- `SpawnElectronHost`
- `SpawnBinary { binary_name, env_templates }` — template vars: `{{url}}`, `{{title}}`, `{{app_id}}`, `{{runtime_path}}`

## Config loading (desktop-agent)

`apps/desktop-agent/src-tauri/src/config.rs` — `AgentConfig::load()`:
1. Reads `agent.config.json` from next to binary or `resources/`
2. Applies env var overrides (`OT_*`)

User channels are seeded at runtime from `AgentConfig.user_channels` — `ChannelManager::new()` creates an empty map.

## FDC3 Bus URL (fdc3-plugin)

Resolved in order:
1. `<meta name="ot-fdc3-bus-url">` content attribute
2. `window.OT_FDC3_BUS_URL`
3. Fallback: `ws://localhost:7891/fdc3`

## Important naming rules

- Framework = **OneTerminal** (not "One Terminal", not "one-terminal" in prose)
- Container window = **Terminal** (the `one-terminal` app)
- Broker = **Desktop Agent** (the `desktop-agent` app)
- Rust package names: `one-terminal`, `desktop-agent`, `tauri-webview-host`, `ot-core`, `ot-fdc3`
- npm package names: `@one-terminal/*`

## Build / check commands

```sh
cargo check --workspace
cargo test -p desktop-agent engines::router
npm run build:app-directory      # rebuilds apps/app-directory/dist
npm run build:all                # all Tauri apps
```

## Scaffolder commands

```sh
npm run extract-templates        # re-derive EJS templates from apps/ (run after any app change)
npm run build:scaffolder         # compile create-one-terminal to packages/create-one-terminal/dist/
npm run check-template-drift     # CI drift check — exits 1 if templates are out of sync
node packages/create-one-terminal/dist/index.js   # run scaffolder locally
```

Templates at `packages/create-one-terminal/templates/` are always generated — never hand-edit them.

## Testing a scaffolded workspace locally

After changing app source or templates, scaffold a test workspace and verify it builds:

```sh
npm run extract-templates
npm run build:scaffolder
node packages/create-one-terminal/dist/index.js   # answer prompts, note output dir
cd <workspace-name>
cargo check --workspace          # must exit 0
npm install
npm run build:app-directory
```

Then start the services in order: `dev:app-directory` → `dev:desktop-agent` → `dev:terminal`.
