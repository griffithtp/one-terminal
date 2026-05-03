# OneTerminal — `tauri-app` Cargo Template

Scaffolds a new Tauri 2 app crate for use inside a OneTerminal workspace.

## Quick start

```sh
cargo generate --git https://github.com/one-terminal/one-terminal \
               --subfolder templates/cargo/tauri-app \
               --name order-blotter
```

Or from a local clone:

```sh
cargo generate --path templates/cargo/tauri-app --name order-blotter
```

## Prompts

| Placeholder | Description | Example | Validation |
|---|---|---|---|
| `app_name` | Kebab-case crate name (pre-filled from `--name`) | `order-blotter` | `^[a-z][a-z0-9-]+$` |
| `org_identifier` | Reverse-domain prefix for the Tauri bundle identifier | `com.acme.trading` | dot-separated lowercase segments |
| `dev_port` | Vite dev server port | `1423` | 4–5 digit integer |
| `include_fdc3` | Add `ot-fdc3` FDC3 spoke dependency | `true` | bool |

## What is generated

```
<app_name>/
├── Cargo.toml              — crate manifest, workspace deps, optional ot-fdc3
├── build.rs                — tauri_build::build() (static, never changes)
├── tauri.conf.json         — Tauri 2 config: identifier, devUrl, window label
├── capabilities/
│   └── default.json        — core:default + event permissions for <app_name>-main
├── resources/
│   └── app.config.json     — runtime config with _managed fields for upgrade tool
└── src/
    ├── main.rs             — #![cfg_attr windows_subsystem] + run()
    └── lib.rs              — tauri::Builder setup with devtools in debug
```

## Naming conventions

- Rust lib name: `<app_name | replace('-','_')>_lib` — e.g. `order_blotter_lib`
- Tauri bundle identifier: `<org_identifier>.<app_name>` — e.g. `com.acme.trading.order-blotter`
- Tauri window label: `<app_name>-main`

## Wiring into an existing OneTerminal workspace

After generation, do the following in the workspace root:

**1. Register the crate in `Cargo.toml`:**
```toml
[workspace]
members = [
    ...
    "apps/order-blotter/src-tauri",  # add this line
]
```

**2. Place the generated directory:**
```
apps/
└── order-blotter/
    └── src-tauri/   ← move generated output here
```

**3. Fix the `@YOUR_SCOPE` placeholder in `tauri.conf.json`:**

`cargo generate` cannot read the npm workspace scope, so the `beforeDevCommand` and
`beforeBuildCommand` fields are emitted with a literal `@YOUR_SCOPE` placeholder:

```json
"beforeDevCommand": "npm -w @YOUR_SCOPE/order-blotter run dev"
```

Replace `@YOUR_SCOPE` with your npm scope (e.g. `@acme-trading`).

**4. Add scripts to root `package.json`:**
```json
"dev:order-blotter":   "npm -w @acme-trading/order-blotter run tauri:dev",
"build:order-blotter": "npm -w @acme-trading/order-blotter run tauri:build"
```

**5. Verify:**
```sh
cargo check -p order-blotter
```

## FDC3 spoke (ot-fdc3)

If you answered `true` to `include_fdc3`, the generated `Cargo.toml` includes:

```toml
ot-fdc3 = { workspace = true }
```

Register the plugin in `src/lib.rs`:

```rust
.plugin(ot_fdc3::init())
```

The Desktop Agent must be running for the FDC3 spoke to connect.

## Runtime config (`app.config.json`)

The generated `resources/app.config.json` uses a `_managed` array to mark
framework-owned fields. The `npx create-one-terminal upgrade` command only
writes to these fields, leaving user-customised values untouched:

```json
{
  "_managed": ["appDirectoryUrl", "engineCatalogUrl"],
  "title": "order-blotter",
  "appDirectoryUrl": "http://localhost:3005",
  "engineCatalogUrl": "http://localhost:3005"
}
```

Override at runtime with `OT_APP_DIRECTORY_URL` and `OT_ENGINE_CATALOG_URL`
environment variables (loaded by `AgentConfig::load()` in the Desktop Agent).

## Template variables reference (Liquid)

| Variable | Type | Example output |
|---|---|---|
| `{{app_name}}` | string | `order-blotter` |
| `{{app_name \| replace: '-', '_'}}` | string | `order_blotter` |
| `{{org_identifier}}` | string | `com.acme.trading` |
| `{{dev_port}}` | string | `1423` |
| `{{include_fdc3}}` | bool | `true` |
