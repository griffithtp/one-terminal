use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver};

use super::*;
use crate::app_dir::{AppDetails, AppIntents, AppInterop, IntentDef};

#[derive(Default)]
struct RecordedEvents(Mutex<Vec<String>>);

impl BrokerEvents for RecordedEvents {
    fn emit(&self, event: BrokerEvent) {
        let name = match event {
            BrokerEvent::Connected(_) => "connected",
            BrokerEvent::Disconnected(_) => "disconnected",
            BrokerEvent::ChannelChanged(_) => "channel_changed",
            BrokerEvent::Context(_) => "context",
            BrokerEvent::Intent(_) => "intent",
        };
        self.0.lock().unwrap().push(name.to_string());
    }
}

struct App {
    id: String,
    rx: UnboundedReceiver<String>,
}

impl App {
    fn drain(&mut self) -> Vec<Value> {
        let mut out = Vec::new();
        while let Ok(s) = self.rx.try_recv() {
            out.push(serde_json::from_str(&s).unwrap());
        }
        out
    }

    fn drain_types(&mut self) -> Vec<String> {
        self.drain()
            .iter()
            .map(|m| m["type"].as_str().unwrap().to_string())
            .collect()
    }
}

struct Harness {
    broker: Broker,
    events: Arc<RecordedEvents>,
    directory: Vec<AppRecord>,
}

fn directory_app(app_id: &str, intents: &[(&str, &[&str])]) -> AppRecord {
    let listens_for = intents
        .iter()
        .map(|(name, contexts)| {
            (
                name.to_string(),
                IntentDef {
                    contexts: contexts.iter().map(|c| c.to_string()).collect(),
                    ..Default::default()
                },
            )
        })
        .collect();
    AppRecord {
        app_id: app_id.to_string(),
        app_type: "web".to_string(),
        details: Some(AppDetails {
            url: Some("http://localhost/".to_string()),
        }),
        interop: Some(AppInterop {
            intents: Some(AppIntents {
                listens_for: Some(listens_for),
                raises: None,
            }),
        }),
        ..Default::default()
    }
}

fn harness(directory: Vec<AppRecord>) -> Harness {
    let events = Arc::new(RecordedEvents::default());
    let broker = Broker::new(events.clone());
    broker.add_user_channel("Red", "Red", "#f00");
    broker.add_user_channel("Blue", "Blue", "#00f");
    Harness {
        broker,
        events,
        directory,
    }
}

impl Harness {
    fn connect(&self, app_id: &str) -> App {
        let (tx, rx) = unbounded_channel();
        let id = self.broker.connect(app_id, "attempt", &self.directory, tx);
        let mut app = App { id, rx };
        assert_eq!(app.drain_types(), ["WCP5ValidateAppIdentityResponse"]);
        app
    }

    fn send(&self, app: &App, msg_type: &str, payload: Value) -> Vec<Effect> {
        let msg = json!({
            "type": msg_type,
            "payload": payload,
            "meta": { "requestUuid": format!("req-{msg_type}"), "timestamp": iso_timestamp() },
        });
        self.broker.handle(&app.id, &msg, &self.directory)
    }

    /// Send a request and return its response, which must be the only message queued.
    fn request(&self, app: &mut App, msg_type: &str, payload: Value) -> Value {
        self.send(app, msg_type, payload);
        let mut messages = app.drain();
        assert_eq!(messages.len(), 1, "expected one reply: {messages:?}");
        messages.remove(0)
    }

    fn join(&self, app: &mut App, channel: &str) {
        self.request(
            app,
            "joinUserChannelRequest",
            json!({ "channelId": channel }),
        );
    }

    fn listen(&self, app: &mut App, payload: Value) -> String {
        let resp = self.request(app, "addContextListenerRequest", payload);
        resp["payload"]["listenerUUID"]
            .as_str()
            .unwrap()
            .to_string()
    }

    fn listen_intent(&self, app: &mut App, intent: &str) {
        self.request(app, "addIntentListenerRequest", json!({ "intent": intent }));
    }

    fn create_private(&self, app: &mut App) -> String {
        let resp = self.request(app, "createPrivateChannelRequest", json!({}));
        resp["payload"]["privateChannel"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }
}

fn instrument() -> Value {
    json!({ "type": "fdc3.instrument", "id": { "ticker": "AAPL" } })
}

// ── Channels and broadcast ─────────────────────────────────────────────────────

#[test]
fn broadcast_reaches_only_listeners_on_the_same_channel() {
    let h = harness(vec![]);
    let (mut sender, mut same, mut other) = (h.connect("a"), h.connect("b"), h.connect("c"));
    h.join(&mut sender, "Red");
    h.join(&mut same, "Red");
    h.join(&mut other, "Blue");
    h.listen(&mut same, json!({ "channelId": null, "contextType": null }));
    h.listen(
        &mut other,
        json!({ "channelId": null, "contextType": null }),
    );

    let resp = h.request(
        &mut sender,
        "broadcastRequest",
        json!({ "channelId": "Red", "context": instrument() }),
    );

    assert_eq!(resp["type"], "broadcastResponse");
    let delivered = same.drain();
    assert_eq!(delivered.len(), 1);
    assert_eq!(delivered[0]["type"], "broadcastEvent");
    assert_eq!(delivered[0]["payload"]["channelId"], "Red");
    assert!(other.drain().is_empty());
}

#[test]
fn listener_context_filters_apply_and_instances_get_one_event() {
    let h = harness(vec![]);
    let (mut sender, mut single, mut array) = (h.connect("a"), h.connect("b"), h.connect("c"));
    for app in [&mut single, &mut array] {
        h.join(app, "Red");
    }
    h.listen(
        &mut single,
        json!({ "channelId": null, "contextType": "fdc3.contact" }),
    );
    h.listen(
        &mut array,
        json!({ "channelId": null, "contextTypes": ["fdc3.instrument", "fdc3.contact"] }),
    );
    // A second matching listener must not produce a second event: the proxy
    // fans each event out to its own listeners.
    h.listen(
        &mut array,
        json!({ "channelId": null, "contextType": null }),
    );

    h.request(
        &mut sender,
        "broadcastRequest",
        json!({ "channelId": "Red", "context": instrument() }),
    );

    assert!(single.drain().is_empty());
    assert_eq!(array.drain().len(), 1);
}

#[test]
fn broadcast_rejects_malformed_context_and_unknown_channel() {
    let h = harness(vec![]);
    let mut app = h.connect("a");
    let bad = h.request(
        &mut app,
        "broadcastRequest",
        json!({ "channelId": "Red", "context": { "id": {} } }),
    );
    assert_eq!(bad["payload"]["error"], "MalformedContext");
    let missing = h.request(
        &mut app,
        "broadcastRequest",
        json!({ "channelId": "Nope", "context": instrument() }),
    );
    assert_eq!(missing["payload"]["error"], "NoChannelFound");
}

#[test]
fn join_rejects_app_channels_and_reports_channel_changes() {
    let h = harness(vec![]);
    let mut app = h.connect("a");
    h.request(
        &mut app,
        "getOrCreateChannelRequest",
        json!({ "channelId": "my-app-channel" }),
    );
    let resp = h.request(
        &mut app,
        "joinUserChannelRequest",
        json!({ "channelId": "my-app-channel" }),
    );
    assert_eq!(resp["payload"]["error"], "NoChannelFound");

    h.request(
        &mut app,
        "addEventListenerRequest",
        json!({ "type": "USER_CHANNEL_CHANGED", "channelId": null }),
    );
    h.send(
        &app,
        "joinUserChannelRequest",
        json!({ "channelId": "Red" }),
    );
    let messages = app.drain();
    assert_eq!(messages[0]["type"], "channelChangedEvent");
    assert_eq!(messages[0]["payload"]["newChannelId"], "Red");
    assert_eq!(messages[1]["type"], "joinUserChannelResponse");
    assert_eq!(h.broker.list_channels()[0].members, vec![app.id.clone()]);
}

#[test]
fn current_context_tracks_history_by_type_and_clear_removes_it() {
    let h = harness(vec![]);
    let mut app = h.connect("a");
    let contact = json!({ "type": "fdc3.contact", "id": { "email": "a@b.c" } });
    for ctx in [instrument(), contact.clone()] {
        h.request(
            &mut app,
            "broadcastRequest",
            json!({ "channelId": "Red", "context": ctx }),
        );
    }

    let latest = h.request(
        &mut app,
        "getCurrentContextRequest",
        json!({ "channelId": "Red", "contextType": null }),
    );
    assert_eq!(latest["payload"]["context"], contact);
    assert!(latest["payload"]["metadata"]["traceId"].is_string());
    let by_type = h.request(
        &mut app,
        "getCurrentContextRequest",
        json!({ "channelId": "Red", "contextType": "fdc3.instrument" }),
    );
    assert_eq!(by_type["payload"]["context"], instrument());

    h.request(
        &mut app,
        "clearContextRequest",
        json!({ "channelId": "Red", "contextType": "fdc3.instrument" }),
    );
    let cleared = h.request(
        &mut app,
        "getCurrentContextRequest",
        json!({ "channelId": "Red", "contextType": "fdc3.instrument" }),
    );
    assert_eq!(cleared["payload"]["context"], Value::Null);
    assert_eq!(cleared["payload"]["metadata"], Value::Null);
    let other_type = h.request(
        &mut app,
        "getCurrentContextRequest",
        json!({ "channelId": "Red", "contextType": "fdc3.contact" }),
    );
    assert_eq!(other_type["payload"]["context"], contact);
}

#[test]
fn context_cleared_event_scopes_to_channel_and_skips_the_clearer() {
    let h = harness(vec![]);
    let (mut clearer, mut on_channel, mut elsewhere) =
        (h.connect("a"), h.connect("b"), h.connect("c"));
    for app in [&mut clearer, &mut on_channel, &mut elsewhere] {
        h.request(
            app,
            "addEventListenerRequest",
            json!({ "type": "CONTEXT_CLEARED", "channelId": null }),
        );
    }
    h.join(&mut clearer, "Red");
    h.join(&mut on_channel, "Red");
    h.join(&mut elsewhere, "Blue");

    h.request(
        &mut clearer,
        "clearContextRequest",
        json!({ "channelId": "Red", "contextType": null }),
    );

    let events = on_channel.drain();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["type"], "contextClearedEvent");
    assert_eq!(events[0]["payload"]["channelId"], "Red");
    assert!(elsewhere.drain().is_empty());
}

// ── Metadata ───────────────────────────────────────────────────────────────────

#[test]
fn metadata_is_enriched_and_app_fields_forwarded_but_source_is_not_spoofable() {
    let h = harness(vec![]);
    let (mut sender, mut listener) = (h.connect("sender"), h.connect("listener"));
    h.listen(
        &mut listener,
        json!({ "channelId": "Red", "contextType": null }),
    );

    h.request(
        &mut sender,
        "broadcastRequest",
        json!({
            "channelId": "Red",
            "context": instrument(),
            "metadata": {
                "traceId": "trace-1",
                "signature": "sig",
                "custom": { "k": "v" },
                "source": { "appId": "spoofed" },
            },
        }),
    );

    let metadata = listener.drain().remove(0)["payload"]["metadata"].clone();
    assert_eq!(metadata["traceId"], "trace-1");
    assert_eq!(metadata["signature"], "sig");
    assert_eq!(metadata["custom"]["k"], "v");
    assert_eq!(metadata["source"]["appId"], "sender");
    assert_eq!(metadata["source"]["instanceId"], sender.id.as_str());
    assert!(metadata["timestamp"].is_string());
}

#[test]
fn trace_id_is_generated_when_the_app_supplies_none() {
    let h = harness(vec![]);
    let (mut sender, mut listener) = (h.connect("sender"), h.connect("listener"));
    h.listen(
        &mut listener,
        json!({ "channelId": "Red", "contextType": null }),
    );
    h.request(
        &mut sender,
        "broadcastRequest",
        json!({ "channelId": "Red", "context": instrument() }),
    );
    let trace = &listener.drain()[0]["payload"]["metadata"]["traceId"];
    assert!(trace.as_str().is_some_and(|t| !t.is_empty()));
}

// ── Private channels ───────────────────────────────────────────────────────────

#[test]
fn private_channels_deny_apps_without_access() {
    let h = harness(vec![]);
    let (mut owner, mut outsider) = (h.connect("owner"), h.connect("outsider"));
    let id = h.create_private(&mut owner);

    for (msg_type, payload) in [
        (
            "broadcastRequest",
            json!({ "channelId": id, "context": instrument() }),
        ),
        (
            "addContextListenerRequest",
            json!({ "channelId": id, "contextType": null }),
        ),
        (
            "getCurrentContextRequest",
            json!({ "channelId": id, "contextType": null }),
        ),
        ("getOrCreateChannelRequest", json!({ "channelId": id })),
        (
            "privateChannelDisconnectRequest",
            json!({ "channelId": id }),
        ),
    ] {
        let resp = h.request(&mut outsider, msg_type, payload);
        assert_eq!(resp["payload"]["error"], "AccessDenied", "{msg_type}");
    }
}

/// Connect a handler, have it return a private channel from an intent, and
/// return `(raiser, handler, channel id)`.
fn private_channel_via_intent(h: &Harness) -> (App, App, String) {
    let (mut raiser, mut handler) = (h.connect("raiser"), h.connect("handler"));
    h.listen_intent(&mut handler, "Chat");
    let id = h.create_private(&mut handler);
    h.send(
        &raiser,
        "raiseIntentRequest",
        json!({ "intent": "Chat", "context": instrument() }),
    );
    let intent_event = handler.drain().remove(0);
    raiser.drain();
    h.request(
        &mut handler,
        "intentResultRequest",
        json!({
            "raiseIntentRequestUuid": intent_event["payload"]["raiseIntentRequestUuid"],
            "intentEventUuid": intent_event["meta"]["eventUuid"],
            "intentResult": { "channel": { "id": id, "type": "private" } },
        }),
    );
    let result = raiser.drain().remove(0);
    assert_eq!(result["type"], "raiseIntentResultResponse");
    (raiser, handler, id)
}

#[test]
fn returning_a_private_channel_as_an_intent_result_grants_the_raiser_access() {
    let h = harness(vec![]);
    let (mut raiser, _handler, id) = private_channel_via_intent(&h);
    let resp = h.request(
        &mut raiser,
        "addContextListenerRequest",
        json!({ "channelId": id, "contextType": null }),
    );
    assert!(resp["payload"]["listenerUUID"].is_string());
}

#[test]
fn private_channel_events_fire_for_listener_changes_and_disconnect() {
    let h = harness(vec![]);
    let (mut raiser, mut handler, id) = private_channel_via_intent(&h);
    h.request(
        &mut handler,
        "privateChannelAddEventListenerRequest",
        json!({ "privateChannelId": id, "listenerType": null }),
    );

    h.listen(
        &mut raiser,
        json!({ "channelId": id, "contextType": "fdc3.contact" }),
    );
    let added = handler.drain().remove(0);
    assert_eq!(added["type"], "privateChannelOnAddContextListenerEvent");
    assert_eq!(added["payload"]["contextType"], "fdc3.contact");

    h.send(
        &raiser,
        "privateChannelDisconnectRequest",
        json!({ "channelId": id }),
    );
    assert_eq!(
        handler.drain_types(),
        [
            "privateChannelOnUnsubscribeEvent",
            "privateChannelOnDisconnectEvent"
        ],
    );
    let denied = {
        raiser.drain();
        h.request(
            &mut raiser,
            "broadcastRequest",
            json!({ "channelId": id, "context": instrument() }),
        )
    };
    assert_eq!(denied["payload"]["error"], "AccessDenied");
}

#[test]
fn disconnecting_an_app_notifies_the_other_private_channel_party() {
    let h = harness(vec![]);
    let (mut raiser, mut handler, id) = private_channel_via_intent(&h);
    h.request(
        &mut handler,
        "privateChannelAddEventListenerRequest",
        json!({ "privateChannelId": id, "listenerType": "disconnect" }),
    );
    h.listen(&mut raiser, json!({ "channelId": id, "contextType": null }));
    handler.drain();

    h.broker.disconnect(&raiser.id);

    assert_eq!(handler.drain_types(), ["privateChannelOnDisconnectEvent"]);
}

// ── Intents ────────────────────────────────────────────────────────────────────

#[test]
fn intent_delivers_to_the_single_running_handler_and_routes_the_result() {
    let h = harness(vec![]);
    let (mut raiser, mut handler) = (h.connect("raiser"), h.connect("handler"));
    h.listen_intent(&mut handler, "ViewChart");

    h.send(
        &raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument(), "metadata": { "traceId": "t" } }),
    );

    let intent_event = handler.drain().remove(0);
    assert_eq!(intent_event["type"], "intentEvent");
    assert_eq!(intent_event["payload"]["metadata"]["traceId"], "t");
    assert_eq!(
        intent_event["payload"]["metadata"]["source"]["appId"],
        "raiser"
    );
    let resolution = raiser.drain().remove(0);
    assert_eq!(resolution["type"], "raiseIntentResponse");
    assert_eq!(
        resolution["payload"]["intentResolution"]["source"]["instanceId"],
        handler.id.as_str(),
    );

    let ack = h.request(
        &mut handler,
        "intentResultRequest",
        json!({
            "raiseIntentRequestUuid": "req-raiseIntentRequest",
            "intentEventUuid": intent_event["meta"]["eventUuid"],
            "intentResult": { "context": instrument() },
        }),
    );
    assert_eq!(ack["type"], "intentResultResponse");
    let result = raiser.drain().remove(0);
    assert_eq!(result["type"], "raiseIntentResultResponse");
    assert_eq!(result["meta"]["requestUuid"], "req-raiseIntentRequest");
    assert_eq!(result["payload"]["intentResult"]["context"], instrument());
    assert_eq!(
        result["payload"]["resultMetadata"]["source"]["appId"],
        "handler"
    );
}

#[test]
fn only_the_delivered_handler_can_resolve_a_result() {
    let h = harness(vec![]);
    let (mut raiser, mut handler, mut other) = (h.connect("r"), h.connect("h"), h.connect("o"));
    h.listen_intent(&mut handler, "ViewChart");
    h.send(
        &raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument() }),
    );
    raiser.drain();

    h.request(
        &mut other,
        "intentResultRequest",
        json!({ "raiseIntentRequestUuid": "req-raiseIntentRequest", "intentResult": {} }),
    );

    assert!(raiser.drain().is_empty());
}

#[test]
fn several_handlers_are_returned_to_the_raiser_for_resolution() {
    let h = harness(vec![]);
    let (mut raiser, mut one, mut two) = (h.connect("r"), h.connect("one"), h.connect("two"));
    h.listen_intent(&mut one, "ViewChart");
    h.listen_intent(&mut two, "ViewChart");

    let resp = h.request(
        &mut raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument() }),
    );

    let apps = resp["payload"]["appIntent"]["apps"].as_array().unwrap();
    assert_eq!(apps.len(), 2);
    assert!(one.drain().is_empty());
}

#[test]
fn listener_context_filter_narrows_candidates() {
    let h = harness(vec![]);
    let (mut raiser, mut handler) = (h.connect("r"), h.connect("h"));
    h.request(
        &mut handler,
        "addIntentListenerRequest",
        json!({ "intent": "ViewChart", "contextTypes": ["fdc3.contact"] }),
    );
    let resp = h.request(
        &mut raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument() }),
    );
    assert_eq!(resp["payload"]["error"], "NoAppsFound");
}

#[test]
fn raising_with_no_handler_launches_the_directory_app_then_delivers_on_listen() {
    let h = harness(vec![directory_app(
        "chart",
        &[("ViewChart", &["fdc3.instrument"])],
    )]);
    let mut raiser = h.connect("raiser");

    let effects = h.send(
        &raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument() }),
    );
    assert!(matches!(&effects[0], Effect::Launch { app, .. } if app.app_id == "chart"));
    assert!(matches!(&effects[1], Effect::ExpireAfter { .. }));
    assert!(raiser.drain().is_empty(), "held until the app is listening");

    let mut chart = h.connect("chart");
    h.send(
        &chart,
        "addIntentListenerRequest",
        json!({ "intent": "ViewChart" }),
    );

    // The listener's response must precede the event, or the proxy drops it.
    assert_eq!(
        chart.drain_types(),
        ["addIntentListenerResponse", "intentEvent"]
    );
    assert_eq!(raiser.drain_types(), ["raiseIntentResponse"]);
}

#[test]
fn pending_intent_times_out_with_delivery_failed() {
    let h = harness(vec![directory_app("chart", &[("ViewChart", &[])])]);
    let mut raiser = h.connect("raiser");
    let effects = h.send(
        &raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument() }),
    );
    let Effect::Launch { pending_id, .. } = &effects[0] else {
        panic!("expected a launch");
    };
    h.broker.expire(pending_id);
    let resp = raiser.drain().remove(0);
    assert_eq!(resp["payload"]["error"], "IntentDeliveryFailed");
}

#[test]
fn new_instance_flag_controls_launching() {
    let h = harness(vec![directory_app("chart", &[("ViewChart", &[])])]);
    let mut raiser = h.connect("raiser");

    let refused = h.request(
        &mut raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument(), "newInstance": false }),
    );
    assert_eq!(refused["payload"]["error"], "TargetInstanceUnavailable");

    let mut chart = h.connect("chart");
    h.listen_intent(&mut chart, "ViewChart");
    let effects = h.send(
        &raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument(), "newInstance": true }),
    );
    assert!(
        matches!(&effects[0], Effect::Launch { .. }),
        "launches despite a running one"
    );
    assert!(chart.drain().is_empty());
}

#[test]
fn targeted_raises_report_the_right_errors() {
    let h = harness(vec![directory_app("chart", &[("ViewChart", &[])])]);
    let mut raiser = h.connect("raiser");
    let cases = [
        (json!({ "appId": "nope" }), "TargetAppUnavailable"),
        (
            json!({ "appId": "chart", "instanceId": "gone" }),
            "TargetInstanceUnavailable",
        ),
    ];
    for (app, error) in cases {
        let resp = h.request(
            &mut raiser,
            "raiseIntentRequest",
            json!({ "intent": "ViewChart", "context": instrument(), "app": app }),
        );
        assert_eq!(resp["payload"]["error"], error);
    }
}

#[test]
fn find_intent_returns_directory_apps_and_running_instances() {
    let h = harness(vec![directory_app(
        "chart",
        &[("ViewChart", &["fdc3.instrument"])],
    )]);
    let (mut asker, mut chart) = (h.connect("asker"), h.connect("chart"));
    h.listen_intent(&mut chart, "ViewChart");

    let resp = h.request(
        &mut asker,
        "findIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument() }),
    );

    let apps = resp["payload"]["appIntent"]["apps"].as_array().unwrap();
    assert_eq!(apps.len(), 2);
    assert_eq!(apps[0], json!({ "appId": "chart" }));
    assert_eq!(apps[1]["instanceId"], chart.id.as_str());

    let by_context = h.request(
        &mut asker,
        "findIntentsByContextRequest",
        json!({ "context": { "type": "fdc3.contact" } }),
    );
    let intents = by_context["payload"]["appIntents"].as_array().unwrap();
    assert_eq!(
        intents.len(),
        1,
        "the unfiltered listener accepts any context"
    );
    assert_eq!(intents[0]["apps"].as_array().unwrap().len(), 1);
}

#[test]
fn losing_the_handler_ends_the_raisers_wait_for_a_result() {
    let h = harness(vec![]);
    let (mut raiser, mut handler) = (h.connect("r"), h.connect("h"));
    h.listen_intent(&mut handler, "ViewChart");
    h.send(
        &raiser,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument() }),
    );
    raiser.drain();

    h.broker.disconnect(&handler.id);

    let result = raiser.drain().remove(0);
    assert_eq!(result["type"], "raiseIntentResultResponse");
    assert_eq!(result["payload"]["error"], "NoResultReturned");
}

// ── open, close, metadata ──────────────────────────────────────────────────────

#[test]
fn open_without_context_resolves_when_the_app_connects() {
    let h = harness(vec![directory_app("chart", &[])]);
    let mut opener = h.connect("opener");
    let effects = h.send(
        &opener,
        "openRequest",
        json!({ "app": { "appId": "chart" } }),
    );
    assert!(matches!(&effects[0], Effect::Launch { .. }));
    assert!(opener.drain().is_empty());

    let chart = h.connect("chart");

    let resp = opener.drain().remove(0);
    assert_eq!(resp["type"], "openResponse");
    assert_eq!(
        resp["payload"]["appIdentifier"]["instanceId"],
        chart.id.as_str()
    );
}

#[test]
fn open_with_context_delivers_it_to_the_first_matching_listener() {
    let h = harness(vec![directory_app("chart", &[])]);
    let mut opener = h.connect("opener");
    h.send(
        &opener,
        "openRequest",
        json!({ "app": { "appId": "chart" }, "context": instrument() }),
    );
    let mut chart = h.connect("chart");
    assert!(
        opener.drain().is_empty(),
        "held until the context is delivered"
    );

    h.listen(
        &mut chart,
        json!({ "channelId": null, "contextType": "fdc3.contact" }),
    );
    assert!(opener.drain().is_empty(), "a different type doesn't match");

    h.send(
        &chart,
        "addContextListenerRequest",
        json!({ "channelId": null, "contextType": "fdc3.instrument" }),
    );
    let messages = chart.drain();
    assert_eq!(messages[0]["type"], "addContextListenerResponse");
    assert_eq!(messages[1]["type"], "broadcastEvent");
    assert_eq!(messages[1]["payload"]["channelId"], Value::Null);
    assert_eq!(
        messages[1]["payload"]["metadata"]["source"]["appId"],
        "opener"
    );
    assert_eq!(opener.drain_types(), ["openResponse"]);
}

#[test]
fn open_failures_use_the_spec_errors() {
    let h = harness(vec![directory_app("chart", &[])]);
    let mut opener = h.connect("opener");

    let missing = h.request(
        &mut opener,
        "openRequest",
        json!({ "app": { "appId": "nope" } }),
    );
    assert_eq!(missing["payload"]["error"], "AppNotFound");

    let malformed = h.request(
        &mut opener,
        "openRequest",
        json!({ "app": { "appId": "chart" }, "context": { "id": {} } }),
    );
    assert_eq!(malformed["payload"]["error"], "MalformedContext");

    // Never connects: the app isn't an FDC3 app.
    let effects = h.send(
        &opener,
        "openRequest",
        json!({ "app": { "appId": "chart" } }),
    );
    let Effect::Launch { pending_id, .. } = &effects[0] else {
        panic!("expected a launch");
    };
    h.broker.expire(pending_id);
    assert_eq!(opener.drain()[0]["payload"]["error"], "ApiTimeout");

    // Connects but never listens for the context.
    let effects = h.send(
        &opener,
        "openRequest",
        json!({ "app": { "appId": "chart" }, "context": instrument() }),
    );
    let Effect::Launch { pending_id, .. } = &effects[0] else {
        panic!("expected a launch");
    };
    let _chart = h.connect("chart");
    h.broker.expire(pending_id);
    assert_eq!(opener.drain()[0]["payload"]["error"], "AppTimeout");

    let effects = h.send(
        &opener,
        "openRequest",
        json!({ "app": { "appId": "chart" } }),
    );
    let Effect::Launch { pending_id, .. } = &effects[0] else {
        panic!("expected a launch");
    };
    h.broker
        .launch_finished(pending_id, Err("boom".to_string()));
    assert_eq!(opener.drain()[0]["payload"]["error"], "ErrorOnLaunch");
}

#[test]
fn close_works_only_for_windows_the_agent_opened() {
    let h = harness(vec![directory_app("chart", &[])]);
    let mut panel = h.connect("panel");
    let refused = h.request(&mut panel, "closeRequest", json!({}));
    assert_eq!(refused["payload"]["error"], "ErrorOnClose");

    let opener = h.connect("opener");
    let effects = h.send(
        &opener,
        "openRequest",
        json!({ "app": { "appId": "chart" } }),
    );
    let Effect::Launch { pending_id, .. } = &effects[0] else {
        panic!("expected a launch");
    };
    h.broker
        .launch_finished(pending_id, Ok(Some("app-chart-1".to_string())));
    let chart = h.connect("chart");

    let effects = h.send(&chart, "closeRequest", json!({}));
    assert!(matches!(&effects[0], Effect::CloseWindow { label, .. } if label == "app-chart-1"));
}

#[test]
fn get_info_reports_fdc3_3_and_find_instances_lists_running_apps() {
    let h = harness(vec![]);
    let (mut a, b) = (h.connect("same"), h.connect("same"));

    let info = h.request(&mut a, "getInfoRequest", json!({}));
    let metadata = &info["payload"]["implementationMetadata"];
    assert_eq!(metadata["fdc3Version"], "3.0");
    assert_eq!(metadata["appMetadata"]["instanceId"], a.id.as_str());
    assert_eq!(metadata["optionalFeatures"]["DesktopAgentBridging"], false);

    let found = h.request(
        &mut a,
        "findInstancesRequest",
        json!({ "app": { "appId": "same" } }),
    );
    let list = found["payload"]["appIdentifiers"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|v| v["instanceId"] == b.id.as_str()));
}

// ── Lifecycle ──────────────────────────────────────────────────────────────────

#[test]
fn disconnect_removes_all_state_and_is_idempotent() {
    let h = harness(vec![]);
    let (mut gone, mut sender) = (h.connect("gone"), h.connect("sender"));
    h.join(&mut gone, "Red");
    h.listen(&mut gone, json!({ "channelId": null, "contextType": null }));
    h.listen_intent(&mut gone, "ViewChart");

    assert!(h.broker.disconnect(&gone.id));
    assert!(!h.broker.disconnect(&gone.id));

    assert!(h
        .broker
        .list_connections()
        .iter()
        .all(|c| c.instance_id != gone.id));
    assert!(h.broker.list_intent_handlers().is_empty());
    assert!(h.broker.list_channels()[0].members.is_empty());
    let resp = h.request(
        &mut sender,
        "raiseIntentRequest",
        json!({ "intent": "ViewChart", "context": instrument() }),
    );
    assert_eq!(resp["payload"]["error"], "NoAppsFound");

    let events = h.events.0.lock().unwrap();
    assert_eq!(events.iter().filter(|e| *e == "disconnected").count(), 1);
}

#[test]
fn forced_disconnect_drops_the_only_sender_so_the_socket_loop_ends() {
    let h = harness(vec![]);
    let mut app = h.connect("a");
    h.broker.disconnect(&app.id);
    assert!(matches!(
        app.rx.try_recv(),
        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
    ));
}

#[test]
fn messages_from_disconnected_instances_are_ignored() {
    let h = harness(vec![]);
    let app = h.connect("a");
    h.broker.disconnect(&app.id);
    assert!(h.send(&app, "openRequest", json!({})).is_empty());
}

#[test]
fn unknown_request_types_get_an_error_response() {
    let h = harness(vec![]);
    let mut app = h.connect("a");
    let resp = h.request(&mut app, "madeUpRequest", json!({}));
    assert_eq!(resp["type"], "madeUpResponse");
    assert_eq!(resp["payload"]["error"], "InvalidArguments");
}

#[test]
fn dashboard_broadcast_and_raise_use_the_agent_identity() {
    let h = harness(vec![]);
    let mut app = h.connect("a");
    h.join(&mut app, "Red");
    h.listen(&mut app, json!({ "channelId": null, "contextType": null }));
    h.listen_intent(&mut app, "ViewChart");

    assert_eq!(
        h.broker
            .broadcast_from_dashboard("Red", instrument())
            .unwrap(),
        1
    );
    assert_eq!(
        app.drain()[0]["payload"]["metadata"]["source"]["appId"],
        "desktop-agent",
    );

    h.broker
        .raise_from_dashboard("ViewChart", instrument(), None)
        .unwrap();
    assert_eq!(app.drain()[0]["type"], "intentEvent");
    assert!(h
        .broker
        .raise_from_dashboard("Nope", instrument(), None)
        .is_err());
}
