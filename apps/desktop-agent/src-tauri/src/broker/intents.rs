//! Intent discovery, resolution, delivery and results.
//!
//! When several apps or instances could handle an intent, the raiser gets an
//! `appIntent` back and its proxy runs the plugin's resolver UI, then raises
//! again against the chosen target.

use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::{
    channels, context_type, enrich_metadata, event, new_uuid, string_list, BrokerEvent,
    BrokerEvents, CdaIntentEvent, Effect, IntentListener, PendingIntent, PendingResult, Request,
    State, APP_LAUNCH_TIMEOUT, DASHBOARD_INSTANCE_ID,
};
use crate::app_dir::{AppRecord, IntentDef};

const NO_APPS: &str = "NoAppsFound";
const TARGET_APP_UNAVAILABLE: &str = "TargetAppUnavailable";
const TARGET_INSTANCE_UNAVAILABLE: &str = "TargetInstanceUnavailable";
const DELIVERY_FAILED: &str = "IntentDeliveryFailed";
const MALFORMED_CONTEXT: &str = "MalformedContext";

/// One raise being resolved, whichever request type it arrived as.
pub(super) struct Raise {
    request_uuid: String,
    response_type: &'static str,
    from: String,
    intent: String,
    context: Value,
    metadata: Value,
    new_instance: Option<bool>,
}

// ── Matching ───────────────────────────────────────────────────────────────────

/// An empty `contexts` declaration accepts any context.
fn accepts_context(def: &IntentDef, ctx_type: Option<&str>) -> bool {
    match ctx_type {
        Some(t) => def.contexts.is_empty() || def.contexts.iter().any(|c| c == t),
        None => true,
    }
}

/// `channel` matches any `channel<...>` declaration.
fn result_type_matches(def: &IntentDef, wanted: Option<&str>) -> bool {
    match (wanted, def.result_type.as_deref()) {
        (None, _) => true,
        (Some(w), Some(r)) => r == w || (w == "channel" && r.starts_with("channel<")),
        (Some(_), None) => false,
    }
}

fn declaration<'a>(app: &'a AppRecord, intent: &str) -> Option<&'a IntentDef> {
    app.interop
        .as_ref()?
        .intents
        .as_ref()?
        .listens_for
        .as_ref()?
        .get(intent)
}

fn declares(app: &AppRecord, intent: &str, ctx_type: Option<&str>) -> bool {
    declaration(app, intent).is_some_and(|d| accepts_context(d, ctx_type))
}

fn listener_accepts(l: &IntentListener, ctx_type: Option<&str>) -> bool {
    match (&l.context_types, ctx_type) {
        (Some(types), Some(t)) => types.iter().any(|c| c == t),
        _ => true,
    }
}

fn running_listeners<'a>(
    st: &'a State,
    intent: &'a str,
    ctx_type: Option<&'a str>,
) -> impl Iterator<Item = &'a IntentListener> {
    st.intent_listeners
        .iter()
        .filter(move |l| l.intent == intent && listener_accepts(l, ctx_type))
}

fn app_entry(app_id: &str) -> Value {
    json!({ "appId": app_id })
}

fn instance_entry(st: &State, instance_id: &str) -> Value {
    st.app_identifier(instance_id)
}

fn intent_metadata(name: &str, directory: &[AppRecord]) -> Value {
    let display = directory
        .iter()
        .find_map(|a| declaration(a, name)?.display_name.as_deref())
        .unwrap_or(name);
    json!({ "name": name, "displayName": display })
}

/// Every app and running instance that can handle `intent`: one bare `appId`
/// entry per directory app, plus one entry per running instance.
fn candidates(
    st: &State,
    intent: &str,
    ctx_type: Option<&str>,
    directory: &[AppRecord],
) -> Vec<Value> {
    let mut apps: Vec<Value> = Vec::new();
    for app in directory.iter().filter(|a| declares(a, intent, ctx_type)) {
        apps.push(app_entry(&app.app_id));
    }
    let mut seen: Vec<&str> = Vec::new();
    for l in running_listeners(st, intent, ctx_type) {
        if !seen.contains(&l.instance_id.as_str()) {
            seen.push(&l.instance_id);
            apps.push(instance_entry(st, &l.instance_id));
        }
    }
    apps
}

// ── Discovery ──────────────────────────────────────────────────────────────────

pub(super) fn find_intent(st: &State, req: &Request, directory: &[AppRecord]) {
    let intent = req.payload["intent"].as_str().unwrap_or_default();
    let ctx_type = context_type(&req.payload["context"]);
    let wanted = req.payload["resultType"].as_str();

    let mut apps: Vec<Value> = directory
        .iter()
        .filter(|a| {
            declaration(a, intent)
                .is_some_and(|d| accepts_context(d, ctx_type) && result_type_matches(d, wanted))
        })
        .map(|a| app_entry(&a.app_id))
        .collect();
    let mut seen: Vec<&str> = Vec::new();
    for l in running_listeners(st, intent, ctx_type) {
        if seen.contains(&l.instance_id.as_str()) {
            continue;
        }
        // Listeners don't declare a result type, so only the directory can
        // confirm one for a running app.
        let app_id = st.app_id_of(&l.instance_id);
        let result_ok = wanted.is_none()
            || directory
                .iter()
                .find(|a| a.app_id == app_id)
                .and_then(|a| declaration(a, intent))
                .is_some_and(|d| result_type_matches(d, wanted));
        if result_ok {
            seen.push(&l.instance_id);
            apps.push(instance_entry(st, &l.instance_id));
        }
    }
    st.respond(
        req.from,
        req.uuid,
        "findIntentResponse",
        json!({ "appIntent": { "intent": intent_metadata(intent, directory), "apps": apps } }),
    );
}

/// Intent names handling `ctx_type`, from the directory and running listeners.
fn intents_for_context(st: &State, ctx_type: Option<&str>, directory: &[AppRecord]) -> Vec<String> {
    let mut names: BTreeMap<String, ()> = BTreeMap::new();
    for app in directory {
        let declared = app
            .interop
            .as_ref()
            .and_then(|i| i.intents.as_ref())
            .and_then(|i| i.listens_for.as_ref());
        for (name, def) in declared.into_iter().flatten() {
            if accepts_context(def, ctx_type) {
                names.insert(name.clone(), ());
            }
        }
    }
    for l in st
        .intent_listeners
        .iter()
        .filter(|l| listener_accepts(l, ctx_type))
    {
        names.insert(l.intent.clone(), ());
    }
    names.into_keys().collect()
}

pub(super) fn find_intents_by_context(st: &State, req: &Request, directory: &[AppRecord]) {
    let ctx_type = context_type(&req.payload["context"]);
    let wanted = req.payload["resultType"].as_str();
    let app_intents: Vec<Value> = intents_for_context(st, ctx_type, directory)
        .into_iter()
        .filter(|name| {
            wanted.is_none()
                || directory
                    .iter()
                    .filter_map(|a| declaration(a, name))
                    .any(|d| result_type_matches(d, wanted))
        })
        .map(|name| {
            json!({
                "intent": intent_metadata(&name, directory),
                "apps": candidates(st, &name, ctx_type, directory),
            })
        })
        .collect();
    st.respond(
        req.from,
        req.uuid,
        "findIntentsByContextResponse",
        json!({ "appIntents": app_intents }),
    );
}

// ── Listeners ──────────────────────────────────────────────────────────────────

pub(super) fn add_listener(st: &mut State, req: &Request, events: &dyn BrokerEvents) {
    let intent = req.payload["intent"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let context_types = string_list(req.payload.get("contextTypes"));
    let uuid = new_uuid();
    st.intent_listeners.push(IntentListener {
        uuid: uuid.clone(),
        instance_id: req.from.to_string(),
        intent: intent.clone(),
        context_types: context_types.clone(),
    });
    st.respond(
        req.from,
        req.uuid,
        "addIntentListenerResponse",
        json!({ "listenerUUID": uuid }),
    );

    // Deliver anything that was waiting for this listener to appear.
    let app_id = st.app_id_of(req.from);
    let listener = IntentListener {
        uuid: String::new(),
        instance_id: req.from.to_string(),
        intent,
        context_types,
    };
    let waiting: Vec<PendingIntent>;
    (waiting, st.pending_intents) = std::mem::take(&mut st.pending_intents)
        .into_iter()
        .partition(|p| {
            p.app_id == app_id
                && p.raise.intent == listener.intent
                && p.instance_id.as_deref().is_none_or(|i| i == req.from)
                && listener_accepts(&listener, context_type(&p.raise.context))
        });
    for pending in waiting {
        deliver(st, &pending.raise, req.from, events);
    }
}

pub(super) fn remove_listener(st: &mut State, req: &Request) {
    const RESP: &str = "intentListenerUnsubscribeResponse";
    let uuid = req.payload["listenerUUID"].as_str().unwrap_or_default();
    let before = st.intent_listeners.len();
    st.intent_listeners
        .retain(|l| !(l.uuid == uuid && l.instance_id == req.from));
    if st.intent_listeners.len() == before {
        return st.respond_error(req.from, req.uuid, RESP, "InvalidArguments");
    }
    st.respond(req.from, req.uuid, RESP, json!({}));
}

// ── Raising ────────────────────────────────────────────────────────────────────

pub(super) fn raise_intent(
    st: &mut State,
    req: &Request,
    directory: &[AppRecord],
    events: &dyn BrokerEvents,
) -> Vec<Effect> {
    const RESP: &str = "raiseIntentResponse";
    let context = req.payload["context"].clone();
    if context_type(&context).is_none() {
        st.respond_error(req.from, req.uuid, RESP, MALFORMED_CONTEXT);
        return Vec::new();
    }
    let raise = new_raise(
        req,
        RESP,
        req.payload["intent"].as_str().unwrap_or_default(),
    );
    let target = &req.payload["app"];
    if target.get("appId").is_some() {
        return raise_to_target(st, raise, target, directory, events);
    }

    let ctx_type = context_type(&raise.context).map(str::to_owned);
    let apps = candidates(st, &raise.intent, ctx_type.as_deref(), directory);
    if apps.is_empty() {
        st.respond_error(req.from, req.uuid, RESP, NO_APPS);
        return Vec::new();
    }
    if let Some(app_id) = single_app(&apps) {
        return raise_within_app(st, raise, &app_id, &apps, directory, events);
    }
    let payload = json!({
        "appIntent": { "intent": intent_metadata(&raise.intent, directory), "apps": apps },
    });
    st.respond(req.from, req.uuid, RESP, payload);
    Vec::new()
}

pub(super) fn raise_intent_for_context(
    st: &mut State,
    req: &Request,
    directory: &[AppRecord],
    events: &dyn BrokerEvents,
) -> Vec<Effect> {
    const RESP: &str = "raiseIntentForContextResponse";
    let context = &req.payload["context"];
    let Some(ctx_type) = context_type(context) else {
        st.respond_error(req.from, req.uuid, RESP, MALFORMED_CONTEXT);
        return Vec::new();
    };
    let names = intents_for_context(st, Some(ctx_type), directory);

    let target = &req.payload["app"];
    if let Some(app_id) = target.get("appId").and_then(Value::as_str) {
        let instance = target["instanceId"].as_str();
        let name = names.iter().find(|name| {
            running_listeners(st, name, Some(ctx_type)).any(|l| {
                instance.map_or(st.app_id_of(&l.instance_id) == app_id, |i| {
                    l.instance_id == i
                })
            }) || directory
                .iter()
                .any(|a| a.app_id == app_id && declares(a, name, Some(ctx_type)))
        });
        return match name {
            Some(name) => {
                let raise = new_raise(req, RESP, name);
                raise_to_target(st, raise, target, directory, events)
            }
            None => {
                st.respond_error(req.from, req.uuid, RESP, NO_APPS);
                Vec::new()
            }
        };
    }

    let app_intents: Vec<(String, Vec<Value>)> = names
        .into_iter()
        .map(|name| {
            let apps = candidates(st, &name, Some(ctx_type), directory);
            (name, apps)
        })
        .filter(|(_, apps)| !apps.is_empty())
        .collect();
    match app_intents.as_slice() {
        [] => {
            st.respond_error(req.from, req.uuid, RESP, NO_APPS);
            Vec::new()
        }
        [(name, apps)] if single_app(apps).is_some() => {
            let app_id = single_app(apps).unwrap_or_default();
            let raise = new_raise(req, RESP, name);
            raise_within_app(st, raise, &app_id, apps, directory, events)
        }
        _ => {
            let list: Vec<Value> = app_intents
                .iter()
                .map(|(name, apps)| {
                    json!({ "intent": intent_metadata(name, directory), "apps": apps })
                })
                .collect();
            st.respond(req.from, req.uuid, RESP, json!({ "appIntents": list }));
            Vec::new()
        }
    }
}

fn new_raise(req: &Request, response_type: &'static str, intent: &str) -> Raise {
    Raise {
        request_uuid: req.uuid.to_string(),
        response_type,
        from: req.from.to_string(),
        intent: intent.to_string(),
        context: req.payload["context"].clone(),
        metadata: req.payload["metadata"].clone(),
        new_instance: req.payload["newInstance"].as_bool(),
    }
}

/// The appId shared by every entry, if there's only one.
fn single_app(apps: &[Value]) -> Option<String> {
    let first = apps.first()?["appId"].as_str()?;
    apps.iter()
        .all(|a| a["appId"].as_str() == Some(first))
        .then(|| first.to_string())
}

fn instances_in(apps: &[Value]) -> Vec<String> {
    apps.iter()
        .filter_map(|a| a["instanceId"].as_str().map(str::to_owned))
        .collect()
}

/// Resolve an untargeted raise that only one app can handle.
fn raise_within_app(
    st: &mut State,
    raise: Raise,
    app_id: &str,
    apps: &[Value],
    directory: &[AppRecord],
    events: &dyn BrokerEvents,
) -> Vec<Effect> {
    let instances = instances_in(apps);
    match (raise.new_instance, instances.as_slice()) {
        (Some(true), _) | (None, []) => launch_for_intent(st, raise, app_id, directory),
        (Some(false), []) => {
            fail(st, &raise, TARGET_INSTANCE_UNAVAILABLE);
            Vec::new()
        }
        (_, [only]) => {
            deliver(st, &raise, only, events);
            Vec::new()
        }
        _ => {
            respond_with_resolver_choice(st, &raise, apps, directory);
            Vec::new()
        }
    }
}

/// Resolve a raise with an explicit `app` target.
fn raise_to_target(
    st: &mut State,
    raise: Raise,
    target: &Value,
    directory: &[AppRecord],
    events: &dyn BrokerEvents,
) -> Vec<Effect> {
    let app_id = target["appId"].as_str().unwrap_or_default().to_string();
    let ctx_type = context_type(&raise.context).map(str::to_owned);
    let ctx_type = ctx_type.as_deref();

    if let Some(instance_id) = target["instanceId"].as_str() {
        if st
            .instances
            .get(instance_id)
            .is_none_or(|i| i.app_id != app_id)
        {
            fail(st, &raise, TARGET_INSTANCE_UNAVAILABLE);
            return Vec::new();
        }
        let listening =
            running_listeners(st, &raise.intent, ctx_type).any(|l| l.instance_id == instance_id);
        if listening {
            deliver(st, &raise, instance_id, events);
            return Vec::new();
        }
        // A declared handler may simply not have registered its listener yet.
        let declared = directory
            .iter()
            .any(|a| a.app_id == app_id && declares(a, &raise.intent, ctx_type));
        if !declared {
            fail(st, &raise, NO_APPS);
            return Vec::new();
        }
        return wait_for_listener(st, raise, &app_id, Some(instance_id.to_string()));
    }

    let record = directory.iter().find(|a| a.app_id == app_id);
    let running: Vec<String> = st
        .instances
        .iter()
        .filter(|(_, i)| i.app_id == app_id)
        .map(|(id, _)| id.clone())
        .collect();
    if record.is_none() && running.is_empty() {
        fail(st, &raise, TARGET_APP_UNAVAILABLE);
        return Vec::new();
    }

    if raise.new_instance == Some(false) {
        let listening: Vec<String> = running
            .into_iter()
            .filter(|id| {
                running_listeners(st, &raise.intent, ctx_type).any(|l| l.instance_id == *id)
            })
            .collect();
        return match listening.as_slice() {
            [] => {
                fail(st, &raise, TARGET_INSTANCE_UNAVAILABLE);
                Vec::new()
            }
            [only] => {
                deliver(st, &raise, only, events);
                Vec::new()
            }
            many => {
                let apps: Vec<Value> = many.iter().map(|i| instance_entry(st, i)).collect();
                respond_with_resolver_choice(st, &raise, &apps, directory);
                Vec::new()
            }
        };
    }

    // An appId with no instance asks for a new instance, which is also what the
    // resolver's "new instance" row sends. Apps outside the directory can't be
    // launched, so they use a running listener instead.
    if record.is_some_and(|a| declares(a, &raise.intent, ctx_type)) {
        return launch_for_intent(st, raise, &app_id, directory);
    }
    let listener = running_listeners(st, &raise.intent, ctx_type)
        .find(|l| running.contains(&l.instance_id))
        .map(|l| l.instance_id.clone());
    match listener {
        Some(instance_id) => deliver(st, &raise, &instance_id, events),
        None => fail(st, &raise, NO_APPS),
    }
    Vec::new()
}

fn respond_with_resolver_choice(
    st: &State,
    raise: &Raise,
    apps: &[Value],
    directory: &[AppRecord],
) {
    let app_intent = json!({ "intent": intent_metadata(&raise.intent, directory), "apps": apps });
    let payload = if raise.response_type == "raiseIntentResponse" {
        json!({ "appIntent": app_intent })
    } else {
        json!({ "appIntents": [app_intent] })
    };
    st.respond(
        &raise.from,
        &raise.request_uuid,
        raise.response_type,
        payload,
    );
}

fn fail(st: &State, raise: &Raise, error: &str) {
    st.respond_error(&raise.from, &raise.request_uuid, raise.response_type, error);
}

fn launch_for_intent(
    st: &mut State,
    raise: Raise,
    app_id: &str,
    directory: &[AppRecord],
) -> Vec<Effect> {
    let Some(app) = directory.iter().find(|a| a.app_id == app_id).cloned() else {
        fail(st, &raise, TARGET_APP_UNAVAILABLE);
        return Vec::new();
    };
    let mut effects = wait_for_listener(st, raise, app_id, None);
    if let Some(Effect::ExpireAfter { pending_id, .. }) = effects.first() {
        effects.insert(
            0,
            Effect::Launch {
                pending_id: pending_id.clone(),
                app: Box::new(app),
            },
        );
    }
    effects
}

fn wait_for_listener(
    st: &mut State,
    raise: Raise,
    app_id: &str,
    instance_id: Option<String>,
) -> Vec<Effect> {
    let pending_id = new_uuid();
    st.pending_intents.push(PendingIntent {
        id: pending_id.clone(),
        raise,
        app_id: app_id.to_string(),
        instance_id,
    });
    vec![Effect::ExpireAfter {
        pending_id,
        after: APP_LAUNCH_TIMEOUT,
    }]
}

/// Fail `pending_id` if it's an outstanding intent. Used for both timeouts and
/// failed launches.
pub(super) fn fail_pending(st: &mut State, pending_id: &str) {
    if let Some(i) = st.pending_intents.iter().position(|p| p.id == pending_id) {
        let pending = st.pending_intents.remove(i);
        fail(st, &pending.raise, DELIVERY_FAILED);
    }
}

/// Send the `intentEvent`, then resolve the raise. The result, if any, follows
/// later through `intent_result`.
fn deliver(st: &mut State, raise: &Raise, handler: &str, events: &dyn BrokerEvents) {
    let metadata = enrich_metadata(st.app_identifier(&raise.from), &raise.metadata);
    st.send(
        handler,
        &event(
            "intentEvent",
            json!({
                "intent": raise.intent,
                "context": raise.context,
                "metadata": metadata,
                "raiseIntentRequestUuid": raise.request_uuid,
            }),
        ),
    );
    st.pending_results.insert(
        raise.request_uuid.clone(),
        PendingResult {
            raiser: raise.from.clone(),
            handler: handler.to_string(),
        },
    );
    let source = st.app_identifier(handler);
    st.respond(
        &raise.from,
        &raise.request_uuid,
        raise.response_type,
        json!({ "intentResolution": { "intent": raise.intent, "source": source } }),
    );
    events.emit(BrokerEvent::Intent(CdaIntentEvent {
        intent: raise.intent.clone(),
        source_instance_id: raise.from.clone(),
        handler_instance_id: handler.to_string(),
        request_id: raise.request_uuid.clone(),
    }));
}

// ── Results ────────────────────────────────────────────────────────────────────

pub(super) fn intent_result(st: &mut State, req: &Request) {
    let raise_uuid = req.payload["raiseIntentRequestUuid"]
        .as_str()
        .unwrap_or_default();
    let is_handler = st
        .pending_results
        .get(raise_uuid)
        .is_some_and(|p| p.handler == req.from);
    if is_handler {
        if let Some(pending) = st.pending_results.remove(raise_uuid) {
            let result = req
                .payload
                .get("intentResult")
                .cloned()
                .unwrap_or(json!({}));
            if result["channel"]["type"] == "private" {
                if let Some(channel_id) = result["channel"]["id"].as_str() {
                    channels::grant_private_access(st, channel_id, req.from, &pending.raiser);
                }
            }
            let metadata = enrich_metadata(st.app_identifier(req.from), &req.payload["metadata"]);
            st.respond(
                &pending.raiser,
                raise_uuid,
                "raiseIntentResultResponse",
                json!({ "intentResult": result, "resultMetadata": metadata }),
            );
        }
    }
    st.respond(req.from, req.uuid, "intentResultResponse", json!({}));
}

// ── Dashboard ──────────────────────────────────────────────────────────────────

pub(super) fn raise_from_dashboard(
    st: &mut State,
    intent: &str,
    context: Value,
    target: Option<&str>,
    events: &dyn BrokerEvents,
) -> Result<String, String> {
    let ctx_type = context_type(&context).map(str::to_owned);
    let handler = match target {
        Some(id) => st.instances.contains_key(id).then(|| id.to_string()),
        None => running_listeners(st, intent, ctx_type.as_deref())
            .next()
            .map(|l| l.instance_id.clone()),
    }
    .ok_or_else(|| format!("No handler found for intent '{intent}'"))?;

    let raise = Raise {
        request_uuid: new_uuid(),
        response_type: "raiseIntentResponse",
        from: DASHBOARD_INSTANCE_ID.to_string(),
        intent: intent.to_string(),
        context,
        metadata: Value::Null,
        new_instance: None,
    };
    deliver(st, &raise, &handler, events);
    Ok(raise.request_uuid)
}

// ── Disconnect ─────────────────────────────────────────────────────────────────

pub(super) fn on_disconnect(st: &mut State, instance_id: &str) {
    st.intent_listeners.retain(|l| l.instance_id != instance_id);
    st.pending_intents.retain(|p| p.raise.from != instance_id);

    // A raiser waiting on a result from this handler would otherwise wait forever.
    let abandoned: Vec<(String, String)> = st
        .pending_results
        .iter()
        .filter(|(_, p)| p.handler == instance_id)
        .map(|(uuid, p)| (uuid.clone(), p.raiser.clone()))
        .collect();
    for (uuid, raiser) in abandoned {
        st.pending_results.remove(&uuid);
        st.respond_error(
            &raiser,
            &uuid,
            "raiseIntentResultResponse",
            "NoResultReturned",
        );
    }
    st.pending_results.retain(|_, p| p.raiser != instance_id);
}
