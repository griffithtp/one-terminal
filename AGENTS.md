# OneTerminal — AI agent context

This is the canonical context file for every AI coding assistant (Claude Code, GitHub
Copilot, Devin, Codex, Cursor, …). GitHub Copilot and the others read it directly; the
`CLAUDE.md` files are generated `@AGENTS.md` imports for Claude Code — edit this file, then
run `npm run sync:ai`. See [.ai/README.md](.ai/README.md).

Area-specific context lives in nested `AGENTS.md` files next to the code they describe;
long-form rules live in [docs/conventions/](docs/conventions/). Read the conventions doc
for the area you are changing before you change it.

## Framework identity

- Product name: **OneTerminal**
- Main apps: `one-terminal` (window manager), `desktop-agent` (FDC3 broker + engine launcher)
- Shared Rust crate: `ot-core` (package `packages/ot-core`)
- Shared browser JS: `fdc3-plugin` (package `packages/fdc3-plugin`): the official FDC3 proxy over a WebSocket; `fdc3-plugin.js` is a generated, committed bundle of `src/`
- All env vars are prefixed `OT_*` (not `FX_*`)
- Target platforms: **Windows (WebView2) and macOS (WKWebView)** are first-class; Linux must compile

## Naming rules

- Framework = **OneTerminal** (not "One Terminal", not "one-terminal" in prose)
- Container window = **Terminal** (the `one-terminal` app)
- Broker = **Desktop Agent** (the `desktop-agent` app)
- Rust package names: `one-terminal`, `desktop-agent`, `tauri-webview-host`, `ot-core`
- npm package names: `@one-terminal/*`

These names are also keys in the scaffolder's substitution table
(`scripts/extract-templates.ts`) — renaming one in source means updating that table too.

## Workspace layout

```
Cargo workspace root: Cargo.toml
npm workspace root:   package.json

apps/one-terminal/            — Terminal: React UI in src/                  (AGENTS.md)
apps/one-terminal/src-tauri   — Tauri 2 window manager                      (AGENTS.md)
apps/desktop-agent/src-tauri  — Tauri 2 desktop agent                       (AGENTS.md)
apps/tauri-webview-host/      — thin host binary for pinned WebView2/WKWebView
apps/electron-host/           — thin Electron host for Electron-engine apps
apps/app-directory/           — React management UI for the App Directory (Vite; no server)
apps/app-directory-server/    — standalone App Directory binary (hosted / Docker)
packages/ot-core/             — shared Rust crate (no Tauri deps)            (AGENTS.md)
packages/ot-appdirectory/     — Rust (axum) FDC3 AppD REST API; mounted in-process by desktop-agent
packages/fdc3-plugin/         — browser FDC3 3.0 client (src/ → fdc3-plugin.js)   (AGENTS.md)
packages/create-one-terminal/ — the scaffolder (`create-one-terminal`)     (AGENTS.md)
apps/sample-ticker/           — demo browser app, port 3010
apps/sample-chart/            — demo browser app, port 3011
apps/sample-widget/           — demo local widget (widgets.config.json), port 3012
```

## Conventions

| Area                   | Doc                                                                      |
| ---------------------- | ------------------------------------------------------------------------ |
| General (start here)   | [docs/conventions/general.md](docs/conventions/general.md)               |
| Rust                   | [docs/conventions/rust.md](docs/conventions/rust.md)                     |
| React / TypeScript     | [docs/conventions/react.md](docs/conventions/react.md)                   |
| Windows + macOS        | [docs/conventions/cross-platform.md](docs/conventions/cross-platform.md) |
| Security               | [docs/conventions/security.md](docs/conventions/security.md)             |
| FDC3                   | [docs/conventions/fdc3.md](docs/conventions/fdc3.md)                     |
| Scaffolder / templates | [docs/conventions/scaffolder.md](docs/conventions/scaffolder.md)         |

## Agents and workflows

Specialist roles live in [.ai/agents/](.ai/agents/) and step-by-step workflows in
[.ai/skills/](.ai/skills/). They are generated into `.claude/agents/` and `.claude/skills/`,
which both Claude Code and GitHub Copilot in VS Code read; any other assistant can follow
the `.ai/` file directly. Run the matching agent before you call a change done:

| Change touches                                                                                        | Agent                                                                                             |
| ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| A new feature or change request (start here)                                                          | [requirements-analyst](.ai/agents/requirements-analyst.md) → [architect](.ai/agents/architect.md) |
| `src-tauri/`, `ot-core`, engine hosts, window management, keybindings, scripts, CI                    | [cross-platform-reviewer](.ai/agents/cross-platform-reviewer.md)                                  |
| `broker/`, `fdc3_bus/`, `app_dir/`, `packages/fdc3-plugin/src`, user channels                         | [fdc3-conformance](.ai/agents/fdc3-conformance.md)                                                |
| Anything in `apps/` or `packages/ot-*` that ships to scaffolded workspaces                            | [scaffolder-steward](.ai/agents/scaffolder-steward.md)                                            |
| Capabilities, CSP, Tauri commands, custom schemes, FDC3 bus, App Directory, spawning, downloads, deps | [security-auditor](.ai/agents/security-auditor.md)                                                |
| Any Rust (`*.rs`, `Cargo.toml`)                                                                       | [rust-reviewer](.ai/agents/rust-reviewer.md)                                                      |
| Any React / TypeScript UI (`apps/*/src`, `apps/app-directory/ui`)                                     | [react-reviewer](.ai/agents/react-reviewer.md)                                                    |
| Missing or failing tests, or a bug needing a regression test                                          | [test-author](.ai/agents/test-author.md)                                                          |
| Bundle size, startup, layout/drag render cost, popouts, FDC3 frame handling, or a failing budget      | [performance-analyst](.ai/agents/performance-analyst.md)                                          |

The requirements analyst writes only `docs/plans/`; the architect and reviewers report and don't edit files; the scaffolder steward drafts and (when
asked) applies migrations.

| Workflow                                                   | Use when                                                                                           |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| [feature](.ai/skills/feature/SKILL.md)                     | Any non-trivial feature: plan → design → implement → test → review → handover, with approval gates |
| [framework-upgrade](.ai/skills/framework-upgrade/SKILL.md) | A framework change is done and must reach scaffolded workspaces                                    |
| [perf-audit](.ai/skills/perf-audit/SKILL.md)               | Before merging UI or hot-path changes: bundle sizes and timings vs `main`                          |

Cloud agents (Copilot coding agent, Devin, Codex, Cursor): machine setup is
`bash scripts/agent-setup.sh`; see [.ai/cloud-agents.md](.ai/cloud-agents.md).

## Generated files — never edit by hand

| File(s)                                                             | Regenerate with                                             |
| ------------------------------------------------------------------- | ----------------------------------------------------------- |
| `packages/create-one-terminal/templates/**`, `static-manifest.json` | `npm run create-migration` (or `npm run extract-templates`) |
| `packages/fdc3-plugin/fdc3-plugin.js`                               | `npm run build:fdc3-plugin`                                 |
| `CLAUDE.md` (every one)                                             | `npm run sync:ai`                                           |
| `.claude/agents/*`                                                  | `npm run sync:ai` (from `.ai/agents/`)                      |
| `.claude/skills/*`                                                  | `npm run sync:ai` (from `.ai/skills/`)                      |

CI fails if any of these drift from their source.

## Changing framework code that ships to scaffolded workspaces

Most of `apps/` and `packages/ot-*` is extracted into the scaffolder's templates. After
changing those, templates drift and CI's template-drift check fails until you run
`npm run create-migration` (interactive, or `--plan` / `--spec` for agents). Follow the
[framework-upgrade](.ai/skills/framework-upgrade/SKILL.md) workflow. See
[packages/create-one-terminal/AGENTS.md](packages/create-one-terminal/AGENTS.md) and
[docs/conventions/scaffolder.md](docs/conventions/scaffolder.md).

## Key port defaults (all overridable via env vars)

| Service              | Port                    | Env var                                               |
| -------------------- | ----------------------- | ----------------------------------------------------- |
| App Directory        | 3005                    | `OT_APP_DIR_URL` (agent) / `PORT` (standalone server) |
| FDC3 Bus WS          | 7891                    | `OT_FDC3_BUS_PORT`                                    |
| Instance discovery   | ephemeral (OS-assigned) | —                                                     |
| sample-ticker        | 3010                    | —                                                     |
| sample-chart         | 3011                    | —                                                     |
| sample-widget        | 3012                    | —                                                     |
| App Directory UI dev | 5174                    | — (`npm run dev:app-directory`, proxies to 3005)      |

## Environment variables

`OT_*` vars marked Desktop Agent are overrides applied after `agent.config.json` is loaded.
The standalone App Directory server (`apps/app-directory-server`) reads its vars from the process environment (Docker, shell, or `.env`).

| Variable               | Component     | Purpose                                                                                                 |
| ---------------------- | ------------- | ------------------------------------------------------------------------------------------------------- |
| `OT_FDC3_BUS_PORT`     | Desktop Agent | FDC3 Bus WS port (default 7891)                                                                         |
| `OT_APP_DIR_URL`       | both          | App Directory URL (default `http://localhost:3005`)                                                     |
| `OT_CATALOG_URL`       | both          | Engine catalog URL (default `http://localhost:3005`)                                                    |
| `OT_APPD_EMBEDDED`     | Desktop Agent | `0` = don't mount the in-process App Directory                                                          |
| `OT_WEBVIEW_POOL_SIZE` | one-terminal  | Pre-warm pool size (default 1; 0 = disabled)                                                            |
| `OT_NEW_TERMINAL`      | one-terminal  | Open one blank Terminal instead of restoring the saved session (set by the Desktop Agent's Open / tray) |
| `OT_ENGINE_CACHE_ROOT` | both          | Override engine cache root directory                                                                    |
| `PORT`                 | App Directory | Standalone server listen port (default 3005)                                                            |
| `CORS_ORIGIN`          | App Directory | Comma-separated allowed origins                                                                         |

`OT_APPD_TOKEN` is read only by the scaffolder's `new-widget` wizard (sent as a bearer token when it registers the widget; the server doesn't check it yet).

Not yet implemented (planned in [Plan 03](docs/plans/03-app-directory-roles-permissions.md) / [Plan 04](docs/plans/04-native-app-launch.md)): `OT_NATIVE_APP_ALLOW_PATHS`, `APPD_DB_PATH`, `APPD_JWT_SECRET`, `APPD_ALLOW_ANONYMOUS_READ`, `APPD_AUTH_PLUGIN_URL` (Auth Provider plugin on 4477, OIDC redirect on 4476). The App Directory store is in-memory, seeded from `packages/ot-appdirectory/src/data.rs`.

## FDC3 architecture (summary)

Widgets run `@finos/fdc3-agent-proxy` (bundled in `fdc3-plugin.js`) over one WebSocket speaking FDC3 3.0 DACP. `apps/desktop-agent/src-tauri/src/broker/` holds all FDC3 rules with no socket or Tauri dependencies; `fdc3_bus/` only moves frames and performs the broker's `Effect`s. Agent Bridging is not implemented. Details: [docs/conventions/fdc3.md](docs/conventions/fdc3.md), [docs/fdc3.md](docs/fdc3.md).

## Build / check commands

```sh
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo test -p desktop-agent engines::router
cargo test -p desktop-agent broker
npm run lint                      # ESLint (flat config, eslint.config.js)
npm run format                    # Prettier
npm run typecheck
npm run build:fdc3-plugin        # regenerates packages/fdc3-plugin/fdc3-plugin.js after editing src/
npm run test:fdc3-plugin         # contract tests: the official proxy against the real Rust bus
npm run test:terminal            # Terminal UI unit/component tests (Vitest + RTL, jsdom)
npm run build:app-directory      # rebuilds apps/app-directory/dist
npm run build:samples            # production builds of sample-ticker + sample-chart (after Epic 07)
npm run build:all                # all Tauri apps
npm run sync:ai                  # regenerate AI-assistant adapter files from AGENTS.md
npm run check:ai-drift           # CI: fail if adapter files are stale
npm run check:bundle-size        # first-paint JS/CSS of every UI vs scripts/bundle-budgets.json (build the UIs first)
cargo deny check                 # advisories, licenses, sources (deny.toml; CI: audit.yml)
npm audit --omit=dev             # npm advisories in production dependencies
```

## Before you call a change done

1. `cargo fmt --all` and `npm run format` — lefthook's pre-push and CI reject unformatted code.
2. `cargo clippy --workspace --all-targets -- -D warnings` and `npm run lint` are clean.
3. `npm run typecheck` passes if you touched TypeScript.
4. Tests for the area you touched pass (`cargo test -p <crate>`, `npm run test:fdc3-plugin` for FDC3, `npm run test:terminal` for the Terminal UI) — and changed behaviour has a test.
5. Code behind `#[cfg(target_os = …)]` has a branch (or explicit no-op) for both Windows and macOS — see [cross-platform.md](docs/conventions/cross-platform.md).
6. Generated files above are regenerated, not hand-edited.

## Performance conventions

- Vite apps: define `manualChunks` to separate vendor code. Entry chunk target: < 60 KB gzipped.
- First-paint budgets (entry, initial JS, initial CSS per HTML entry) are enforced in CI by
  `npm run check:bundle-size` against `scripts/bundle-budgets.json`. The initial-JS/CSS
  budgets are ratchets: lower them when a change shrinks a bundle; raising one needs a
  reason in the PR.
- Static asset responses: set `Cache-Control: public, max-age=31536000, immutable` on content-hashed filenames; `no-cache` on `index.html`.
- Service Workers: register in all web apps for offline-capable asset caching (production builds only).
- Webview pool: `OT_WEBVIEW_POOL_SIZE` controls the pre-warm pool in `one-terminal` (default 1).

## Framework plugins (planned, Epic 08)

Framework plugins are distinct from engine plugins. They will live in `<data_dir>/one-terminal/plugins/<name>/` and declare a `plugin.json` (not `manifest.json`); engine plugins declare a `manifest.json` and only add a browser runtime. Framework plugins run as Node.js sidecars and can extend the Desktop Agent, App Directory, and browser API. Planned packages (not yet in the repo): `packages/plugin-sdk/` (`@one-terminal/plugin-sdk`), `packages/plugin-themes/`, `packages/plugin-auth-provider/`, `packages/plugin-permissions/`. Browser API: `window.OT.plugins.get(capabilityName)`. See [Plan 08](docs/plans/08-plugin-system.md).

## Enhancement plans

Detailed implementation plans for the next development phase are in `docs/plans/`.

| Plan                        | File                                                                                                         | Milestone |
| --------------------------- | ------------------------------------------------------------------------------------------------------------ | --------- |
| 01 — Widget Headers         | [docs/plans/01-customisable-widget-headers.md](docs/plans/01-customisable-widget-headers.md)                 | Phase 1   |
| 02 — Faster Loading         | [docs/plans/02-faster-widget-loading.md](docs/plans/02-faster-widget-loading.md)                             | Phase 3   |
| 03 — App Directory Auth     | [docs/plans/03-app-directory-roles-permissions.md](docs/plans/03-app-directory-roles-permissions.md)         | Phase 1   |
| 04 — Native App Launch      | [docs/plans/04-native-app-launch.md](docs/plans/04-native-app-launch.md)                                     | Phase 3   |
| 05 — Ticker View Chart      | [docs/plans/05-ticker-view-chart-enhancement.md](docs/plans/05-ticker-view-chart-enhancement.md)             | Phase 2   |
| 06 — Command Palette        | [docs/plans/06-command-palette-hotkeys.md](docs/plans/06-command-palette-hotkeys.md)                         | Phase 2   |
| 07 — Deployment             | [docs/plans/07-deployment-and-hosting.md](docs/plans/07-deployment-and-hosting.md)                           | Phase 4   |
| 08 — Plugin System          | [docs/plans/08-plugin-system.md](docs/plans/08-plugin-system.md)                                             | Phase 5   |
| 09 — Dashboards & Terminals | [docs/dashboards/README.md](docs/dashboards/README.md)                                                       | Phase 2–5 |
| 10 — Terminal App Menu      | [docs/plans/10-terminal-app-menu.md](docs/plans/10-terminal-app-menu.md)                                     | Phase 2   |
| 11 — Widget Channel Scoping | [docs/plans/11-widget-channel-scoping.md](docs/plans/11-widget-channel-scoping.md)                           | Phase 2   |
| 12 — Widget Keep-Alive      | [docs/plans/12-widget-keep-alive-across-dashboards.md](docs/plans/12-widget-keep-alive-across-dashboards.md) | Phase 2   |
| 13 — Generic Web Widget     | [docs/plans/13-generic-web-widget.md](docs/plans/13-generic-web-widget.md)                                   | Phase 2   |
| 14 — Dashboard Tab Menu     | [docs/plans/14-dashboard-tab-context-menu.md](docs/plans/14-dashboard-tab-context-menu.md)                   | Phase 2   |
| 15 — Shared Dashboards      | [docs/plans/15-shared-dashboards-across-terminals.md](docs/plans/15-shared-dashboards-across-terminals.md)   | Phase 3   |
| 16 — Desktop Agent Select   | [docs/plans/16-desktop-agent-selection.md](docs/plans/16-desktop-agent-selection.md)                         | Phase 3   |
| 17 — Widget Pop-out         | [docs/plans/17-widget-popout.md](docs/plans/17-widget-popout.md)                                             | Phase 3   |

See [docs/plans/README.md](docs/plans/README.md) for execution order, effort estimates, and GitHub Project setup instructions.
