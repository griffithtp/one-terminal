# create-one-terminal

Scaffold and upgrade [OneTerminal](https://github.com/griffithtchenpan/one-terminal) workspaces — a Tauri 2 + FDC3 window-manager framework for financial desktop applications.

```sh
npm create one-terminal@latest
```

## Requirements

- Node.js 18 or later
- Rust + Cargo (for the Tauri apps)
- [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS

---

## Creating a new workspace

```sh
npm create one-terminal@latest
# or
npx create-one-terminal
```

The CLI walks you through a short set of prompts:

| Prompt | Example | Notes |
|---|---|---|
| Workspace name | `acme-trading` | kebab-case, used as the npm scope and Cargo workspace name |
| Output folder | `./acme-trading` | defaults to `./<workspace-name>` |
| Tauri bundle identifier | `com.acme.trading` | reverse-domain, dot-separated lowercase segments |
| Include FDC3 integration | Yes | adds `ot-fdc3`, `fdc3-client`, and `fdc3-plugin` |
| Customize default ports | No | optional — see port table below |

After confirming, the CLI renders the full monorepo and prints next steps.

### Default ports

| Service | Default | Notes |
|---|---|---|
| App Directory | 3005 | Express AppD API + management UI |
| TCP Broker | 7890 | `OT_TCP_PORT` |
| FDC3 Bus WebSocket | 7891 | `OT_FDC3_BUS_PORT` |
| DACP Bridge | 4475 | `OT_DACP_PORT` |
| Terminal Vite dev | 1422 | |
| Desktop Agent Vite dev | 1421 | |

All ports can be customized during scaffolding or overridden at runtime via `OT_*` environment variables.

### Scaffolded layout

```
<workspace>/
├── Cargo.toml                    # Rust workspace
├── package.json                  # npm workspace
├── apps/
│   ├── one-terminal/             # Tauri 2 window manager (the Terminal)
│   ├── desktop-agent/            # Tauri 2 FDC3 broker + engine launcher
│   ├── app-directory/            # Express AppD API + React management UI
│   ├── tauri-webview-host/       # thin host for pinned WKWebView/WebView2
│   ├── electron-host/            # thin Electron host for Electron-engine apps
│   ├── sample-ticker/            # demo app (port 3010)
│   └── sample-chart/             # demo app (port 3011)
└── packages/
    ├── ot-core/                  # shared Rust crate (no Tauri deps)
    ├── ot-fdc3/                  # shared Tauri plugin: FDC3 2.2 TCP spoke client  *
    ├── fdc3-client/              # TypeScript FDC3 types + Fdc3Agent               *
    └── fdc3-plugin/              # browser FDC3 agent (fdc3-plugin.js)             *
```

`*` Only included when FDC3 integration is enabled.

### Starting the workspace

```sh
cd <workspace>
npm install
npm run dev:app-directory   # start first
npm run dev:desktop-agent   # start second
npm run dev:terminal        # start last
```

---

## Upgrading an existing workspace

```sh
npx create-one-terminal upgrade
```

Run this inside an existing OneTerminal workspace. The CLI:

1. Reads the current framework version from `package.json` (`oneTerminal.version`)
2. Builds the migration chain from your version to the latest
3. Shows a summary of each migration and asks for confirmation
4. Creates a snapshot of all affected files (used for rollback on failure)
5. Applies migrations in order — three migration types are supported:
   - **`dep-bump`** — updates dependency versions in `package.json` or `Cargo.toml`
   - **`config-merge`** — deep-merges a JSON patch into a config file
   - **`structural`** — adds new files, or inserts/replaces lines in existing files by pattern
6. Writes `upgrade-report.md` listing every migration's outcome (applied / skipped / needs-manual-review)
7. Updates `oneTerminal.version` in `package.json`

If any migration fails, all changes are rolled back atomically from the snapshot.

---

## Version tracking

After scaffolding, your workspace `package.json` contains a metadata block that the upgrade command reads:

```json
{
  "oneTerminal": {
    "version": "0.1.2",
    "scaffoldedAt": "2026-05-04"
  }
}
```

Do not remove or edit this block manually.

---

## License

MIT
