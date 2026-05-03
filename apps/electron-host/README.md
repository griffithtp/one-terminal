# electron-host

A minimal Electron shell that opens a single `BrowserWindow` pointed at a URL supplied via environment variables. It is the Electron-engine counterpart to `apps/tauri-webview-host` — both are spawned by the CDA launcher and the Window Manager when an app requests a specific browser engine.

---

## How it works

The shell reads three environment variables at startup and opens one window:

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `OT_URL` | yes | — | URL to load in the window |
| `OT_TITLE` | no | `"App"` | Window title |
| `OT_APP_ID` | no | `"app"` | App identity — controls the Electron `userData` directory so multiple instances share nothing |

The spawner (CDA or WM) sets these variables and invokes the shell as:

```sh
electron <path-to-electron-host/>
```

The shell quits as soon as its window is closed.

---

## Getting started

### 1. Install the Electron binary

From the **monorepo root**:

```sh
npm run setup:electron-host
```

This runs `npm install -w @one-terminal/electron-host`, which downloads the `electron` npm package (Chromium binary) into the workspace. Because npm workspaces hoist packages, the binary lands at `node_modules/electron/dist/`.

### 2. Smoke-test the shell directly

```sh
OT_URL=https://example.com OT_TITLE="My App" npm run start:electron-host
```

A standalone Electron window opens. Close it and the process exits.

### 3. Launch from the Window Manager

Start the Terminal with the Electron override pointing at this folder:

```sh
npm run dev:terminal:electron
```

This sets `OT_ELECTRON_HOST_OVERRIDE=$INIT_CWD/apps/electron-host` so the Terminal knows where the shell lives during development. When you then open an app whose engine binding is `electron`, the Terminal spawns this shell as a separate process.

Similarly for the Desktop Agent:

```sh
npm run dev:desktop-agent:electron
```

---

## DevTools

Every Electron window launched through this shell supports two keyboard shortcuts to toggle DevTools:

| Shortcut | Platform |
|---|---|
| `F12` | All |
| `Cmd+Shift+I` | macOS |
| `Ctrl+Shift+I` | Windows / Linux |

---

## Binary resolution order

The Rust launchers (`ot_core::electron_host`) resolve the Electron binary in this order:

1. **`OT_ELECTRON_BIN`** env var — point at an explicit binary for testing or CI.
2. **Engine cache** — `<app-data>/one-terminal/engines/electron/<version>/` — populated by the engine picker's download flow for production catalog installs.
3. **`node_modules/electron/dist/`** — walks up from the shell folder to the monorepo root, finding the hoisted npm package. This is the normal dev path after `npm run setup:electron-host`.

The shell folder itself is resolved as:

1. **`OT_ELECTRON_HOST_OVERRIDE`** — set by `dev:terminal:electron` / `dev:desktop-agent:electron` to point at this directory.
2. **`<exe-dir>/electron-host/`** — the ship-alongside layout for release builds.

---

## Relationship to `tauri-webview-host`

| | `electron-host` | `tauri-webview-host` |
|---|---|---|
| Runtime | Electron (Chromium) | Tauri (WKWebView / WebView2) |
| Engine family | `electron` | `webview2` / `wkwebview` |
| Binary | `electron` npm package | `tauri-webview-host` sidecar |
| Pinnable version | via npm / engine catalog | via `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` |
| DevTools | F12 / Cmd+Shift+I | Browser native DevTools |

Both shells receive the same `OT_URL` / `OT_TITLE` / `OT_APP_ID` contract and both quit when the window closes.

---

## Repo layout

```
apps/electron-host/
├── main.js          entry point — reads env vars, opens BrowserWindow, registers DevTools shortcuts
└── package.json     declares electron as a devDependency; "start" script for smoke tests
```
