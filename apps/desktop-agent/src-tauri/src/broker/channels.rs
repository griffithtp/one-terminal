//! User, app and private channels; context listeners; agent-level events.

use serde_json::{json, Value};
use std::collections::HashSet;

use super::{
    context_type, enrich_metadata, event, iso_timestamp, new_uuid, string_list, BrokerEvent,
    BrokerEvents, CdaChannelChangedEvent, CdaContextEvent, Channel, ChannelKind, ContextListener,
    EventListener, PrivateEventListener, Request, State,
};

const NO_CHANNEL: &str = "NoChannelFound";
const ACCESS_DENIED: &str = "AccessDenied";
const MALFORMED_CONTEXT: &str = "MalformedContext";
const LISTENER_NOT_FOUND: &str = "InvalidArguments";

fn channel_json(c: &Channel) -> Value {
    let mut out = json!({ "id": c.id, "type": c.kind.as_str() });
    if c.kind == ChannelKind::User {
        out["displayMetadata"] = json!({ "name": c.display_name, "color": c.color });
    }
    out
}

/// Whether `instance_id` may use channel `c`.
fn can_access(c: &Channel, instance_id: &str) -> bool {
    c.kind != ChannelKind::Private || c.access.contains(instance_id)
}

/// Resolve a channel the caller is allowed to use, or the DACP error for it.
fn accessible<'a>(
    st: &'a State,
    channel_id: &str,
    instance_id: &str,
) -> Result<&'a Channel, &'static str> {
    let channel = st.channel(channel_id).ok_or(NO_CHANNEL)?;
    if can_access(channel, instance_id) {
        Ok(channel)
    } else {
        Err(ACCESS_DENIED)
    }
}

// ── Channel management ─────────────────────────────────────────────────────────

pub(super) fn get_user_channels(st: &State, req: &Request) {
    let channels: Vec<Value> = st
        .channels
        .iter()
        .filter(|c| c.kind == ChannelKind::User)
        .map(channel_json)
        .collect();
    st.respond(
        req.from,
        req.uuid,
        "getUserChannelsResponse",
        json!({ "userChannels": channels }),
    );
}

pub(super) fn get_or_create(st: &mut State, req: &Request) {
    const RESP: &str = "getOrCreateChannelResponse";
    let id = req.payload["channelId"].as_str().unwrap_or_default();
    if id.trim().is_empty() {
        return st.respond_error(req.from, req.uuid, RESP, "InvalidArguments");
    }
    if st.channel(id).is_none() {
        st.channels.push(Channel {
            id: id.to_string(),
            kind: ChannelKind::App,
            display_name: String::new(),
            color: String::new(),
            history: Vec::new(),
            access: HashSet::new(),
        });
    }
    let channel = st.channel(id).expect("channel exists or was just added");
    if channel.kind == ChannelKind::Private {
        return st.respond_error(req.from, req.uuid, RESP, ACCESS_DENIED);
    }
    let payload = json!({ "channel": channel_json(channel) });
    st.respond(req.from, req.uuid, RESP, payload);
}

pub(super) fn get_current_channel(st: &State, req: &Request) {
    let channel = st.instances[req.from]
        .current_channel
        .as_deref()
        .and_then(|id| st.channel(id))
        .map(channel_json);
    st.respond(
        req.from,
        req.uuid,
        "getCurrentChannelResponse",
        json!({ "channel": channel }),
    );
}

pub(super) fn join(st: &mut State, req: &Request, events: &dyn BrokerEvents) {
    const RESP: &str = "joinUserChannelResponse";
    let id = req.payload["channelId"].as_str().unwrap_or_default();
    if st.channel(id).is_none_or(|c| c.kind != ChannelKind::User) {
        return st.respond_error(req.from, req.uuid, RESP, NO_CHANNEL);
    }
    set_current_channel(st, req.from, Some(id.to_string()), events);
    st.respond(req.from, req.uuid, RESP, json!({}));
}

pub(super) fn leave(st: &mut State, req: &Request, events: &dyn BrokerEvents) {
    set_current_channel(st, req.from, None, events);
    st.respond(req.from, req.uuid, "leaveCurrentChannelResponse", json!({}));
}

fn set_current_channel(
    st: &mut State,
    instance_id: &str,
    channel_id: Option<String>,
    events: &dyn BrokerEvents,
) {
    let Some(instance) = st.instances.get_mut(instance_id) else {
        return;
    };
    instance.current_channel = channel_id.clone();
    let app_id = instance.app_id.clone();

    let listening = st.event_listeners.iter().any(|l| {
        l.instance_id == instance_id
            && l.channel_id.is_none()
            && l.event_type
                .as_deref()
                .is_none_or(|t| t == "USER_CHANNEL_CHANGED")
    });
    if listening {
        st.send(
            instance_id,
            &event("channelChangedEvent", json!({ "newChannelId": channel_id })),
        );
    }
    events.emit(BrokerEvent::ChannelChanged(CdaChannelChangedEvent {
        instance_id: instance_id.to_string(),
        app_id,
        channel_id,
    }));
}

// ── Broadcast and context history ──────────────────────────────────────────────

pub(super) fn broadcast(st: &mut State, req: &Request, events: &dyn BrokerEvents) {
    const RESP: &str = "broadcastResponse";
    let channel_id = req.payload["channelId"].as_str().unwrap_or_default();
    let result = accessible(st, channel_id, req.from).map(|_| ());
    let result = result.and_then(|()| {
        publish(
            st,
            req.from,
            channel_id,
            req.payload["context"].clone(),
            &req.payload["metadata"],
            events,
        )
    });
    match result {
        Ok(_) => st.respond(req.from, req.uuid, RESP, json!({})),
        Err(e) => st.respond_error(req.from, req.uuid, RESP, e),
    }
}

/// Store `context` on the channel and deliver it to every other instance with
/// a matching listener. Each instance gets one event; the proxy fans it out to
/// its own listeners. Returns the number of instances reached.
pub(super) fn publish(
    st: &mut State,
    source: &str,
    channel_id: &str,
    context: Value,
    app_metadata: &Value,
    events: &dyn BrokerEvents,
) -> Result<usize, &'static str> {
    let ctx_type = context_type(&context).ok_or(MALFORMED_CONTEXT)?.to_string();
    st.channel(channel_id).ok_or(NO_CHANNEL)?;

    let metadata = enrich_metadata(st.app_identifier(source), app_metadata);
    let mut targets: Vec<&str> = st
        .context_listeners
        .iter()
        .filter(|l| l.instance_id != source)
        .filter(|l| match &l.channel_id {
            Some(id) => id == channel_id,
            None => {
                st.instances
                    .get(&l.instance_id)
                    .and_then(|i| i.current_channel.as_deref())
                    == Some(channel_id)
            }
        })
        .filter(|l| {
            l.context_types
                .as_ref()
                .is_none_or(|types| types.contains(&ctx_type))
        })
        .map(|l| l.instance_id.as_str())
        .collect();
    targets.sort_unstable();
    targets.dedup();

    let msg = event(
        "broadcastEvent",
        json!({ "channelId": channel_id, "context": context, "metadata": metadata }),
    );
    let reached = targets.iter().filter(|t| st.send(t, &msg)).count();

    let source_app_id = st.app_id_of(source);
    let channel = st.channel_mut(channel_id).ok_or(NO_CHANNEL)?;
    channel
        .history
        .retain(|(c, _)| context_type(c) != Some(ctx_type.as_str()));
    channel.history.insert(0, (context.clone(), metadata));

    events.emit(BrokerEvent::Context(CdaContextEvent {
        channel_id: channel_id.to_string(),
        context,
        source_instance_id: source.to_string(),
        source_app_id,
        recipient_count: reached,
    }));
    Ok(reached)
}

pub(super) fn get_current_context(st: &State, req: &Request) {
    const RESP: &str = "getCurrentContextResponse";
    let channel_id = req.payload["channelId"].as_str().unwrap_or_default();
    let channel = match accessible(st, channel_id, req.from) {
        Ok(c) => c,
        Err(e) => return st.respond_error(req.from, req.uuid, RESP, e),
    };
    let wanted = req.payload["contextType"].as_str();
    let stored = channel.history.iter().find(|(c, _)| match wanted {
        Some(t) => context_type(c) == Some(t),
        None => true,
    });
    // The schema requires context and metadata to be both set or both null.
    let (context, metadata) = match stored {
        Some((c, m)) => (c.clone(), m.clone()),
        None => (Value::Null, Value::Null),
    };
    st.respond(
        req.from,
        req.uuid,
        RESP,
        json!({ "context": context, "metadata": metadata }),
    );
}

pub(super) fn clear_context(st: &mut State, req: &Request) {
    const RESP: &str = "clearContextResponse";
    let channel_id = req.payload["channelId"].as_str().unwrap_or_default();
    if let Err(e) = accessible(st, channel_id, req.from) {
        return st.respond_error(req.from, req.uuid, RESP, e);
    }
    let cleared_type = req.payload["contextType"].as_str().map(str::to_owned);
    if let Some(channel) = st.channel_mut(channel_id) {
        match &cleared_type {
            Some(t) => channel
                .history
                .retain(|(c, _)| context_type(c) != Some(t.as_str())),
            None => channel.history.clear(),
        }
    }

    let mut targets: Vec<String> = st
        .event_listeners
        .iter()
        .filter(|l| l.instance_id != req.from)
        .filter(|l| {
            l.event_type
                .as_deref()
                .is_none_or(|t| t == "CONTEXT_CLEARED")
        })
        .filter(|l| match &l.channel_id {
            Some(id) => id == channel_id,
            None => {
                st.instances
                    .get(&l.instance_id)
                    .and_then(|i| i.current_channel.as_deref())
                    == Some(channel_id)
            }
        })
        .map(|l| l.instance_id.clone())
        .collect();
    targets.sort_unstable();
    targets.dedup();

    let msg = event(
        "contextClearedEvent",
        json!({ "channelId": channel_id, "contextType": cleared_type }),
    );
    for target in &targets {
        st.send(target, &msg);
    }
    st.respond(req.from, req.uuid, RESP, json!({}));
}

// ── Context listeners ──────────────────────────────────────────────────────────

pub(super) fn add_context_listener(st: &mut State, req: &Request) {
    const RESP: &str = "addContextListenerResponse";
    let channel_id = req.payload["channelId"].as_str().map(str::to_owned);
    if let Some(id) = &channel_id {
        if let Err(e) = accessible(st, id, req.from) {
            return st.respond_error(req.from, req.uuid, RESP, e);
        }
    }
    let array_types = string_list(req.payload.get("contextTypes"));
    let is_array = array_types.is_some();
    let context_types = array_types.or_else(|| {
        req.payload["contextType"]
            .as_str()
            .map(|t| vec![t.to_string()])
    });
    let uuid = new_uuid();
    st.context_listeners.push(ContextListener {
        uuid: uuid.clone(),
        instance_id: req.from.to_string(),
        channel_id: channel_id.clone(),
        context_types: context_types.clone(),
        is_array,
    });
    st.respond(req.from, req.uuid, RESP, json!({ "listenerUUID": uuid }));
    if let Some(id) = channel_id {
        notify_private_listeners(
            st,
            &id,
            "addContextListener",
            "privateChannelOnAddContextListenerEvent",
            Some((context_types.as_deref(), is_array)),
        );
    }
}

pub(super) fn remove_context_listener(st: &mut State, req: &Request) {
    const RESP: &str = "contextListenerUnsubscribeResponse";
    let uuid = req.payload["listenerUUID"].as_str().unwrap_or_default();
    let Some(i) = st
        .context_listeners
        .iter()
        .position(|l| l.uuid == uuid && l.instance_id == req.from)
    else {
        return st.respond_error(req.from, req.uuid, RESP, LISTENER_NOT_FOUND);
    };
    let listener = st.context_listeners.remove(i);
    st.respond(req.from, req.uuid, RESP, json!({}));
    if let Some(id) = &listener.channel_id {
        notify_private_listeners(
            st,
            id,
            "unsubscribe",
            "privateChannelOnUnsubscribeEvent",
            Some((listener.context_types.as_deref(), listener.is_array)),
        );
    }
}

// ── Agent and channel events ───────────────────────────────────────────────────

pub(super) fn add_event_listener(st: &mut State, req: &Request) {
    const RESP: &str = "addEventListenerResponse";
    let channel_id = req.payload["channelId"].as_str().map(str::to_owned);
    if let Some(id) = &channel_id {
        if let Err(e) = accessible(st, id, req.from) {
            return st.respond_error(req.from, req.uuid, RESP, e);
        }
    }
    let uuid = new_uuid();
    st.event_listeners.push(EventListener {
        uuid: uuid.clone(),
        instance_id: req.from.to_string(),
        channel_id,
        event_type: req.payload["type"].as_str().map(str::to_owned),
    });
    st.respond(req.from, req.uuid, RESP, json!({ "listenerUUID": uuid }));
}

pub(super) fn remove_event_listener(st: &mut State, req: &Request) {
    const RESP: &str = "eventListenerUnsubscribeResponse";
    let uuid = req.payload["listenerUUID"].as_str().unwrap_or_default();
    let before = st.event_listeners.len();
    st.event_listeners
        .retain(|l| !(l.uuid == uuid && l.instance_id == req.from));
    if st.event_listeners.len() == before {
        return st.respond_error(req.from, req.uuid, RESP, LISTENER_NOT_FOUND);
    }
    st.respond(req.from, req.uuid, RESP, json!({}));
}

// ── Private channels ───────────────────────────────────────────────────────────

pub(super) fn create_private(st: &mut State, req: &Request) {
    let id = new_uuid();
    st.channels.push(Channel {
        id: id.clone(),
        kind: ChannelKind::Private,
        display_name: String::new(),
        color: String::new(),
        history: Vec::new(),
        access: HashSet::from([req.from.to_string()]),
    });
    st.respond(
        req.from,
        req.uuid,
        "createPrivateChannelResponse",
        json!({ "privateChannel": { "id": id, "type": "private" } }),
    );
}

/// Give a raiser access to a private channel returned as an intent result, if
/// the handler returning it has access itself.
pub(super) fn grant_private_access(st: &mut State, channel_id: &str, handler: &str, raiser: &str) {
    if let Some(channel) = st.channel_mut(channel_id) {
        if channel.kind == ChannelKind::Private && channel.access.contains(handler) {
            channel.access.insert(raiser.to_string());
        }
    }
}

pub(super) fn add_private_event_listener(st: &mut State, req: &Request) {
    const RESP: &str = "privateChannelAddEventListenerResponse";
    let channel_id = req.payload["privateChannelId"].as_str().unwrap_or_default();
    match st.channel(channel_id) {
        Some(c) if c.kind == ChannelKind::Private => {
            if !can_access(c, req.from) {
                return st.respond_error(req.from, req.uuid, RESP, ACCESS_DENIED);
            }
        }
        _ => return st.respond_error(req.from, req.uuid, RESP, NO_CHANNEL),
    }
    let uuid = new_uuid();
    st.private_event_listeners.push(PrivateEventListener {
        uuid: uuid.clone(),
        instance_id: req.from.to_string(),
        channel_id: channel_id.to_string(),
        event_type: req.payload["listenerType"].as_str().map(str::to_owned),
    });
    st.respond(req.from, req.uuid, RESP, json!({ "listenerUUID": uuid }));
}

pub(super) fn remove_private_event_listener(st: &mut State, req: &Request) {
    const RESP: &str = "privateChannelUnsubscribeEventListenerResponse";
    let uuid = req.payload["listenerUUID"].as_str().unwrap_or_default();
    let before = st.private_event_listeners.len();
    st.private_event_listeners
        .retain(|l| !(l.uuid == uuid && l.instance_id == req.from));
    if st.private_event_listeners.len() == before {
        return st.respond_error(req.from, req.uuid, RESP, LISTENER_NOT_FOUND);
    }
    st.respond(req.from, req.uuid, RESP, json!({}));
}

pub(super) fn private_disconnect(st: &mut State, req: &Request) {
    const RESP: &str = "privateChannelDisconnectResponse";
    let channel_id = req.payload["channelId"].as_str().unwrap_or_default();
    match st.channel(channel_id) {
        Some(c) if c.kind == ChannelKind::Private => {
            if !can_access(c, req.from) {
                return st.respond_error(req.from, req.uuid, RESP, ACCESS_DENIED);
            }
        }
        _ => return st.respond_error(req.from, req.uuid, RESP, NO_CHANNEL),
    }
    leave_private_channel(st, channel_id, req.from);
    st.respond(req.from, req.uuid, RESP, json!({}));
}

/// Remove `instance_id` from a private channel: drop its listeners, then tell
/// the remaining parties about each unsubscribe, followed by the disconnect.
fn leave_private_channel(st: &mut State, channel_id: &str, instance_id: &str) {
    let mut removed = Vec::new();
    st.context_listeners.retain_mut(|l| {
        let matches = l.instance_id == instance_id && l.channel_id.as_deref() == Some(channel_id);
        if matches {
            removed.push((l.context_types.take(), l.is_array));
        }
        !matches
    });
    st.private_event_listeners
        .retain(|l| !(l.instance_id == instance_id && l.channel_id == channel_id));
    st.event_listeners
        .retain(|l| !(l.instance_id == instance_id && l.channel_id.as_deref() == Some(channel_id)));

    for (types, is_array) in &removed {
        notify_private_listeners(
            st,
            channel_id,
            "unsubscribe",
            "privateChannelOnUnsubscribeEvent",
            Some((types.as_deref(), *is_array)),
        );
    }
    notify_private_listeners(
        st,
        channel_id,
        "disconnect",
        "privateChannelOnDisconnectEvent",
        None,
    );

    if let Some(channel) = st.channel_mut(channel_id) {
        channel.access.remove(instance_id);
    }
    // Once nobody can reach a private channel, it can never be used again.
    st.channels
        .retain(|c| c.kind != ChannelKind::Private || !c.access.is_empty());
}

/// Send a private channel lifecycle event to each instance listening for it.
/// `types` is `(context types, array form)`; `None` omits the field entirely.
fn notify_private_listeners(
    st: &State,
    channel_id: &str,
    event_type: &str,
    message_type: &str,
    types: Option<(Option<&[String]>, bool)>,
) {
    if st
        .channel(channel_id)
        .is_none_or(|c| c.kind != ChannelKind::Private)
    {
        return;
    }
    let mut payload = json!({ "privateChannelId": channel_id });
    match types {
        Some((Some(list), true)) => payload["contextTypes"] = json!(list),
        Some((list, _)) => {
            payload["contextType"] = json!(list.and_then(|l| l.first()));
        }
        None => {}
    }
    let msg = json!({
        "type": message_type,
        "payload": payload,
        "meta": { "eventUuid": new_uuid(), "timestamp": iso_timestamp() },
    });
    let mut targets: Vec<&str> = st
        .private_event_listeners
        .iter()
        .filter(|l| l.channel_id == channel_id)
        .filter(|l| l.event_type.as_deref().is_none_or(|t| t == event_type))
        .map(|l| l.instance_id.as_str())
        .collect();
    targets.sort_unstable();
    targets.dedup();
    for target in targets {
        st.send(target, &msg);
    }
}

// ── Disconnect ─────────────────────────────────────────────────────────────────

pub(super) fn on_disconnect(st: &mut State, instance_id: &str) {
    let private_channels: Vec<String> = st
        .channels
        .iter()
        .filter(|c| c.kind == ChannelKind::Private && c.access.contains(instance_id))
        .map(|c| c.id.clone())
        .collect();
    for channel_id in private_channels {
        leave_private_channel(st, &channel_id, instance_id);
    }
    st.context_listeners
        .retain(|l| l.instance_id != instance_id);
    st.event_listeners.retain(|l| l.instance_id != instance_id);
    st.private_event_listeners
        .retain(|l| l.instance_id != instance_id);
}
