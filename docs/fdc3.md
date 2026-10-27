# FDC3 Desktop Agent connectivity

This page explains how OneTerminal widgets find an FDC3 Desktop Agent, how users choose between several agents, and how the Desktop Agent itself is configured.

For the client libraries (`fdc3-plugin`, `fdc3-client`, `ot-fdc3`), see the [root README](../README.md#-fdc3-client-libraries-fdc3-plugin-vs-fdc3-client-vs-ot-fdc3).

- [How a widget connects](#how-a-widget-connects)
- [Choosing a Desktop Agent in the Terminal](#choosing-a-desktop-agent-in-the-terminal)
- [Connector kinds](#connector-kinds)
- [Desktop Agent configuration](#desktop-agent-configuration)

---

## How a widget connects

Each widget runs in its own panel webview and loads `fdc3-plugin`, which opens a WebSocket to a Desktop Agent's FDC3 bus. The plugin uses the first non-empty value from this list:

1. `<meta name="ot-fdc3-bus-url">` in the widget's page
2. `window.OT_FDC3_AGENT_URL`, which the Terminal injects into every panel webview when it creates it
3. `window.OT_FDC3_BUS_URL` (legacy override)
4. `ws://localhost:7891/fdc3`, the default bus of the bundled Desktop Agent

If the Terminal injects an empty string (`window.OT_FDC3_AGENT_URL = ""`), FDC3 is turned off on purpose. `connect()` then fails straight away with `NoAgentConfigured`, and the error names the setting to change.

The injected value is fixed for the life of a webview. That's why switching agents recreates the panel webviews (see [Switching](#switching)).

---

## Choosing a Desktop Agent in the Terminal

All widgets in a Terminal process use the same agent. Splitting one window's widgets across agents would quietly break channel linking, because a broadcast only reaches widgets on the same bus.

### Configuring agents

Add agents in `terminal.config.json`:

```jsonc
"fdc3": {
  "defaultAgent": "uat",
  "allowUserOverride": true,
  "agents": [
    { "id": "uat",  "name": "OneTerminal UAT",  "env": "uat",  "url": "ws://uat-host:7891/fdc3" },
    { "id": "prod", "name": "OneTerminal Prod", "env": "prod", "url": "wss://prod-host/fdc3" },
    { "id": "io",   "name": "io.Connect",       "env": "prod", "kind": "interopio", "url": "ws://localhost:8385" }
  ]
}
```

| Field               | Meaning                                                                                                      |
| ------------------- | ------------------------------------------------------------------------------------------------------------ |
| `agents[].id`       | Stable id. The saved choice and `defaultAgent` refer to it. `local`, `config`, `none` and `env` are reserved |
| `agents[].name`     | Display name                                                                                                 |
| `agents[].env`      | Environment label, used for grouping. `prod` is highlighted in red                                           |
| `agents[].kind`     | Connector kind, default `onet`. See [Connector kinds](#connector-kinds)                                      |
| `agents[].url`      | Agent endpoint                                                                                               |
| `defaultAgent`      | Agent used when the user hasn't chosen one                                                                   |
| `allowUserOverride` | `false` ignores the user's saved choice and makes the picker read-only. Default `true`                       |
| `agentUrl`          | Legacy single-agent setting. Still supported. `""` turns FDC3 off                                            |

The list also always has two built-in entries:

- **Local Desktop Agent:** `ws://localhost:7891/fdc3`. When this is selected, nothing is injected, so the page's own meta tag still applies.
- **No agent (FDC3 off).**

A non-empty legacy `agentUrl` adds a third entry, **Configured agent**.

### Which agent is used

The first match wins:

1. The `OT_FDC3_AGENT_URL` env var. It forces the agent, and the picker becomes read-only.
2. The user's saved choice, unless `allowUserOverride` is `false`.
3. `defaultAgent`.
4. The legacy `agentUrl`.
5. The first configured agent, otherwise Local Desktop Agent.

The Terminal logs the result and its source at startup:

```
[agent] Desktop Agent: OneTerminal UAT = ws://uat-host:7891/fdc3 (source: terminal.config.json)
```

### Saved choice

The user's choice is stored in `agent-selection.json` in the Terminal's app data directory. On macOS that's `~/Library/Application Support/com.one-terminal.one-terminal/`. The Rust side reads it at startup, because panel webviews are restored before the frontend loads.

- **Shared across processes:** every Terminal process for the user reads the same file, but only at startup. A process that's already running keeps its agent until it restarts, or until the user switches inside it.
- **Unreachable agent:** the Terminal stays on it and shows it as disconnected. It never quietly falls back, because that could move a user from UAT to Prod without them noticing.
- **Agent removed from config:** the Terminal uses the default, shows a one-time notice, and deletes the saved file.

### Switching

Users can switch from:

- **App Menu → Desktop Agent**
- the header badge (agent name and a reachability dot, red for Prod)
- the **Switch Desktop Agent…** command in the palette

Switching to a Prod agent asks for an extra confirmation.

A switch:

1. saves the choice
2. closes and re-creates every panel webview in every window of the process, keeping the same labels and the new URL, and re-applies each panel's saved FDC3 channel
3. closes parked keep-alive webviews, which are rebuilt when their dashboard is next opened
4. drains and refills the pre-warmed webview pool
5. emits `wm:agent-changed` with the new listing

Widgets reload, so any unsaved state inside them is lost.

The reachability dot only checks that a TCP connection to the agent's host and port opens. It doesn't confirm the FDC3 handshake succeeds.

### Implementation

| Piece                                       | Location                                                            |
| ------------------------------------------- | ------------------------------------------------------------------- |
| Choosing the agent, saved choice, commands  | `apps/one-terminal/src-tauri/src/agent.rs`                          |
| Config types (`Fdc3Config`, `AgentProfile`) | `apps/one-terminal/src-tauri/src/config.rs`                         |
| Frontend bindings and hooks                 | `apps/one-terminal/src/lib/desktopAgents.ts`                        |
| App Menu section                            | `apps/one-terminal/src/components/sections/DesktopAgentSection.tsx` |
| Header badge                                | `AgentBadge` in `apps/one-terminal/src/components/Header.tsx`       |

Tauri commands:

- `wm_agent_list`
- `wm_agent_select` (`id: null` resets to the default)
- `wm_agent_probe`

---

## Connector kinds

`kind` says which protocol a widget needs to reach the agent.

| Kind                                | Status                     | Notes                                                     |
| ----------------------------------- | -------------------------- | --------------------------------------------------------- |
| `onet`                              | Supported                  | OneTerminal's `fdc3:*` WebSocket protocol (`fdc3-plugin`) |
| `openfin`, `interopio`, `finsemble` | Listed, cannot be selected | Needs a vendor connector                                  |

Third-party agents don't speak the `onet` protocol, and their FDC3 API is normally only available to apps running inside their own container. Reaching them will need one of these:

- a vendor connector inside `fdc3-plugin`, behind the standard `window.fdc3` API
- FDC3 Desktop Agent Bridging through a local OneTerminal Desktop Agent

Which route works depends on each vendor's support for outside connections, their authentication and their licensing.

---

## Desktop Agent configuration

The Desktop Agent builds its config in layers. Each layer overrides the one before it. Objects merge key by key and arrays are replaced, so a file only needs the keys it changes.

1. Compiled-in defaults
2. Bundled `agent.config.json` (next to the binary, or in the app resources)
3. User `agent.config.json`:
   - macOS: `~/Library/Application Support/one-terminal/`
   - Windows: `%APPDATA%\one-terminal\`
   - Linux: `$XDG_CONFIG_HOME/one-terminal/`, or `~/.config/one-terminal/`
4. `OT_*` environment variables

For example, to move the FDC3 bus to another port, the user file only needs:

```json
{ "ports": { "fdc3Bus": 7999 } }
```

Unknown keys are warned about and ignored. If the user file has a value of the wrong type, the agent drops that file and still starts. At startup the agent logs every setting and the layer it came from:

```
[desktop-agent] config ports.fdc3Bus = 7999 (user config)
```

If you change the bus port, also point the Terminal at it with an `agents` entry or `OT_FDC3_AGENT_URL`.

Implementation: `apps/desktop-agent/src-tauri/src/config.rs`.
