# OneTerminal — Cargo Generate Templates

This directory contains [`cargo-generate`](https://github.com/cargo-generate/cargo-generate) templates for scaffolding new Rust crates that integrate into a OneTerminal workspace.

## Available templates

| Template | Description |
|---|---|
| [`tauri-app/`](tauri-app/) | A new Tauri 2 app crate with FDC3 spoke support |

## Prerequisites

Install `cargo-generate` once:

```sh
cargo install cargo-generate
```

## Usage

### From the published monorepo (recommended)

```sh
cargo generate --git https://github.com/one-terminal/one-terminal \
               --subfolder templates/cargo/tauri-app \
               --name my-app
```

### Pinned to a specific release tag

```sh
cargo generate --git https://github.com/one-terminal/one-terminal \
               --tag cargo-template-v0.1.0 \
               --subfolder templates/cargo/tauri-app \
               --name my-app
```

### From a local clone

```sh
cargo generate --path templates/cargo/tauri-app --name my-app
```

## After generation

The template produces a `src-tauri/` crate only. You still need to wire up the npm side manually or use `npm create one-terminal` for a full workspace scaffold.

1. Add the new crate to the workspace `Cargo.toml`:
   ```toml
   members = [
       ...
       "apps/my-app/src-tauri",
   ]
   ```

2. Move the generated directory into `apps/my-app/src-tauri/`.

3. Replace `@YOUR_SCOPE` in `tauri.conf.json` with your npm scope (e.g. `@acme-trading`).

4. Add root scripts to `package.json`:
   ```json
   "dev:my-app":   "npm -w @acme-trading/my-app run tauri:dev",
   "build:my-app": "npm -w @acme-trading/my-app run tauri:build"
   ```

5. Run `cargo check -p my-app` to verify.

## Releasing a new template version

Tag the monorepo with `cargo-template-v<semver>` whenever the templates change materially:

```sh
git tag cargo-template-v0.2.0
git push origin cargo-template-v0.2.0
```
