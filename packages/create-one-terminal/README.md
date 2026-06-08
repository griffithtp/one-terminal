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

| Prompt                   | Example                            | Notes                                                                                  |
| ------------------------ | ---------------------------------- | -------------------------------------------------------------------------------------- |
| Workspace name           | `acme-trading`                     | kebab-case, used as the npm scope and Cargo workspace name                             |
| Output folder            | `./acme-trading`                   | defaults to `./<workspace-name>`                                                       |
| Workspace variant        | `standalone` (default)             | see [Variants](#variants) below                                                        |
| Tauri bundle identifier  | `com.acme.trading`                 | reverse-domain, dot-separated lowercase segments                                       |
| External FDC3 agent URL  | `ws://prod-fdc3.example:7891/fdc3` | Standalone only — leave blank to wire up later                                         |
| Include FDC3 integration | Yes                                | Enterprise only — Standalone always includes the FDC3 browser client                   |
| Customize default ports  | No                                 | optional — Standalone asks only for the Terminal dev port; Enterprise asks for all six |

After confirming, the CLI renders the workspace and prints next steps.

### Variants

OneTerminal scaffolds in one of two shapes:

- **Standalone** _(default)_ — Terminal + a single sample widget. No Desktop Agent, no App Directory. The Terminal acts as an FDC3 _client_ and joins an external agent over a WebSocket URL you provide. Best for teams that already run an FDC3 agent or want to evaluate the Terminal shell on its own.
- **Enterprise** — full stack: Terminal, Desktop Agent (broker, FDC3 bus, DACP), App Directory (Express API + React UI), engine plugin runtime, and the `sample-ticker` / `sample-chart` demos. Best for platform teams standing up an in-house FDC3 estate.

Both variants share the Terminal shell, `ot-core`, and `fdc3-plugin` (the browser FDC3 client). The TypeScript `fdc3-client` package is Enterprise-only — it invokes `fdc3_*` Tauri commands that only exist when the bundled `ot-fdc3` plugin ships. The Terminal's widget catalog source switches automatically (`widgets.config.json` on Standalone, App Directory HTTP on Enterprise).

### Adding a widget

After scaffolding, generate additional widget apps with the bundled subcommand:

```sh
npx create-one-terminal add-widget my-widget
```

The wizard asks for a title, picks the next free port (starting at 3010), and:

- On **Standalone** — appends an entry to `widgets.config.json`. Restart the Terminal to see it in the launcher.
- On **Enterprise** — prints a `curl` command to register the widget with the App Directory. If `OT_APPD_TOKEN` and `OT_APP_DIR_URL` are set, the wizard makes the call for you.

### Default ports

| Service                | Default | Variant    | Env var            |
| ---------------------- | ------- | ---------- | ------------------ |
| Terminal Vite dev      | 1422    | both       | —                  |
| Desktop Agent Vite dev | 1421    | enterprise | —                  |
| App Directory          | 3005    | enterprise | —                  |
| TCP Broker             | 7890    | enterprise | `OT_TCP_PORT`      |
| FDC3 Bus WebSocket     | 7891    | enterprise | `OT_FDC3_BUS_PORT` |
| DACP Bridge            | 4475    | enterprise | `OT_DACP_PORT`     |
| Sample widget          | 3012    | standalone | —                  |

### Scaffolded layout

**Standalone:**

```
<workspace>/
├── Cargo.toml                    # slim Rust workspace
├── package.json                  # npm workspace
├── widgets.config.json           # local widget registry — the launcher reads this
├── apps/
│   ├── one-terminal/             # Tauri 2 window manager (the Terminal)
│   └── sample-widget/            # minimal demo widget (port 3012)
└── packages/
    ├── ot-core/                  # shared Rust crate
    └── fdc3-plugin/              # browser FDC3 agent (fdc3-plugin.js)
```

**Enterprise:**

```
<workspace>/
├── Cargo.toml                    # full Rust workspace
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

`*` Only included when FDC3 integration is enabled (Enterprise prompt only). `fdc3-plugin` always ships in Standalone; `fdc3-client` is Enterprise-only because it depends on Tauri commands registered by `ot-fdc3`.

### Starting the workspace

**Standalone:**

```sh
cd <workspace>
npm install
npm run dev:sample-widget   # http://localhost:3012
npm run dev:terminal        # separate terminal
```

**Enterprise:**

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
    "version": "0.1.6",
    "scaffoldedAt": "2026-06-07",
    "variant": "standalone"
  }
}
```

The `variant` field is used by the upgrade wizard to filter migrations — only migrations whose `appliesTo` matches the workspace variant (or is `"both"`) are applied.

Do not remove or edit this block manually.

---

## License

MIT
