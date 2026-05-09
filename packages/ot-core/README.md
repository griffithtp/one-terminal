# ot-core

Shared Rust crate for the OneTerminal framework. Contains domain logic and types that are used by multiple apps (`desktop-agent`, `one-terminal`) without pulling in Tauri or any app-specific dependency.

---

## Modules

### `engine` — Browser Engine Types

```rust
use ot_core::engine::{EngineFamily, EngineBinding, OsKey, default_cache_root, binding_path, is_installed};
```

Shared types for browser engine management. Tauri-free — safe to use in any crate.

| Item                          | Description                                                                                           |
| ----------------------------- | ----------------------------------------------------------------------------------------------------- |
| `EngineFamily`                | `Webview2 \| Wkwebview \| Electron` — serialised as lowercase strings                                 |
| `OsKey`                       | `Windows \| Macos \| Linux`                                                                           |
| `EngineBinding`               | `{ family: EngineFamily, version: String }` — e.g. `electron@29.3.0` or `wkwebview@system`            |
| `is_system_version(v)`        | Returns `true` when `version == "system"` (case-insensitive)                                          |
| `default_cache_root()`        | Platform cache directory for downloaded engine runtimes. Respects `OT_ENGINE_CACHE_ROOT` env override |
| `binding_path(root, binding)` | `<root>/<family>/<version>/` — the on-disk folder for a specific runtime                              |
| `is_installed(dir)`           | `true` if the `.installed` sentinel file exists in `dir`                                              |
| `INSTALL_SENTINEL`            | `".installed"` — written by install flows to mark a complete runtime                                  |

**Cache root locations** (unless `OT_ENGINE_CACHE_ROOT` is set):

| OS      | Default path                                                                     |
| ------- | -------------------------------------------------------------------------------- |
| macOS   | `~/Library/Application Support/one-terminal/engines/`                            |
| Windows | `%APPDATA%\one-terminal\engines\`                                                |
| Linux   | `$XDG_DATA_HOME/one-terminal/engines/` or `~/.local/share/one-terminal/engines/` |

Both the Terminal (`engines.rs`) and the Desktop Agent (`engines/runtime.rs`) use the same cache root so a runtime downloaded by either app is immediately available to the other.

---

### `electron_host` — Electron Shell Launcher

```rust
use ot_core::electron_host::{locate_electron_shell, locate_electron_binary, spawn_electron_app};
```

Shared launcher for `apps/electron-host/`. Both the Desktop Agent and the Terminal call `spawn_electron_app` when an app requests the Electron engine; this module owns all path resolution so both call sites stay aligned.

| Function                                                      | Description                                                                                                                                                                  |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `locate_electron_shell()`                                     | Resolves the `apps/electron-host/` folder. Checks `OT_ELECTRON_HOST_OVERRIDE` first, then `<exe-dir>/electron-host/`                                                         |
| `locate_electron_binary(binding, cache_root, shell_dir)`      | Resolves the Electron executable. Checks `OT_ELECTRON_BIN`, then the engine cache, then walks up from `shell_dir` through ancestor `node_modules/electron/dist/` directories |
| `electron_binary_in(dir)`                                     | Finds the platform Electron executable inside an unpacked dist folder                                                                                                        |
| `spawn_electron_app(binding, cache_root, app_id, url, title)` | Full launch: resolves shell + binary, then spawns `<bin> <shell-dir>` with `OT_URL` / `OT_TITLE` / `OT_APP_ID` set                                                           |

**Binary resolution order** (inside `locate_electron_binary`):

1. `OT_ELECTRON_BIN` env var — explicit path override
2. `<cache_root>/electron/<version>/` — catalog-installed runtime (production)
3. Walk up from `shell_dir` to find `node_modules/electron/dist/` — picks up the npm-hoisted package in the monorepo root after `npm run setup:electron-host`

**Environment variables** read by `locate_electron_shell` / `locate_electron_binary`:

| Variable                    | Purpose                                                          |
| --------------------------- | ---------------------------------------------------------------- |
| `OT_ELECTRON_HOST_OVERRIDE` | Override shell folder path (dev: point at `apps/electron-host/`) |
| `OT_ELECTRON_BIN`           | Override Electron binary path directly                           |

---

## Adding to a new crate

```toml
# In your app's src-tauri/Cargo.toml
[dependencies]
ot-core = { path = "../../../packages/ot-core" }
```

The crate has no Tauri dependency and compiles cleanly in any Rust binary or library.
