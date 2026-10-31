//! FDC3 3.0 Desktop Agent core.
//!
//! The broker owns all FDC3 state and speaks the Desktop Agent Communication
//! Protocol (DACP) as JSON values. It is synchronous and knows nothing about
//! sockets or Tauri: [`Broker::handle`] applies one app message, queues replies
//! on each instance's outbound sender, and returns the side effects the caller
//! must perform ([`Effect`]). `fdc3_bus` is the only transport.
//!
//! Ordering matters to the official `DesktopAgentProxy`: it registers a listener
//! only after the listener's response arrives, so a response is always queued
//! before any event that targets the new listener.

mod apps;
mod channels;
mod intents;
mod types;

pub use types::*;

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use crate::app_dir::AppRecord;

pub const FDC3_VERSION: &str = "3.0";
pub const PROVIDER: &str = "one-terminal-desktop-agent";

/// How long a launched app has to connect and register before `open` or
/// `raiseIntent` gives up. The plugin's `appLaunchTimeout` must be longer.
const APP_LAUNCH_TIMEOUT: Duration = Duration::from_secs(20);

/// Identity used for actions taken from the Desktop Agent's own dashboard.
const DASHBOARD_APP_ID: &str = "desktop-agent";
const DASHBOARD_INSTANCE_ID: &str = "__dashboard__";

/// Side effects [`Broker::handle`] can't perform itself.
#[derive(Debug)]
pub enum Effect {
    /// Launch `app`, then report the outcome with [`Broker::launch_finished`].
    Launch {
        pending_id: String,
        app: Box<AppRecord>,
    },
    /// Call [`Broker::expire`] for `pending_id` once `after` has elapsed.
    ExpireAfter { pending_id: String, after: Duration },
    /// Close the window `label`; on failure call [`Broker::close_failed`].
    CloseWindow {
        instance_id: String,
        request_uuid: String,
        label: String,
    },
}

// ── Internal state ─────────────────────────────────────────────────────────────

struct Instance {
    app_id: String,
    connected_at: u64,
    current_channel: Option<String>,
    /// Window this agent opened for the instance; `None` if it's hosted elsewhere.
    window_label: Option<String>,
    tx: UnboundedSender<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum ChannelKind {
    User,
    App,
    Private,
}

impl ChannelKind {
    fn as_str(self) -> &'static str {
        match self {
            ChannelKind::User => "user",
            ChannelKind::App => "app",
            ChannelKind::Private => "private",
        }
    }
}

struct Channel {
    id: String,
    kind: ChannelKind,
    display_name: String,
    color: String,
    /// `(context, metadata)` pairs, most recently broadcast first; one per type.
    history: Vec<(Value, Value)>,
    /// Private channels only: instances allowed to use the channel.
    access: HashSet<String>,
}

struct ContextListener {
    uuid: String,
    instance_id: String,
    /// `None` follows the instance's current user channel.
    channel_id: Option<String>,
    /// `None` matches every context type.
    context_types: Option<Vec<String>>,
    /// Whether the app asked with the array form, which events echo back.
    is_array: bool,
}

struct EventListener {
    uuid: String,
    instance_id: String,
    channel_id: Option<String>,
    event_type: Option<String>,
}

struct PrivateEventListener {
    uuid: String,
    instance_id: String,
    channel_id: String,
    event_type: Option<String>,
}

struct IntentListener {
    uuid: String,
    instance_id: String,
    intent: String,
    context_types: Option<Vec<String>>,
}

/// A request waiting for a launched app to register a matching intent listener.
struct PendingIntent {
    id: String,
    raise: intents::Raise,
    app_id: String,
    instance_id: Option<String>,
}

#[derive(PartialEq)]
enum OpenState {
    Opening,
    DeliveringContext,
}

struct PendingOpen {
    id: String,
    request_uuid: String,
    from: String,
    app_id: String,
    context: Option<Value>,
    metadata: Value,
    state: OpenState,
    instance_id: Option<String>,
}

/// A delivered intent whose result hasn't come back from the handler yet.
struct PendingResult {
    raiser: String,
    handler: String,
}

struct State {
    instances: HashMap<String, Instance>,
    channels: Vec<Channel>,
    context_listeners: Vec<ContextListener>,
    event_listeners: Vec<EventListener>,
    private_event_listeners: Vec<PrivateEventListener>,
    intent_listeners: Vec<IntentListener>,
    pending_intents: Vec<PendingIntent>,
    pending_opens: Vec<PendingOpen>,
    pending_results: HashMap<String, PendingResult>,
    /// Windows opened for an app that hasn't connected yet: `(app_id, label)`.
    launched_windows: Vec<(String, String)>,
}

impl State {
    fn send(&self, instance_id: &str, msg: &Value) -> bool {
        let Some(inst) = self.instances.get(instance_id) else {
            return false;
        };
        inst.tx.send(msg.to_string()).is_ok()
    }

    fn respond(&self, to: &str, request_uuid: &str, resp_type: &str, payload: Value) {
        self.send(
            to,
            &json!({
                "type": resp_type,
                "payload": payload,
                "meta": {
                    "requestUuid": request_uuid,
                    "responseUuid": new_uuid(),
                    "timestamp": iso_timestamp(),
                },
            }),
        );
    }

    fn respond_error(&self, to: &str, request_uuid: &str, resp_type: &str, error: &str) {
        self.respond(to, request_uuid, resp_type, json!({ "error": error }));
    }

    fn app_identifier(&self, instance_id: &str) -> Value {
        match self.instances.get(instance_id) {
            Some(i) => json!({ "appId": i.app_id, "instanceId": instance_id }),
            None => json!({ "appId": DASHBOARD_APP_ID, "instanceId": instance_id }),
        }
    }

    fn app_id_of(&self, instance_id: &str) -> String {
        self.instances
            .get(instance_id)
            .map(|i| i.app_id.clone())
            .unwrap_or_else(|| DASHBOARD_APP_ID.to_string())
    }

    fn channel(&self, id: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    fn channel_mut(&mut self, id: &str) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.id == id)
    }
}

fn event(event_type: &str, payload: Value) -> Value {
    json!({
        "type": event_type,
        "payload": payload,
        "meta": { "eventUuid": new_uuid(), "timestamp": iso_timestamp() },
    })
}

/// Merge app-provided metadata with the fields the Desktop Agent owns.
/// App-supplied `traceId`, `signature`, `antiReplay` and `custom` pass through
/// unchanged; `source` and `timestamp` can't be set by the app.
fn enrich_metadata(source: Value, app_metadata: &Value) -> Value {
    let mut out = json!({
        "source": source,
        "timestamp": iso_timestamp(),
        "traceId": app_metadata
            .get("traceId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(new_uuid),
    });
    for key in ["signature", "antiReplay", "custom"] {
        if let Some(v) = app_metadata.get(key).filter(|v| !v.is_null()) {
            out[key] = v.clone();
        }
    }
    out
}

/// The `type` of a well-formed context, or `None` if it's malformed.
fn context_type(context: &Value) -> Option<&str> {
    context.get("type").and_then(Value::as_str)
}

fn string_list(value: Option<&Value>) -> Option<Vec<String>> {
    Some(
        value?
            .as_array()?
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
    )
}

// ── Broker ─────────────────────────────────────────────────────────────────────

/// Thread-safe, clone-safe handle to the Desktop Agent's FDC3 state.
#[derive(Clone)]
pub struct Broker {
    state: Arc<Mutex<State>>,
    events: Arc<dyn BrokerEvents>,
}

impl Broker {
    pub fn new(events: Arc<dyn BrokerEvents>) -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                instances: HashMap::new(),
                channels: Vec::new(),
                context_listeners: Vec::new(),
                event_listeners: Vec::new(),
                private_event_listeners: Vec::new(),
                intent_listeners: Vec::new(),
                pending_intents: Vec::new(),
                pending_opens: Vec::new(),
                pending_results: HashMap::new(),
                launched_windows: Vec::new(),
            })),
            events,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Seed a user channel from config. No-op if the id already exists.
    pub fn add_user_channel(&self, id: &str, display_name: &str, color: &str) {
        let mut st = self.lock();
        if st.channel(id).is_none() {
            st.channels.push(Channel {
                id: id.to_string(),
                kind: ChannelKind::User,
                display_name: display_name.to_string(),
                color: color.to_string(),
                history: Vec::new(),
                access: HashSet::new(),
            });
        }
    }

    // ── Connection lifecycle ───────────────────────────────────────────────────

    /// Register a new connection and answer its `WCP4ValidateAppIdentity`.
    /// Returns the instance id assigned to the connection.
    ///
    /// `app_id` is self-asserted by the page: the agent does not authenticate it.
    pub fn connect(
        &self,
        app_id: &str,
        connection_attempt_uuid: &str,
        directory: &[AppRecord],
        tx: UnboundedSender<String>,
    ) -> String {
        let instance_id = new_uuid();
        let instance_uuid = new_uuid();
        let connected_at = now_ms();
        let mut st = self.lock();

        let window_label = st
            .launched_windows
            .iter()
            .position(|(a, _)| a == app_id)
            .map(|i| st.launched_windows.remove(i).1);

        st.instances.insert(
            instance_id.clone(),
            Instance {
                app_id: app_id.to_string(),
                connected_at,
                current_channel: None,
                window_label,
                tx,
            },
        );

        let metadata = apps::implementation_metadata(app_id, &instance_id, directory);
        st.send(
            &instance_id,
            &json!({
                "type": "WCP5ValidateAppIdentityResponse",
                "meta": {
                    "connectionAttemptUuid": connection_attempt_uuid,
                    "timestamp": iso_timestamp(),
                },
                "payload": {
                    "appId": app_id,
                    "instanceId": instance_id,
                    "instanceUuid": instance_uuid,
                    "implementationMetadata": metadata,
                },
            }),
        );

        apps::on_app_connected(&mut st, app_id, &instance_id);

        self.events.emit(BrokerEvent::Connected(CdaConnectedEvent {
            instance_id: instance_id.clone(),
            app_id: app_id.to_string(),
            display_name: None,
            connected_at,
        }));
        instance_id
    }

    /// Remove an instance and everything it registered. Safe to call twice: a
    /// forced disconnect drops the sender, and the socket task then calls this
    /// again as it exits.
    pub fn disconnect(&self, instance_id: &str) -> bool {
        let mut st = self.lock();
        let Some(instance) = st.instances.remove(instance_id) else {
            return false;
        };
        channels::on_disconnect(&mut st, instance_id);
        intents::on_disconnect(&mut st, instance_id);
        self.events
            .emit(BrokerEvent::Disconnected(CdaDisconnectedEvent {
                instance_id: instance_id.to_string(),
                app_id: instance.app_id,
            }));
        true
    }

    // ── Message dispatch ───────────────────────────────────────────────────────

    /// Apply one DACP message from `instance_id`. `directory` is a current
    /// snapshot of the App Directory.
    pub fn handle(&self, instance_id: &str, msg: &Value, directory: &[AppRecord]) -> Vec<Effect> {
        let Some(msg_type) = msg.get("type").and_then(Value::as_str) else {
            return Vec::new();
        };
        let mut st = self.lock();
        if !st.instances.contains_key(instance_id) {
            return Vec::new();
        }
        let request_uuid = msg["meta"]["requestUuid"].as_str().unwrap_or_default();
        let req = Request {
            from: instance_id,
            uuid: request_uuid,
            payload: &msg["payload"],
        };
        let events = self.events.as_ref();

        match msg_type {
            "getUserChannelsRequest" => channels::get_user_channels(&st, &req),
            "getOrCreateChannelRequest" => channels::get_or_create(&mut st, &req),
            "getCurrentChannelRequest" => channels::get_current_channel(&st, &req),
            "joinUserChannelRequest" => channels::join(&mut st, &req, events),
            "leaveCurrentChannelRequest" => channels::leave(&mut st, &req, events),
            "createPrivateChannelRequest" => channels::create_private(&mut st, &req),
            "privateChannelDisconnectRequest" => channels::private_disconnect(&mut st, &req),
            "privateChannelAddEventListenerRequest" => {
                channels::add_private_event_listener(&mut st, &req)
            }
            "privateChannelUnsubscribeEventListenerRequest" => {
                channels::remove_private_event_listener(&mut st, &req)
            }
            "broadcastRequest" => channels::broadcast(&mut st, &req, events),
            "getCurrentContextRequest" => channels::get_current_context(&st, &req),
            "clearContextRequest" => channels::clear_context(&mut st, &req),
            "addContextListenerRequest" => {
                channels::add_context_listener(&mut st, &req);
                apps::deliver_open_context(&mut st, instance_id, req.payload);
            }
            "contextListenerUnsubscribeRequest" => channels::remove_context_listener(&mut st, &req),
            "addEventListenerRequest" => channels::add_event_listener(&mut st, &req),
            "eventListenerUnsubscribeRequest" => channels::remove_event_listener(&mut st, &req),

            "findIntentRequest" => intents::find_intent(&st, &req, directory),
            "findIntentsByContextRequest" => intents::find_intents_by_context(&st, &req, directory),
            "addIntentListenerRequest" => intents::add_listener(&mut st, &req, events),
            "intentListenerUnsubscribeRequest" => intents::remove_listener(&mut st, &req),
            "raiseIntentRequest" => return intents::raise_intent(&mut st, &req, directory, events),
            "raiseIntentForContextRequest" => {
                return intents::raise_intent_for_context(&mut st, &req, directory, events)
            }
            "intentResultRequest" => intents::intent_result(&mut st, &req),

            "openRequest" => return apps::open(&mut st, &req, directory),
            "closeRequest" => return apps::close(&st, &req),
            "findInstancesRequest" => apps::find_instances(&st, &req),
            "getAppMetadataRequest" => apps::get_app_metadata(&st, &req, directory),
            "getInfoRequest" => apps::get_info(&st, &req, directory),

            // Liveness is tracked by the socket itself. Webviews on hidden
            // dashboards can be suspended, so a missed heartbeat proves nothing.
            "heartbeatAcknowledgementRequest" => {}

            other => {
                if let Some(resp_type) = other.strip_suffix("Request") {
                    st.respond_error(
                        instance_id,
                        request_uuid,
                        &format!("{resp_type}Response"),
                        "InvalidArguments",
                    );
                }
                eprintln!("[broker] unhandled message type: {other}");
            }
        }
        Vec::new()
    }

    /// Report the outcome of an [`Effect::Launch`]. `window_label` names the
    /// window if the agent can later close it.
    pub fn launch_finished(&self, pending_id: &str, result: Result<Option<String>, String>) {
        let mut st = self.lock();
        match result {
            Ok(label) => {
                let app_id = st
                    .pending_opens
                    .iter()
                    .find(|p| p.id == pending_id)
                    .map(|p| p.app_id.clone())
                    .or_else(|| {
                        st.pending_intents
                            .iter()
                            .find(|p| p.id == pending_id)
                            .map(|p| p.app_id.clone())
                    });
                if let (Some(label), Some(app_id)) = (label, app_id) {
                    st.launched_windows.push((app_id, label));
                }
            }
            Err(e) => {
                eprintln!("[broker] launch failed: {e}");
                apps::fail_open(&mut st, pending_id, "ErrorOnLaunch");
                intents::fail_pending(&mut st, pending_id);
            }
        }
    }

    /// A launch timeout elapsed: fail `pending_id` if it's still outstanding.
    pub fn expire(&self, pending_id: &str) {
        let mut st = self.lock();
        apps::expire_open(&mut st, pending_id);
        intents::fail_pending(&mut st, pending_id);
    }

    /// An [`Effect::CloseWindow`] failed, so the app is still running and can
    /// be told.
    pub fn close_failed(&self, instance_id: &str, request_uuid: &str) {
        self.lock()
            .respond_error(instance_id, request_uuid, "closeResponse", "ErrorOnClose");
    }

    /// Serialized `heartbeatEvent`, sent periodically by the transport.
    pub fn heartbeat_event() -> String {
        event("heartbeatEvent", json!({})).to_string()
    }

    // ── Dashboard API ──────────────────────────────────────────────────────────

    pub fn list_connections(&self) -> Vec<WindowHandle> {
        let st = self.lock();
        let mut out: Vec<WindowHandle> = st
            .instances
            .iter()
            .map(|(id, i)| WindowHandle {
                instance_id: id.clone(),
                app_id: i.app_id.clone(),
                display_name: None,
                connected_at: i.connected_at,
                current_channel: i.current_channel.clone(),
                tx: i.tx.clone(),
            })
            .collect();
        out.sort_by_key(|h| h.connected_at);
        out
    }

    pub fn list_channels(&self) -> Vec<ChannelInfo> {
        let st = self.lock();
        st.channels
            .iter()
            .filter(|c| c.kind == ChannelKind::User)
            .map(|c| {
                let mut members: Vec<String> = st
                    .instances
                    .iter()
                    .filter(|(_, i)| i.current_channel.as_deref() == Some(c.id.as_str()))
                    .map(|(id, _)| id.clone())
                    .collect();
                members.sort();
                ChannelInfo {
                    channel_id: c.id.clone(),
                    display_name: c.display_name.clone(),
                    color: c.color.clone(),
                    members,
                }
            })
            .collect()
    }

    pub fn list_intent_handlers(&self) -> Vec<IntentHandlerInfo> {
        let st = self.lock();
        st.intent_listeners
            .iter()
            .map(|l| IntentHandlerInfo {
                intent: l.intent.clone(),
                app_id: st.app_id_of(&l.instance_id),
                instance_id: l.instance_id.clone(),
                display_name: None,
            })
            .collect()
    }

    /// Broadcast from the dashboard. Returns the number of instances reached.
    pub fn broadcast_from_dashboard(
        &self,
        channel_id: &str,
        context: Value,
    ) -> Result<usize, String> {
        let mut st = self.lock();
        channels::publish(
            &mut st,
            DASHBOARD_INSTANCE_ID,
            channel_id,
            context,
            &json!({}),
            self.events.as_ref(),
        )
        .map_err(str::to_owned)
    }

    /// Raise an intent from the dashboard at `target`, or else at the first
    /// running listener. The dashboard has no resolver, so it never launches.
    pub fn raise_from_dashboard(
        &self,
        intent: &str,
        context: Value,
        target: Option<&str>,
    ) -> Result<String, String> {
        let mut st = self.lock();
        intents::raise_from_dashboard(&mut st, intent, context, target, self.events.as_ref())
    }
}

/// One inbound request, as passed to the per-area handlers.
pub(crate) struct Request<'a> {
    pub from: &'a str,
    pub uuid: &'a str,
    pub payload: &'a Value,
}

#[cfg(test)]
mod tests;
