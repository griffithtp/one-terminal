# `ot-core` — agent context

Shared, **Tauri-agnostic** Rust consumed by every workspace member. Never add a `tauri`
dependency here; anything needing an `AppHandle` belongs in the app crates. Rules:
[docs/conventions/rust.md](../../docs/conventions/rust.md).

| Module             | Contents                                                                                                       |
| ------------------ | -------------------------------------------------------------------------------------------------------------- |
| `engine.rs`        | `EngineFamily`, engine bindings, `default_cache_root()`                                                        |
| `plugin.rs`        | Engine plugin manifests, `LaunchMode`                                                                          |
| `electron_host.rs` | Locating / launching the Electron host (`spawn_electron_app_with` lets the caller add env, e.g. trace context) |
| `widget_url.rs`    | Widget URL policy: `parse_widget_url`, `check_widget_url`, `is_allowed_navigation` (see security.md)           |

## Engine families

`ot_core::engine::EngineFamily` enum:

- `Webview2` — Windows system WebView2
- `Wkwebview` — macOS system WebKit
- `Electron` — Electron via `apps/electron-host`
- `Custom(String)` — open-world variant for plugin engines

`EngineFamily` is NOT `Copy` (contains a `String`). Always `.clone()` when building structs from a `&EngineBinding`.

## Engine plugin manifests

Types live here; loading happens in the Desktop Agent at `EngineRuntimeStore::new(root)`
(`apps/desktop-agent/src-tauri/src/engines/runtime.rs`) from `<root>/plugins/*/manifest.json`.

`LaunchMode` variants (in `ot_core::plugin`):

- `InProcess`
- `SpawnTauriHost`
- `SpawnElectronHost`
- `SpawnBinary { binary_name, env_templates }` — template vars: `{{url}}`, `{{title}}`, `{{app_id}}`, `{{runtime_path}}`

## Per-OS paths

`default_cache_root()` honours `OT_ENGINE_CACHE_ROOT`, then resolves per OS
(`~/Library/Application Support/one-terminal/engines`, `%APPDATA%\one-terminal\engines`,
`$XDG_DATA_HOME`/`~/.local/share` on Linux). Keep all three branches when editing.

## Ships to scaffolded workspaces

Extracted into the scaffolder's `shared` overlay — see
[packages/create-one-terminal/AGENTS.md](../create-one-terminal/AGENTS.md).
