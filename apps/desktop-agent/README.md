# Desktop Agent

The Desktop Agent is the central broker for a OneTerminal session. It runs as a Tauri 2 application (system-tray only, no visible window by default) and owns four subsystems:

| Subsystem          | Purpose                                                                         |
| ------------------ | ------------------------------------------------------------------------------- |
| **TCP Broker**     | Accepts spoke connections from Tauri-based apps via `ot-fdc3` plugin            |
| **FDC3 Bus**       | WebSocket server for browser-based apps using `fdc3-plugin.js`                  |
| **DACP Bridge**    | Cross-agent WebSocket bridge — connects peer Desktop Agents on the same machine |
| **Engine Runtime** | Downloads, caches, and launches pinned browser-engine runtimes                  |

The dashboard UI (`apps/desktop-agent/src/`) opens in a webview on demand and visualises connected spokes, channels, intents, and bridge peers.

---

## Starting the agent

```sh
# Development (hot-reload)
npm run dev:desktop-agent

# Production build
npm run build:desktop-agent
```

Start order for a full session: `dev:app-directory` → `dev:desktop-agent` → `dev:terminal`.

---

## Configuration

### File location

`AgentConfig::load()` searches for `agent.config.json` in this order, using the first file found:

1. `<binary-dir>/agent.config.json` — release deployments
2. `<binary-dir>/resources/agent.config.json` — packaged Tauri resources
3. `<cwd>/resources/agent.config.json` — `cargo run` from the workspace root
4. `<cwd>/src-tauri/resources/agent.config.json` — `tauri dev` from the app directory

The resolved path is printed to stdout on startup:

```
[desktop-agent] config loaded from /path/to/agent.config.json
```

If no file is found, built-in defaults are used (identical to the sample below).

### Annotated example

```jsonc
{
  // Title shown in the dashboard window and system tray tooltip.
  "title": "OneTerminal Desktop Agent",

  // FDC3 App Directory REST base URL. Used to fetch the app list and
  // to power the engine catalog endpoint.
  "appDirectoryUrl": "http://localhost:3005",

  // URL for the engine catalog JSON (usually the same App Directory).
  "engineCatalogUrl": "http://localhost:3005",

  "ports": {
    "tcpBroker": 7890, // Tauri-app spokes connect here (ot-fdc3 plugin)
    "fdc3Bus": 7891, // Browser-app spokes connect here (fdc3-plugin.js)
    "dacpBridge": 4475, // Cross-agent peer WebSocket server
  },

  // User channels seeded at startup. Add, remove, or reorder freely.
  "userChannels": [
    { "id": "Red", "displayName": "Red", "color": "#E74C3C" },
    { "id": "Orange", "displayName": "Orange", "color": "#E67E22" },
    { "id": "Yellow", "displayName": "Yellow", "color": "#F1C40F" },
    { "id": "Green", "displayName": "Green", "color": "#2ECC71" },
    { "id": "Teal", "displayName": "Teal", "color": "#1ABC9C" },
    { "id": "Blue", "displayName": "Blue", "color": "#3498DB" },
    { "id": "Purple", "displayName": "Purple", "color": "#9B59B6" },
    { "id": "Pink", "displayName": "Pink", "color": "#EC407A" },
  ],
}
```

### Environment variable overrides

All `OT_*` variables are applied on top of the file values after loading.

| Variable           | Overrides          | Default                       |
| ------------------ | ------------------ | ----------------------------- |
| `OT_TITLE`         | `title`            | `"OneTerminal Desktop Agent"` |
| `OT_APP_DIR_URL`   | `appDirectoryUrl`  | `http://localhost:3005`       |
| `OT_CATALOG_URL`   | `engineCatalogUrl` | `http://localhost:3005`       |
| `OT_TCP_PORT`      | `ports.tcpBroker`  | `7890`                        |
| `OT_FDC3_BUS_PORT` | `ports.fdc3Bus`    | `7891`                        |
| `OT_DACP_PORT`     | `ports.dacpBridge` | `4475`                        |

### Port conflict detection

At startup, before the Tauri runtime initialises, the agent probes all three ports with a short-lived `TcpListener::bind`. If any port is already bound it prints all conflicts and exits immediately:

```
[desktop-agent] startup aborted: port conflict(s) detected
  port 7890 is already in use — set OT_TCP_PORT=<port> to override
  port 7891 is already in use — set OT_FDC3_BUS_PORT=<port> to override
```

---

## TCP Spoke Protocol

Tauri-based apps connect using the `ot-fdc3` plugin, which opens a TCP connection to `127.0.0.1:7890`. All messages are newline-terminated JSON, discriminated by the `"type"` field.

### Handshake

The first message from a spoke must be `Hello`. The agent replies with `Welcome` and registers the connection.

```jsonc
// Spoke → Agent
{ "type": "hello", "app_id": "ticker-plant", "display_name": "Ticker Plant" }

// Agent → Spoke
{
  "type": "welcome",
  "instance_id": "a1b2c3d4-...",
  "channels": [
    { "channelId": "Red", "displayName": "Red", "color": "#E74C3C" },
    ...
  ]
}
```

### Inbound messages (spoke → agent)

| `type`                   | Required fields                   | Description                                                |
| ------------------------ | --------------------------------- | ---------------------------------------------------------- |
| `hello`                  | `app_id`                          | Opens the session. Must be first. `display_name` optional. |
| `join_channel`           | `channel_id`                      | Join a user channel, leaving any current one.              |
| `leave_channel`          | —                                 | Leave the current channel.                                 |
| `broadcast`              | `channel_id`, `context`           | Broadcast a context to all other members of the channel.   |
| `raise_intent`           | `intent`, `context`, `request_id` | Raise a named intent. `target_instance_id` optional.       |
| `add_intent_listener`    | `intent`                          | Register this spoke as a handler for the intent.           |
| `remove_intent_listener` | `intent`                          | Deregister.                                                |
| `ping`                   | —                                 | Keep-alive. Agent replies with `pong`.                     |

### Outbound messages (agent → spoke)

| `type`              | Trigger                                                 | Description                                                                                         |
| ------------------- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `welcome`           | After `hello`                                           | Session established. Carries channel list.                                                          |
| `channel_joined`    | After `join_channel`                                    | ACK.                                                                                                |
| `channel_left`      | After `leave_channel`                                   | ACK.                                                                                                |
| `context_broadcast` | When another member broadcasts                          | Carries `channel_id`, `context`, `source_instance_id`, `source_app_id`.                             |
| `intent_delivery`   | When a raised intent targets this spoke                 | Carries `intent`, `context`, `source_instance_id`, `request_id`.                                    |
| `intent_resolved`   | When a handler is found for an intent this spoke raised | Carries `handler_instance_id`, `handler_app_id`, `request_id`.                                      |
| `error`             | On protocol error                                       | `code` is one of `CHANNEL_NOT_FOUND`, `NO_INTENT_HANDLERS`, `MALFORMED_MESSAGE`, `HANDLER_TIMEOUT`. |
| `pong`              | After `ping`                                            | Keep-alive reply.                                                                                   |

---

## FDC3 Bus (browser apps)

Browser apps include `fdc3-plugin.js` and connect via WebSocket to `ws://127.0.0.1:7891/fdc3`. The bus uses the same broker internals as the TCP layer — a connected browser app is indistinguishable from a TCP spoke in the channel and intent registries.

The WebSocket URL is resolved in the browser in this order:

1. `<meta name="ot-fdc3-bus-url">` content attribute
2. `window.OT_FDC3_BUS_URL`
3. Fallback: `ws://localhost:7891/fdc3`

See [`packages/fdc3-plugin/README.md`](../../packages/fdc3-plugin/README.md) for the full browser-side API.

---

## DACP Bridge

The Desktop Agent Connectivity Protocol (DACP) enables multiple Desktop Agent instances on the same machine to exchange contexts and resolve intents across their boundaries.

### Peer discovery

The agent uses two complementary mechanisms, running every 30 seconds:

1. **Port scan** — probes `127.0.0.1:4475` through `4575` for TCP listeners. Any open port is assumed to be a DACP bridge and a WebSocket connection is attempted.
2. **Discovery files** — reads `$TMPDIR/fdc3/bridges/*.json`. Each file contains `{ "url": "ws://127.0.0.1:<port>/v2/bridge", ... }`. On startup the agent writes its own file; on shutdown it removes it.

A URL is only connected to once: `try_claim_url` is atomic and returns `false` for URLs already tracked.

### Bridge endpoint

Inbound peer connections arrive on `ws://127.0.0.1:<dacpBridge>/v2/bridge`.

---

## Engine Runtime

The engine runtime subsystem downloads, verifies, and caches browser engine binaries. The cache is shared with `apps/one-terminal` so engines installed by either app are visible to both.

### Engine families

| Family         | Description                                                                               |
| -------------- | ----------------------------------------------------------------------------------------- |
| `Webview2`     | Windows system WebView2. Pinned versions are downloaded and run via `tauri-webview-host`. |
| `Wkwebview`    | macOS system WebKit. Always in-process; no pinning needed.                                |
| `Electron`     | Electron. Launched via `apps/electron-host`. See [Electron mode](#electron-mode).         |
| `Custom(name)` | Plugin-declared family. Requires a manifest in the engine cache.                          |

### Cache location

Runtimes are stored in the platform app-data directory under `engines/<family>/<version>/`. The exact path is printed to stdout when an engine is first installed.

### App Directory engine bindings

An app can declare per-OS engine bindings in its App Directory record:

```jsonc
{
  "appId": "my-app",
  "type": "onlineNative",
  "details": { "url": "https://my-app.example.com" },
  "engineBindings": {
    "windows": [{ "family": "Webview2", "version": "124.0.2478.97" }],
    "macos": [{ "family": "Wkwebview", "version": "system" }],
  },
}
```

If multiple bindings exist for the current OS, a picker dialog is shown in the Terminal header.

### Launch decision table

| Binding                   | `system` flag | Outcome                                        |
| ------------------------- | ------------- | ---------------------------------------------- |
| None / missing            | —             | `InProcessWebview` (uses current Tauri engine) |
| `Webview2` or `Wkwebview` | `true`        | `InProcessWebview`                             |
| `Webview2`                | `false`       | `SpawnTauriHost` with pinned runtime folder    |
| `Electron`                | any           | `SpawnElectronHost`                            |
| `Custom(name)`            | any           | Resolved from plugin manifest `LaunchMode`     |

### Plugin manifests

Drop a `manifest.json` into `<engine-cache>/plugins/<name>/` to register a custom engine. The `launchMode` field drives the launch decision. Template variables available in `envTemplates`: `{{url}}`, `{{title}}`, `{{app_id}}`, `{{runtime_path}}`.

Example (`plugins/cef/manifest.json`):

```json
{
  "family": "cef",
  "label": "Chromium Embedded Framework",
  "supportedPlatforms": ["windows", "macos", "linux"],
  "launchMode": {
    "type": "spawnBinary",
    "binaryName": "cef-host",
    "envTemplates": [
      { "key": "CEF_URL", "value": "{{url}}" },
      { "key": "CEF_TITLE", "value": "{{title}}" },
      { "key": "CEF_APP_ID", "value": "{{app_id}}" },
      { "key": "CEF_RUNTIME_PATH", "value": "{{runtime_path}}" }
    ]
  }
}
```

---

## Electron mode

Running apps in Electron requires extra setup not needed for WebView2/WKWebView.

**One-time setup:**

```sh
npm run setup:electron-host
```

**Start with Electron engine support:**

```sh
OT_ELECTRON_HOST_OVERRIDE=$PWD/apps/electron-host npm run dev:desktop-agent
# or use the root convenience script:
npm run dev:desktop-agent:electron
```

`OT_ELECTRON_HOST_OVERRIDE` must point to the `apps/electron-host` directory (the folder containing `package.json`, not the binary). In release builds, the electron-host is expected to be a sibling of the desktop-agent binary.

Additional overrides for Electron:

| Variable                    | Purpose                                                         |
| --------------------------- | --------------------------------------------------------------- |
| `OT_ELECTRON_HOST_OVERRIDE` | Path to the `electron-host` directory (dev only)                |
| `OT_ELECTRON_BIN`           | Explicit path to the Electron binary (overrides catalog lookup) |

---

## Tauri IPC commands

These commands are callable from the dashboard UI via `invoke()`.

| Command                                                     | Returns               | Description                                                  |
| ----------------------------------------------------------- | --------------------- | ------------------------------------------------------------ |
| `cda_list_connections`                                      | `WindowHandle[]`      | All connected spoke instances                                |
| `cda_list_channels`                                         | `ChannelInfo[]`       | All channels with member lists                               |
| `cda_get_spoke_channel(instance_id)`                        | `string \| null`      | Channel a spoke is currently on                              |
| `cda_disconnect_spoke(instance_id)`                         | `boolean`             | Force-disconnect a spoke                                     |
| `cda_broadcast_from_dashboard(channel_id, context)`         | `number`              | Broadcast from the dashboard itself; returns recipient count |
| `cda_list_peers`                                            | `PeerInfo[]`          | Connected bridge peers                                       |
| `cda_list_intent_handlers`                                  | `IntentHandlerInfo[]` | All intent handlers (local + bridged)                        |
| `cda_raise_intent_from_dashboard(intent, context, target?)` | `string`              | Raise intent; returns `request_id`                           |
| `cda_list_app_directory`                                    | `AppRecord[]`         | Current App Directory snapshot                               |
| `cda_refresh_app_directory`                                 | `number`              | Re-fetch App Directory; returns app count                    |
| `cda_launch_app(app_id, app_url, app_type, ...)`            | `void`                | Launch an app from the directory                             |
| `engine_catalog`                                            | `EngineCatalog`       | Current engine catalog snapshot                              |
| `engine_refresh_catalog`                                    | `number`              | Re-fetch engine catalog; returns entry count                 |
| `engine_list_installed`                                     | `InstalledEngine[]`   | Runtimes present in the on-disk cache                        |
| `engine_ensure(binding)`                                    | `InstalledEngine`     | Ensure a runtime is installed, downloading if needed         |

---

## Tauri events

The agent emits these events to the dashboard window (`desktop-agent-dashboard`).

| Event              | Payload fields                                                              | Trigger                                 |
| ------------------ | --------------------------------------------------------------------------- | --------------------------------------- |
| `cda:connected`    | `instanceId`, `appId`, `displayName`, `connectedAt`                         | A spoke completes the `Hello` handshake |
| `cda:disconnected` | `instanceId`, `appId`                                                       | A spoke TCP connection closes           |
| `cda:context`      | `channelId`, `context`, `sourceInstanceId`, `sourceAppId`, `recipientCount` | A context broadcast is routed           |
| `cda:intent`       | `intent`, `sourceInstanceId`, `handlerInstanceId`, `requestId`              | An intent is delivered to a handler     |

---

## System tray

The tray icon is always visible while the agent is running. The context menu provides:

- **Open CDA** — show the dashboard window (or focus it if already open)
- **Launch Terminal** — spawn a new `one-terminal` window manager instance
- **Launch App** (submenu) — one item per app in the App Directory; refreshes after `cda_refresh_app_directory`
- **Exit** — terminate the agent

The dashboard window hides (rather than closes) when its close button is clicked, keeping the agent alive in the tray.

---

## Resilience

The current behaviour on unexpected shutdown is minimal by design:

- **Desktop Agent crash** — connected TCP spokes receive an EOF on their connection. The `ot-fdc3` plugin does not auto-reconnect; spoke apps will lose their session and need to be restarted. There is no reconnection logic in `packages/ot-fdc3`.
- **Spoke disconnect** — the agent deregisters the spoke from all channel and intent registries immediately and emits `cda:disconnected`. Other spokes are unaffected.
- **App Directory / engine catalog unavailable at startup** — both fetches are non-fatal. The agent starts with an empty app list and retries on the next manual `cda_refresh_app_directory` call or tray refresh.
- **DACP peer disconnect** — the peer is removed from the `PeerRegistry`. The discovery loop will attempt to reconnect on the next 30-second scan if the peer's discovery file is still present.
