# Medium Impact Improvements

## 4. Port Availability Check at Startup

**Status:** Done — `PortsConfig::check_availability()` added to `config.rs`; called in `lib.rs::run()` before the Tauri builder  
**Scope:** `apps/desktop-agent/src-tauri/src/config.rs`

`AgentConfig::load()` reads ports from env vars (`OT_TCP_PORT`, `OT_FDC3_BUS_PORT`, `OT_DACP_PORT`) but nothing checks whether those ports are already bound. A stale process or port conflict causes silent, confusing failures — the Desktop Agent either panics deep in the TCP layer or silently drops connections.

**Actions:**

- After config is loaded, probe each port with a short-lived `TcpListener::bind` attempt
- On failure, emit a clear startup error: `"Port 7890 is already in use. Set OT_TCP_PORT to override."`
- Exit early with a non-zero code rather than letting the app reach an inconsistent state
- Document the port env vars and their defaults in `apps/desktop-agent/README.md`

---

## 5. Improve Electron Setup DX

**Status:** Done — `dev:desktop-agent:electron` and `dev:terminal:electron` auto-run `setup:electron-host`; `dev:all:electron` composite added; `validate_electron_override()` warns at startup if the shell path is broken or the Electron binary is missing; sample apps (`ticker-plant`, `chart-viewer`) now list `electron` as an engine binding in `apps/app-directory/src/data.ts`  
**Scope:** `apps/electron-host/`, root `package.json`

Running in Electron mode requires manually running `npm run setup:electron-host`, then setting `OT_ELECTRON_HOST_OVERRIDE` to an absolute path, and neither step appears in the critical-path getting-started docs. New contributors reliably hit this gap.

**Actions:**

- Add a pre-flight check in `apps/desktop-agent/src-tauri/src/` that detects when `OT_ENGINE_FAMILY=electron` is set but `OT_ELECTRON_HOST_OVERRIDE` points to a non-existent path — print a one-line fix hint
- Alternatively, update `dev:desktop-agent:electron` in root `package.json` to run `setup:electron-host` as a prerequisite if the build output is missing
- Add an "Electron mode" section to `apps/desktop-agent/README.md` with the exact setup sequence
- Consider exposing an `npm run dev:full:electron` composite script that starts app-directory → desktop-agent (electron) → terminal (electron) in one command

---

## 6. Complete FDC3 2.2 Surface Coverage

**Status:** Not started  
**Scope:** `apps/sample-ticker/`, `apps/sample-chart/`, `apps/desktop-agent/src/`, `apps/desktop-agent/src-tauri/src/broker/`, `packages/fdc3-plugin/`

### What is already covered (do not duplicate)

The Desktop Agent Dashboard already provides solid coverage for several areas originally listed in this improvement:

| Feature                                            | Where it lives                                                           |
| -------------------------------------------------- | ------------------------------------------------------------------------ |
| `raiseIntent` with multi-handler selection         | `InteropDashboard.tsx` — handler picker grouped by local vs. peer bridge |
| Intent resolution modal                            | `IntentResolverModal.tsx` — auto-launches candidate apps                 |
| Event log showing broadcasts and intent deliveries | `ActivityFeed.tsx` — timestamped, 200-entry log                          |
| App Directory browsing and `open()`                | `LauncherPanel.tsx` + `useFdcBus.onOpen()`                               |
| Bridge topology visualisation                      | `BridgeVisualizer.tsx` — force-directed graph                            |

Adding a new sample app to reproduce these would just duplicate what the Dashboard already does better and from the broker's own vantage point. **Do not build a third sample app.**

### What is genuinely missing

#### Track A — App channels (largest gap, affects FDC3 2.2 compliance)

`createPrivateChannel()` and `getOrCreateChannel()` are exposed in `fdc3-client` and `Fdc3Agent` but stub out to Tauri commands that do not exist in the broker. The `fdc3-plugin.js` browser client has no implementation for either call. Any app that calls these APIs will receive a runtime error.

- **Broker** (`apps/desktop-agent/src-tauri/src/broker/`): add an `AppChannelRegistry` alongside `ChannelManager`; handle `CreatePrivateChannel` and `GetOrCreateChannel` in the TCP and FDC3-Bus message routers; add `cda_list_app_channels` Tauri command for the dashboard
- **Plugin** (`packages/fdc3-plugin/`): implement `createPrivateChannel()` and `getOrCreateChannel()` so browser apps can use app channels over the WebSocket bus
- **Dashboard** (`apps/desktop-agent/src/`): add a read-only app-channels panel (channel ID, members, message count) similar to the existing `ChannelsPanel`

#### Track B — Dashboard channel and listener controls

The `ChannelsPanel` and `InteropDashboard` are read-only with respect to channel membership and listener registration. The Dashboard sends broadcasts via the internal `cda_broadcast_from_dashboard` shortcut, bypassing the real FDC3 spoke path.

- Add join/leave controls to `ChannelsPanel` so the Dashboard itself can act as an FDC3 spoke for testing
- Add a "Listeners" sub-tab to `InteropDashboard` showing active context and intent listeners across all connected spokes (sourced from a new `cda_list_listeners` Tauri command); allow registering and deregistering listeners from the dashboard for quick smoke-testing

#### Track C — Make the existing sample apps genuinely configurable

Both samples hard-code their channel and listener setup, making them poor general-purpose test harnesses. The samples are important because they exercise the real `fdc3-plugin.js` WebSocket path — the Dashboard uses direct Tauri IPC shortcuts and cannot substitute for this.

**`apps/sample-chart/`** (currently hardcoded to Green channel, fixed ViewChart/ViewQuote listeners):

- Replace the hardcoded `joinUserChannel('Green')` with a channel picker populated from `getUserChannels()`, matching what sample-ticker already does
- Replace fixed `addIntentListener` calls with a dynamic listener panel: checkboxes for each known intent so the tester can register and deregister handlers at runtime
- Show which channel is currently joined and what context types are being listened to

**`apps/sample-ticker/`** (already has channel picker and activity log, good baseline):

- Add a "received context" section that shows incoming `fdc3.instrument` broadcasts from other apps on the same channel — currently the ticker only sends, never displays received context
- Add a configurable broadcast interval slider (currently fixed at 500 ms) so testers can control data rate

### Completion criteria

- `createPrivateChannel()` and `getOrCreateChannel()` return valid channel objects and route messages correctly between subscribers
- Dashboard can join/leave user channels and register/deregister listeners without a separate spoke app running
- sample-chart channel and intent listeners are runtime-configurable, not hardcoded
- sample-ticker displays incoming context from other spokes on the same channel
- All four changes are exercisable in a single `npm run dev:all:electron` session without writing any additional code
