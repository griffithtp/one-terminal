# OneTerminal

A reusable desktop container framework built on [Tauri 2](https://tauri.app) that hosts browser-based applications in native windows, connected via an [FDC3 2.2](https://fdc3.finos.org) desktop agent.

---

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  OneTerminal  (apps/one-terminal)                   │
│  Window manager — hosts app webviews in tabs        │
│  Reads App Directory to discover available apps     │
└────────────────┬────────────────────────────────────┘
                 │ TCP 7890
┌────────────────▼────────────────────────────────────┐
│  Desktop Agent  (apps/desktop-agent)                │
│  FDC3 2.2 broker — channels, intents, DACP         │
│  Manages engine runtime downloads + launches        │
│  ┌─────────────────────────────────────────────┐    │
│  │  Engine Cache  <app_data>/engines/          │    │
│  │    wkwebview/<ver>/   webview2/<ver>/       │    │
│  │    electron/<ver>/    <plugin-family>/      │    │
│  └─────────────────────────────────────────────┘    │
└────────────────┬──────────────────┬─────────────────┘
                 │ WS 7891          │ UDP/WS 4475
        ┌────────▼──────┐  ┌────────▼──────┐
        │  fdc3-plugin  │  │  DACP bridge  │
        │  (browser JS) │  │  (multi-inst) │
        └───────────────┘  └───────────────┘
```

**Packages:**

| Path | Purpose |
|---|---|
| `apps/one-terminal` | Window manager (Tauri 2 app) |
| `apps/desktop-agent` | FDC3 desktop agent + engine launcher |
| `apps/tauri-webview-host` | Thin Tauri shell spawned for pinned WebView2/WKWebView apps |
| `apps/electron-host` | Thin Electron shell spawned for Electron-engine apps |
| `apps/app-directory` | Express server serving FDC3 AppD REST API |
| `packages/ot-core` | Tauri-agnostic shared Rust crate (engine abstraction, plugin manifests) |
| `packages/fdc3-plugin` | Browser-side FDC3 2.2 client (`DesktopAgentClient`) |
| `packages/fdc3-client` | TypeScript FDC3 type definitions |

**Sample apps** (development / demo):

| Path | Port | Description |
|---|---|---|
| `apps/sample-ticker` | 3010 | Market data ticker — broadcasts `fx.rate` on the Green channel |
| `apps/sample-chart` | 3011 | Candlestick chart — mock EUR/USD stream, handles `ViewChart`/`ViewQuote` intents |

---

## Quick Start

### Prerequisites

| Tool | Version |
|---|---|
| [Rust](https://rustup.rs) | stable ≥ 1.77 |
| [Node.js](https://nodejs.org) | ≥ 20 LTS |
| Xcode Command Line Tools | macOS only — `xcode-select --install` |

### Run in development

```sh
# 1. Install all JS dependencies
npm install

# 2. App Directory (FDC3 AppD REST API + management UI)
npm run dev:app-directory        # http://localhost:3005

# 3. Sample apps (optional)
npm run dev:sample-ticker          # http://localhost:3010
npm run dev:sample-chart           # http://localhost:3011

# 4. Desktop Agent (separate terminal)
npm run dev:desktop-agent

# 5. OneTerminal window manager (separate terminal)
npm run dev:terminal
```

> The first `cargo` build takes 1–2 minutes. Subsequent incremental builds are much faster.

### Build for production

```sh
npm run build:terminal         # OneTerminal window manager
npm run build:desktop-agent    # Desktop Agent
npm run build:all              # All Tauri apps in sequence
```

---

## Configuration

The Desktop Agent reads `agent.config.json` (bundled as a resource) and applies environment variable overrides on top.

### `agent.config.json`

Located at `apps/desktop-agent/src-tauri/resources/agent.config.json`:

```json
{
  "title": "OneTerminal Desktop Agent",
  "appDirectoryUrl": "http://localhost:3005/v2/apps",
  "engineCatalogUrl": "http://localhost:3005/v2/engines",
  "ports": {
    "tcpBroker": 7890,
    "fdc3Bus": 7891,
    "dacpBridge": 4475
  },
  "userChannels": [
    { "id": "Red",    "displayName": "Red",    "color": "#E74C3C" },
    { "id": "Orange", "displayName": "Orange", "color": "#E67E22" },
    { "id": "Yellow", "displayName": "Yellow", "color": "#F1C40F" },
    { "id": "Green",  "displayName": "Green",  "color": "#2ECC71" },
    { "id": "Teal",   "displayName": "Teal",   "color": "#1ABC9C" },
    { "id": "Blue",   "displayName": "Blue",   "color": "#3498DB" },
    { "id": "Purple", "displayName": "Purple", "color": "#9B59B6" },
    { "id": "Pink",   "displayName": "Pink",   "color": "#EC407A" }
  ]
}
```

### Environment variable overrides

| Variable | Overrides |
|---|---|
| `OT_TITLE` | Window title |
| `OT_APP_DIR_URL` | `appDirectoryUrl` |
| `OT_CATALOG_URL` | `engineCatalogUrl` |
| `OT_TCP_PORT` | `ports.tcpBroker` |
| `OT_FDC3_BUS_PORT` | `ports.fdc3Bus` |
| `OT_DACP_PORT` | `ports.dacpBridge` |

---

## Browser Engine Support

OneTerminal supports four engine families:

| Family | Description |
|---|---|
| `wkwebview` | macOS system WebKit — always available, no download |
| `webview2` | Windows system WebView2 — always available, no download |
| `electron` | Electron — requires `apps/electron-host` to be set up |
| _custom_ | Any engine via a plugin manifest (see below) |

### Selecting an engine per app

In the App Directory record, set `hostManifests.oneTerminal.engine`:

```json
{
  "appId": "my-app",
  "hostManifests": {
    "oneTerminal": {
      "engine": { "family": "electron", "version": "29.3.0" }
    }
  }
}
```

Omit `engine` (or set `"version": "system"`) to use the OS-default WebView.

### Setting up Electron

```sh
npm run setup:electron-host            # install electron binary once
npm run dev:terminal:electron          # OneTerminal with Electron support
npm run dev:desktop-agent:electron     # Desktop Agent with Electron support
```

### Engine plugin manifests

Drop a `manifest.json` into `<engine-cache>/plugins/<family>/` to register a new engine family without recompiling:

```json
{
  "family": "cef",
  "label": "Chromium Embedded Framework",
  "supportedPlatforms": ["windows", "macos", "linux"],
  "launchMode": {
    "type": "spawnBinary",
    "binaryName": "cef-host",
    "envTemplates": [
      { "key": "CEF_URL",          "value": "{{url}}" },
      { "key": "CEF_TITLE",        "value": "{{title}}" },
      { "key": "CEF_APP_ID",       "value": "{{app_id}}" },
      { "key": "CEF_RUNTIME_PATH", "value": "{{runtime_path}}" }
    ]
  }
}
```

**`launchMode.type` values:**

| Type | Behaviour |
|---|---|
| `inProcess` | Render inside the Desktop Agent Tauri webview |
| `spawnTauriHost` | Spawn `tauri-webview-host` binary |
| `spawnElectronHost` | Spawn `apps/electron-host` via Electron |
| `spawnBinary` | Spawn an arbitrary binary with env vars from `envTemplates` |

**Template variables** in `envTemplates.value`:

| Variable | Resolved value |
|---|---|
| `{{url}}` | App URL from App Directory |
| `{{title}}` | App display name |
| `{{app_id}}` | FDC3 `appId` |
| `{{runtime_path}}` | Absolute path to the downloaded engine runtime |

A sample CEF manifest is shipped at `apps/desktop-agent/src-tauri/resources/plugins/cef/manifest.json`.

---

## Managing the App Directory

The App Directory (`apps/app-directory`) serves the FDC3 AppD REST API and ships a management UI. There are three ways to add, edit, or remove apps.

### Option 1 — Management UI (recommended for quick changes)

Start the server and open the UI in a browser:

```sh
npm run dev:app-directory    # http://localhost:3005
```

- **Add** — click **Register Application** and fill in the form.
- **Edit** — click the pencil icon on any listed app.
- **Delete** — click the trash icon.

> **Note:** The server stores apps in memory. Changes made through the UI (or the REST API) are lost when the server restarts. To persist them, copy the updated records into `apps/app-directory/src/data.ts` (see Option 2).

### Option 2 — Edit `data.ts` (persistent)

Edit `apps/app-directory/src/data.ts` and add or update entries in the `_apps` array. Required fields are `appId`, `name`, `type`, and `details.url`:

```typescript
{
  appId:   "my-app",
  name:    "My App",
  type:    "web",
  details: { url: "http://localhost:4000" },
  title:   "My App",
  version: "1.0.0",
}
```

To pin a specific browser engine, add a `hostManifests.oneTerminal.engine` block (see [Browser Engine Support](#browser-engine-support)).

After editing, rebuild the static dist so the changes are bundled into the production binary:

```sh
npm run build:app-directory
```

During development (`npm run dev:app-directory`) the server hot-reloads and serves directly from source — no rebuild needed.

### Option 3 — REST API

The server exposes a standard FDC3 AppD CRUD API while it is running:

| Method | Path | Action |
|---|---|---|
| `GET` | `/v2/apps` | List all apps (supports `?$filter=`) |
| `GET` | `/v2/apps/:appId` | Get one app |
| `POST` | `/v2/apps` | Register a new app |
| `PUT` | `/v2/apps/:appId` | Replace an existing app |
| `DELETE` | `/v2/apps/:appId` | Remove an app |

```sh
# Add an app
curl -X POST http://localhost:3005/v2/apps \
  -H "Content-Type: application/json" \
  -d '{"appId":"my-app","name":"My App","type":"web","details":{"url":"http://localhost:4000"}}'

# Edit an app
curl -X PUT http://localhost:3005/v2/apps/my-app \
  -H "Content-Type: application/json" \
  -d '{"appId":"my-app","name":"My App","type":"web","details":{"url":"http://localhost:4001"}}'

# Delete an app
curl -X DELETE http://localhost:3005/v2/apps/my-app
```

Same in-memory caveat applies — persist changes to `data.ts` to survive a restart.

---

## FDC3 Integration (Browser Apps)

Browser apps use `fdc3-plugin.js` as the FDC3 desktop agent proxy over WebSocket.

```html
<script type="module">
import { DesktopAgentClient } from '/fdc3-plugin.js';

const fdc3 = await DesktopAgentClient.connect('my-app-id');

// Join a user channel
await fdc3.joinUserChannel('Green');

// Broadcast context
await fdc3.broadcast({
  type: 'fdc3.instrument',
  id:   { ticker: 'EUR/USD' },
  name: 'EUR/USD',
});

// Listen for context
await fdc3.addContextListener('fdc3.instrument', (ctx) => { /* ... */ });

// Raise an intent
await fdc3.raiseIntent('ViewChart', context);

// Handle an intent
await fdc3.addIntentListener('ViewChart', (ctx, meta) => { /* ... */ });
</script>
```

The plugin connects to `ws://localhost:7891/fdc3` by default. Override via:

```html
<!-- HTML meta tag -->
<meta name="ot-fdc3-bus-url" content="ws://myhost:7891/fdc3" />
```

```js
// Or global before the module loads
window.OT_FDC3_BUS_URL = 'ws://myhost:7891/fdc3';
```

---

## Development Notes

### Dev overrides

```sh
# Skip engine download — use a local runtime directory
OT_ENGINE_RUNTIME_OVERRIDE=/path/to/runtime npm run dev:desktop-agent

# Use a local electron-host checkout
OT_ELECTRON_HOST_OVERRIDE=$PWD/apps/electron-host npm run dev:terminal
```

### Rust workspace

```sh
cargo check --workspace
cargo test -p desktop-agent engines::router
```

### Shared crate: `ot-core`

`packages/ot-core` contains:

| Module | Contents |
|---|---|
| `ot_core::engine` | `EngineFamily`, `EngineBinding`, cache-path helpers, install sentinel |
| `ot_core::plugin` | `EngineManifest`, `LaunchMode`, template expansion, plugin-dir scanner |
| `ot_core::electron_host` | Electron binary/shell resolution, spawn helpers |

Both `desktop-agent` and `one-terminal` depend on it via `{ workspace = true }`.

---

## Repository Structure

```
one-terminal/
├── apps/
│   ├── one-terminal/          Window manager (Tauri 2)
│   ├── desktop-agent/         FDC3 2.2 desktop agent + engine launcher (Tauri 2)
│   ├── app-directory/         FDC3 AppD REST API + management UI (Express + React)
│   ├── tauri-webview-host/    Minimal Tauri host for pinned WebView2/WKWebView apps
│   ├── electron-host/         Minimal Electron host for Electron-engine apps
│   ├── sample-ticker/         Sample: ticker plant browser app (port 3010)
│   └── sample-chart/          Sample: candlestick chart viewer browser app (port 3011)
├── packages/
│   ├── ot-core/               Shared Rust crate (engine abstraction, plugin manifests)
│   ├── fdc3-plugin/           Browser FDC3 client (WebSocket transport)
│   └── fdc3-client/           TypeScript FDC3 types
├── Cargo.toml                 Rust workspace
└── package.json               npm workspace root
```
