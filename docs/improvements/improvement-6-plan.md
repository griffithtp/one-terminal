# Improvement #6 — Implementation Plan: Complete FDC3 2.2 Surface Coverage

## Architecture notes

- Rust broker state → `manage()` → Tauri commands → React dashboard
- Rust broker state → `app.emit()` → React `listen()` hooks → re-render
- Browser apps → WebSocket → `fdc3_bus/mod.rs` → broker internals
- All new registries: `Arc<DashMap<…>>`, `#[derive(Clone)]`, same pattern as `ChannelManager`
- TCP protocol tags: `snake_case` via `serde(tag = "type", rename_all = "snake_case")`
- WebSocket protocol: `fdc3:*` types, camelCase fields, `requestId` for request/reply pairing

---

## Track A — App Channels

### A1 — New types in `broker/types.rs` (S)

Add after `ChannelInfoSummary`:

```rust
#[derive(Clone, Debug)]
pub struct AppChannelListener {
    pub instance_id: String,
    pub context_type: Option<String>,  // None = wildcard
}

#[derive(Clone, Debug)]
pub struct AppChannelEntry {
    pub channel_id: String,
    pub is_private: bool,
    pub creator_instance_id: String,
    pub listeners: Vec<AppChannelListener>,
    pub last_context: HashMap<String, serde_json::Value>,
    pub message_count: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppChannelInfo {
    pub channel_id: String,
    pub is_private: bool,
    pub creator_instance_id: String,
    pub member_count: usize,
    pub message_count: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaAppChannelEvent {
    pub channel_id: String,
    pub context: serde_json::Value,
    pub source_instance_id: String,
    pub recipient_count: usize,
}
```

New `CdaRequest` variants:

```rust
CreatePrivateChannel,
GetOrCreateChannel { channel_id: String },
AppChannelBroadcast { channel_id: String, context: serde_json::Value },
AddAppChannelListener { channel_id: String, context_type: Option<String> },
RemoveAppChannelListener { channel_id: String, context_type: Option<String> },
```

New `CdaResponse` variants:

```rust
AppChannelReady { channel_id: String, is_private: bool },
AppChannelBroadcast { channel_id: String, context: serde_json::Value, source_instance_id: String },
```

> **Note:** `AppChannelBroadcast` appears in both enums with the same wire tag `"app_channel_broadcast"`. This is correct — TCP clients send it inbound and receive it outbound. `CdaRequest` is only deserialized; `CdaResponse` is only serialized. No runtime collision.

---

### A2 — `AppChannelRegistry` in `broker/app_channel_registry.rs` (M, new file)

```rust
use dashmap::DashMap;
use std::sync::Arc;
use super::types::{AppChannelEntry, AppChannelInfo, AppChannelListener};

#[derive(Clone)]
pub struct AppChannelRegistry {
    inner: Arc<DashMap<String, AppChannelEntry>>,
}

impl AppChannelRegistry {
    pub fn new() -> Self
    pub fn create_private(&self, creator_instance_id: &str) -> String
    pub fn get_or_create(&self, channel_id: &str, creator_instance_id: &str) -> String
    pub fn add_listener(&self, channel_id: &str, instance_id: &str, context_type: Option<String>) -> bool
    pub fn remove_listener(&self, channel_id: &str, instance_id: &str, context_type: Option<String>)
    pub fn remove_all(&self, instance_id: &str)
    // Returns deduplicated target instance_ids; updates last_context and message_count.
    pub fn fan_out(&self, channel_id: &str, context: &serde_json::Value, source_instance_id: &str) -> Vec<String>
    pub fn list_all(&self) -> Vec<AppChannelInfo>
    pub fn exists(&self, channel_id: &str) -> bool
}
```

`fan_out` matches listeners where `context_type.is_none()` OR equals the context's `type` field.
One message per target instance (deduplicate even if multiple listeners registered).

---

### A3 — Export from `broker/mod.rs` (S)

```rust
pub mod app_channel_registry;
pub use app_channel_registry::AppChannelRegistry;
pub use types::{AppChannelInfo, CdaAppChannelEvent, /* existing exports */};
```

---

### A4 — TCP dispatch in `tcp/handler.rs` (M)

Add `app_channel_registry: AppChannelRegistry` to `handle_connection` and `dispatch` signatures.

In Phase 3 cleanup: `app_channel_registry.remove_all(&instance_id);`

New `dispatch` arms:

```rust
CdaRequest::CreatePrivateChannel => {
    let channel_id = app_channel_registry.create_private(instance_id);
    respond(window_manager, instance_id, CdaResponse::AppChannelReady { channel_id, is_private: true });
}
CdaRequest::GetOrCreateChannel { channel_id } => {
    let cid = app_channel_registry.get_or_create(&channel_id, instance_id);
    respond(window_manager, instance_id, CdaResponse::AppChannelReady { channel_id: cid, is_private: false });
}
CdaRequest::AppChannelBroadcast { channel_id, context } => {
    fan_out_app_channel_broadcast(instance_id, &channel_id, context, &app_channel_registry, window_manager, app);
}
CdaRequest::AddAppChannelListener { channel_id, context_type } => {
    app_channel_registry.add_listener(&channel_id, instance_id, context_type);
}
CdaRequest::RemoveAppChannelListener { channel_id, context_type } => {
    app_channel_registry.remove_listener(&channel_id, instance_id, context_type);
}
```

Add `pub(crate) fn fan_out_app_channel_broadcast(...)` helper (mirrors `fan_out_broadcast`).

---

### A5 — FDC3 Bus in `fdc3_bus/mod.rs` (M)

Add `app_channel_registry: AppChannelRegistry` to `BusState` and `start()` signature.

In Phase 3 cleanup: `state.app_channel_registry.remove_all(&instance_id);`

New WebSocket message arms:

```
"fdc3:createPrivateChannel"    → create_private; reply fdc3:appChannelReady
"fdc3:getOrCreateChannel"      → get_or_create; reply fdc3:appChannelReady
"fdc3:appChannelBroadcast"     → fan_out_app_channel_broadcast
"fdc3:addAppChannelListener"   → add_listener
"fdc3:removeAppChannelListener"→ remove_listener
```

In `cda_to_fdc3`, add:

```rust
"app_channel_broadcast" => json!({
    "type":             "fdc3:appChannelBroadcast",
    "channelId":        v["channel_id"],
    "context":          v["context"],
    "sourceInstanceId": v["source_instance_id"],
}),
```

---

### A6 — Wire into `lib.rs` (M)

```rust
let app_channel_registry = AppChannelRegistry::new();
// .manage(app_channel_registry.clone())
// pass clone into tcp::server::start() and fdc3_bus::start()
```

New command:

```rust
#[tauri::command]
fn cda_list_app_channels(app_channel_registry: State<AppChannelRegistry>) -> Vec<AppChannelInfo> {
    app_channel_registry.list_all()
}
```

Update `cda_disconnect_spoke` to call `app_channel_registry.remove_all(&instance_id)`.
Register `cda_list_app_channels` in `invoke_handler!`.
Update `tcp/server.rs` and `fdc3_bus/mod.rs` `start()` signatures accordingly.

---

### A7 — `fdc3-plugin.js`: implement app channel methods (M)

Add private field `#appChannels = new Map()` (channel_id → `{ listeners: Map }`).

In `#onMessage`, handle `fdc3:appChannelBroadcast` push — fan out to registered local handlers by context type.

Add public methods:

```js
async createPrivateChannel() { /* send fdc3:createPrivateChannel, await fdc3:appChannelReady */ }
async getOrCreateChannel(channelId) { /* send fdc3:getOrCreateChannel, await fdc3:appChannelReady */ }
```

Add private helper `#wrapAppChannel(channelId, isPrivate)` returning a Channel-like object with:

- `broadcast(context)` → posts `fdc3:appChannelBroadcast`
- `addContextListener(type, handler)` → registers locally + posts `fdc3:addAppChannelListener`; returns `{ unsubscribe }` that posts `fdc3:removeAppChannelListener`
- `getCurrentContext(_type)` → `Promise.resolve(null)`

---

### A8 — Dashboard: `AppChannelsPanel.tsx` (S, new file)

```tsx
interface Props {
  appChannels: AppChannelInfo[];
}
export function AppChannelsPanel({ appChannels }: Props);
```

Renders rows: truncated channel ID (8 chars), `private`/`named` badge, member count, message count.
CSS: reuse existing `.panel`, `.panel__title`, `.panel__badge` classes.

Wire-up:

- `useCdaEvents.ts`: add `appChannels` state, `invoke<AppChannelInfo[]>("cda_list_app_channels")` inside `refresh()`
- `App.tsx` dashboard tab: render `<AppChannelsPanel>` after `<ChannelsPanel>`
- `types.ts`: add `AppChannelInfo` interface

---

## Track B — Dashboard Channel and Listener Controls

### B1 — `ContextListenerRegistry` in `broker/context_listener_registry.rs` (S, new file)

```rust
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerEntry {
    pub instance_id: String,
    pub context_types: Vec<Option<String>>,
}

#[derive(Clone)]
pub struct ContextListenerRegistry {
    inner: Arc<DashMap<String, Vec<Option<String>>>>,
}

impl ContextListenerRegistry {
    pub fn new() -> Self
    pub fn add(&self, instance_id: &str, context_type: Option<String>)
    pub fn remove(&self, instance_id: &str, context_type: Option<String>)
    pub fn remove_all(&self, instance_id: &str)
    pub fn list_all(&self) -> Vec<ListenerEntry>
}
```

---

### B2 — New `CdaRequest` variants + `ListenerInfo` type in `broker/types.rs` (S)

```rust
// CdaRequest additions:
AddContextListener { context_type: Option<String> },
RemoveContextListener { context_type: Option<String> },

// New Tauri command return type:
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerInfo {
    pub instance_id: String,
    pub app_id: String,
    pub display_name: Option<String>,
    pub context_types: Vec<Option<String>>,
    pub intents: Vec<String>,
}
```

---

### B3 — Export from `broker/mod.rs` (S)

```rust
pub mod context_listener_registry;
pub use context_listener_registry::ContextListenerRegistry;
pub use types::{ListenerEntry, ListenerInfo, /* existing */};
```

---

### B4 — TCP dispatch in `tcp/handler.rs` (S)

Add `context_listener_registry: ContextListenerRegistry` to signatures.
Cleanup: `context_listener_registry.remove_all(&instance_id);`

```rust
CdaRequest::AddContextListener { context_type } => {
    context_listener_registry.add(instance_id, context_type);
}
CdaRequest::RemoveContextListener { context_type } => {
    context_listener_registry.remove(instance_id, context_type);
}
```

---

### B5 — FDC3 Bus in `fdc3_bus/mod.rs` (S)

Add `context_listener_registry: ContextListenerRegistry` to `BusState` and `start()`.
Cleanup: `state.context_listener_registry.remove_all(&instance_id);`

```
"fdc3:addContextListener"    → context_listener_registry.add
"fdc3:removeContextListener" → context_listener_registry.remove
```

---

### B6 — Wire into `lib.rs` + new Tauri commands (M)

`.manage(ContextListenerRegistry::new())`, pass clone into both server spawns.
Update `cda_disconnect_spoke` to also call `context_listener_registry.remove_all`.

New commands:

```rust
#[tauri::command]
fn cda_list_listeners(
    context_listener_registry: State<ContextListenerRegistry>,
    intent_registry: State<IntentRegistry>,
    window_manager: State<WindowManager>,
) -> Vec<ListenerInfo>
// Merge both registries, join with WindowManager for app_id/display_name.

#[tauri::command]
fn cda_dashboard_join_channel(channel_id: String, ...) -> Result<(), String>
// register_with_id("dashboard", …) if not already registered (with no-op sink task)
// then channel_manager.join("dashboard", &channel_id)

#[tauri::command]
fn cda_dashboard_leave_channel(...) -> ()
// channel_manager.leave("dashboard")
// window_manager.unregister("dashboard")
// cancel the no-op sink task
```

Add to `WindowManager` (`broker/window_manager.rs`):

```rust
pub fn register_with_id(
    &self,
    instance_id: String,
    app_id: String,
    display_name: Option<String>,
    tx: UnboundedSender<String>,
)
```

> **No-op sink:** Spawn `tauri::async_runtime::spawn(async move { while rx.recv().await.is_some() {} })` in `cda_dashboard_join_channel`. Store the `JoinHandle` in `Mutex<Option<JoinHandle<()>>>` managed state. Cancel it in `cda_dashboard_leave_channel`.

Register all three commands in `invoke_handler!`.

---

### B7 — `fdc3-plugin.js`: report context listeners to broker (S)

Modify `addContextListener`: after registering the handler locally, also `#post`:

```js
this.#post({
  type: "fdc3:addContextListener",
  instanceId: this.#instanceId,
  contextType: contextType ?? null,
});
```

In the returned `{ unsubscribe }`:

```js
this.#post({
  type: "fdc3:removeContextListener",
  instanceId: this.#instanceId,
  contextType: contextType ?? null,
});
```

---

### B8 — Dashboard: "Listeners" sub-tab in `InteropDashboard.tsx` (M)

Extend `Tab` union: `"broadcast" | "intent" | "listeners"`.
Add "Listeners" button to the tab strip.
Add `listeners` state: `invoke<ListenerInfo[]>("cda_list_listeners")` when tab is active.
Render table: App | Instance (8-char) | Context Types | Intents.
Add `ListenerInfo` to `types.ts`.

---

### B9 — Dashboard: join/leave buttons in `ChannelsPanel.tsx` (M)

```ts
interface Props {
  channels: ChannelInfo[];
  connections: WindowHandle[];
  dashboardChannel: string | null;
  onJoin: (channelId: string) => void;
  onLeave: () => void;
}
```

Each channel card: "Join" button (or "Leave" if `dashboardChannel === ch.channelId`).
Apply `.channel-card--dashboard` highlight class when the dashboard is a member.

In `useCdaEvents.ts`, add:

```ts
const [dashboardChannel, setDashboardChannel] = useState<string | null>(null);
const joinDashboardChannel = (channelId: string) =>
  invoke("cda_dashboard_join_channel", { channelId }).then(() => {
    setDashboardChannel(channelId);
    refresh();
  });
const leaveDashboardChannel = () =>
  invoke("cda_dashboard_leave_channel").then(() => {
    setDashboardChannel(null);
    refresh();
  });
```

Return both from `useCdaEvents`; pass to `<ChannelsPanel>` in `App.tsx`.

---

## Track C — Sample App Improvements

### C1 — `sample-chart/index.html`: channel picker (S)

Replace `joinUserChannel('Green')` with:

```js
const channels = await fdc3.getUserChannels();
// populate <select id="channel-select">; default to 'Green' if available
await fdc3.joinUserChannel(channelSel.value);
channelSel.addEventListener("change", async () => {
  await fdc3.leaveCurrentChannel();
  await fdc3.joinUserChannel(channelSel.value);
});
```

---

### C2 — `sample-chart/index.html`: dynamic intent listener toggles (S)

Replace static `.listener-badge` spans with `<label><input type="checkbox" checked> Intent: ViewChart</label>` elements.

Extract `registerListener(intent)` / `unregisterListener(intent)` helpers storing handles in `listenerHandles = {}`. Wire each checkbox `change` event to call the appropriate helper.

---

### C3 — `sample-chart/index.html`: status row (S)

Add:

```html
<div class="status-row">
  Channel: <span id="active-channel">—</span> · Active listeners:
  <span id="active-listeners">—</span>
</div>
```

Update whenever channel or checkboxes change.

---

### C4 — `sample-ticker/index.html`: receive broadcasts from channel (S)

After channel join, subscribe:

```js
contextListener?.unsubscribe();
contextListener = await fdc3.addContextListener("fdc3.instrument", (context, meta) => {
  addReceivedLog(context.id?.ticker ?? "?", meta?.source?.appId ?? "unknown", context);
});
```

Add a separate `#received-log` section below the activity log. Style with amber colour to distinguish from self-sent rows.

---

### C5 — `sample-ticker/index.html`: tick interval slider (S)

Replace `setInterval(tick, 500)` with a variable driven by:

```html
<input type="range" id="tick-rate" min="100" max="2000" step="100" value="500" />
<span id="tick-rate-display">500 ms</span>
```

```js
tickRateEl.addEventListener("input", (e) => {
  tickInterval = Number(e.target.value);
  tickRateDisplay.textContent = `${tickInterval} ms`;
  clearInterval(tickTimer);
  tickTimer = setInterval(tick, tickInterval);
});
```

---

## Execution order

| #   | Step | Files                                                               | Size |
| --- | ---- | ------------------------------------------------------------------- | ---- |
| 1   | A1   | `broker/types.rs`                                                   | S    |
| 2   | B1   | `broker/context_listener_registry.rs` (new)                         | S    |
| 3   | B2   | `broker/types.rs`                                                   | S    |
| 4   | A2   | `broker/app_channel_registry.rs` (new)                              | M    |
| 5   | A3   | `broker/mod.rs`                                                     | S    |
| 6   | B3   | `broker/mod.rs`                                                     | S    |
| 7   | B4   | `tcp/handler.rs`                                                    | S    |
| 8   | A4   | `tcp/handler.rs`                                                    | M    |
| 9   | A5   | `fdc3_bus/mod.rs`                                                   | M    |
| 10  | B5   | `fdc3_bus/mod.rs`                                                   | S    |
| 11  | A6   | `lib.rs` + `window_manager.rs` + `tcp/server.rs`                    | M    |
| 12  | B6   | `lib.rs` + `window_manager.rs`                                      | M    |
| 13  | A7   | `fdc3-plugin.js`                                                    | M    |
| 14  | B7   | `fdc3-plugin.js`                                                    | S    |
| 15  | A8   | `AppChannelsPanel.tsx` + `useCdaEvents.ts` + `App.tsx` + `types.ts` | S    |
| 16  | B8   | `InteropDashboard.tsx` + `types.ts`                                 | M    |
| 17  | B9   | `ChannelsPanel.tsx` + `useCdaEvents.ts` + `App.tsx`                 | M    |
| 18  | C1   | `sample-chart/index.html`                                           | S    |
| 19  | C2   | `sample-chart/index.html`                                           | S    |
| 20  | C3   | `sample-chart/index.html`                                           | S    |
| 21  | C4   | `sample-ticker/index.html`                                          | S    |
| 22  | C5   | `sample-ticker/index.html`                                          | S    |

**Rust compile checkpoints:**

- After step 6: `cargo check -p desktop-agent` — new structs/enums should compile
- After step 12: `cargo build -p desktop-agent` — full build before touching JS

**Track C is fully independent** — steps 18–22 can be done in any order and at any point.

---

## Pitfalls to watch

| Risk                                                                                | Mitigation                                                                                         |
| ----------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `AppChannelBroadcast` variant name collision between `CdaRequest` and `CdaResponse` | Document in code; not a runtime issue since each enum is only ever used in one direction           |
| Dashboard "no-op sink" receiver dropped immediately                                 | Spawn a drain task; store its `JoinHandle` in managed state and cancel on `leave_channel`          |
| `register_with_id` for the dashboard pseudo-spoke                                   | Ensure `unregister("dashboard")` is called on leave so it doesn't appear in `cda_list_connections` |
| Plugin `addContextListener` called before `#instanceId` is set                      | Existing `readyState` guard already covers this; no extra handling needed                          |
