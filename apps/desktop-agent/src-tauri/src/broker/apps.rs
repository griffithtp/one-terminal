//! `open`, `close`, instance lookup and agent metadata.

use serde_json::{json, Value};

use super::{
    context_type, enrich_metadata, event, new_uuid, Effect, OpenState, PendingOpen, Request, State,
    APP_LAUNCH_TIMEOUT, FDC3_VERSION, PROVIDER,
};
use crate::app_dir::AppRecord;

const OPEN_RESP: &str = "openResponse";

/// FDC3 `AppMetadata` for an app, enriched from the directory when it has a record.
fn app_metadata(app_id: &str, instance_id: Option<&str>, directory: &[AppRecord]) -> Value {
    let mut out = json!({ "appId": app_id });
    if let Some(id) = instance_id {
        out["instanceId"] = json!(id);
    }
    if let Some(app) = directory.iter().find(|a| a.app_id == app_id) {
        out["title"] = json!(app.display_title());
        for (key, value) in [
            ("version", app.version.as_ref()),
            ("description", app.description.as_ref()),
            ("tooltip", app.tooltip.as_ref()),
        ] {
            if let Some(v) = value {
                out[key] = json!(v);
            }
        }
        for (key, value) in [
            ("icons", app.icons.as_ref()),
            ("screenshots", app.screenshots.as_ref()),
        ] {
            if let Some(v) = value {
                out[key] = v.clone();
            }
        }
    }
    out
}

pub(super) fn implementation_metadata(
    app_id: &str,
    instance_id: &str,
    directory: &[AppRecord],
) -> Value {
    json!({
        "fdc3Version": FDC3_VERSION,
        "provider": PROVIDER,
        "providerVersion": env!("CARGO_PKG_VERSION"),
        "optionalFeatures": {
            "DesktopAgentBridging": false,
            "UserChannelMembershipAPIs": true,
        },
        "appMetadata": app_metadata(app_id, Some(instance_id), directory),
    })
}

pub(super) fn get_info(st: &State, req: &Request, directory: &[AppRecord]) {
    let metadata = implementation_metadata(&st.app_id_of(req.from), req.from, directory);
    st.respond(
        req.from,
        req.uuid,
        "getInfoResponse",
        json!({ "implementationMetadata": metadata }),
    );
}

pub(super) fn get_app_metadata(st: &State, req: &Request, directory: &[AppRecord]) {
    const RESP: &str = "getAppMetadataResponse";
    let app_id = req.payload["app"]["appId"].as_str().unwrap_or_default();
    let instance_id = req.payload["app"]["instanceId"].as_str();
    let known = directory.iter().any(|a| a.app_id == app_id)
        || st.instances.values().any(|i| i.app_id == app_id);
    let instance_ok =
        instance_id.is_none_or(|id| st.instances.get(id).is_some_and(|i| i.app_id == app_id));
    if !known || !instance_ok {
        return st.respond_error(req.from, req.uuid, RESP, "TargetAppUnavailable");
    }
    st.respond(
        req.from,
        req.uuid,
        RESP,
        json!({ "appMetadata": app_metadata(app_id, instance_id, directory) }),
    );
}

pub(super) fn find_instances(st: &State, req: &Request) {
    let app_id = req.payload["app"]["appId"].as_str().unwrap_or_default();
    let mut instances: Vec<Value> = st
        .instances
        .iter()
        .filter(|(_, i)| i.app_id == app_id)
        .map(|(id, i)| json!({ "appId": i.app_id, "instanceId": id }))
        .collect();
    instances.sort_by_key(|v| v["instanceId"].as_str().map(str::to_owned));
    st.respond(
        req.from,
        req.uuid,
        "findInstancesResponse",
        json!({ "appIdentifiers": instances }),
    );
}

// ── open ───────────────────────────────────────────────────────────────────────

pub(super) fn open(st: &mut State, req: &Request, directory: &[AppRecord]) -> Vec<Effect> {
    let app_id = req.payload["app"]["appId"].as_str().unwrap_or_default();
    let Some(app) = directory.iter().find(|a| a.app_id == app_id).cloned() else {
        st.respond_error(req.from, req.uuid, OPEN_RESP, "AppNotFound");
        return Vec::new();
    };
    let context = req.payload.get("context").filter(|c| !c.is_null()).cloned();
    if context.as_ref().is_some_and(|c| context_type(c).is_none()) {
        st.respond_error(req.from, req.uuid, OPEN_RESP, "MalformedContext");
        return Vec::new();
    }
    let pending_id = new_uuid();
    st.pending_opens.push(PendingOpen {
        id: pending_id.clone(),
        request_uuid: req.uuid.to_string(),
        from: req.from.to_string(),
        app_id: app_id.to_string(),
        context,
        metadata: req.payload["metadata"].clone(),
        state: OpenState::Opening,
        instance_id: None,
    });
    vec![
        Effect::Launch {
            pending_id: pending_id.clone(),
            app: Box::new(app),
        },
        Effect::ExpireAfter {
            pending_id,
            after: APP_LAUNCH_TIMEOUT,
        },
    ]
}

/// A new instance connected: it may be the app an `open` is waiting for. With
/// no context to deliver, connecting is all `open` needs to succeed.
pub(super) fn on_app_connected(st: &mut State, app_id: &str, instance_id: &str) {
    let Some(i) = st
        .pending_opens
        .iter()
        .position(|p| p.app_id == app_id && p.state == OpenState::Opening)
    else {
        return;
    };
    if st.pending_opens[i].context.is_some() {
        st.pending_opens[i].state = OpenState::DeliveringContext;
        st.pending_opens[i].instance_id = Some(instance_id.to_string());
    } else {
        let pending = st.pending_opens.remove(i);
        respond_opened(st, &pending, instance_id);
    }
}

fn respond_opened(st: &State, pending: &PendingOpen, instance_id: &str) {
    st.respond(
        &pending.from,
        &pending.request_uuid,
        OPEN_RESP,
        json!({ "appIdentifier": st.app_identifier(instance_id) }),
    );
}

/// Deliver an `open` context to the opened app's first matching listener.
/// Runs after the listener's response is queued, so the proxy has registered
/// the listener by the time this event arrives.
pub(super) fn deliver_open_context(st: &mut State, instance_id: &str, payload: &Value) {
    // Channel-scoped listeners don't receive open context.
    if payload["channelId"].is_string() {
        return;
    }
    let matches = |pending: &PendingOpen| {
        let Some(ctx_type) = pending.context.as_ref().and_then(context_type) else {
            return false;
        };
        if let Some(types) = payload.get("contextTypes").and_then(Value::as_array) {
            return types.iter().any(|t| t.as_str() == Some(ctx_type));
        }
        payload["contextType"]
            .as_str()
            .is_none_or(|t| t == ctx_type)
    };
    let Some(i) = st.pending_opens.iter().position(|p| {
        p.state == OpenState::DeliveringContext
            && p.instance_id.as_deref() == Some(instance_id)
            && matches(p)
    }) else {
        return;
    };
    let pending = st.pending_opens.remove(i);
    let metadata = enrich_metadata(st.app_identifier(&pending.from), &pending.metadata);
    st.send(
        instance_id,
        &event(
            "broadcastEvent",
            json!({ "channelId": null, "context": pending.context, "metadata": metadata }),
        ),
    );
    respond_opened(st, &pending, instance_id);
}

pub(super) fn fail_open(st: &mut State, pending_id: &str, error: &str) {
    if let Some(i) = st.pending_opens.iter().position(|p| p.id == pending_id) {
        let pending = st.pending_opens.remove(i);
        st.respond_error(&pending.from, &pending.request_uuid, OPEN_RESP, error);
    }
}

/// `ApiTimeout` means the app never initialized FDC3; `AppTimeout` means it did
/// but never added a listener for the context.
pub(super) fn expire_open(st: &mut State, pending_id: &str) {
    let error = match st.pending_opens.iter().find(|p| p.id == pending_id) {
        Some(p) if p.state == OpenState::Opening => "ApiTimeout",
        Some(_) => "AppTimeout",
        None => return,
    };
    fail_open(st, pending_id, error);
}

// ── close ──────────────────────────────────────────────────────────────────────

/// `fdc3.close()` closes the caller. The agent can only close windows it opened
/// itself: Terminal panels and processes spawned on separate hosts reject with
/// `ErrorOnClose`. On success no response is sent, as the app no longer exists.
pub(super) fn close(st: &State, req: &Request) -> Vec<Effect> {
    match st.instances[req.from].window_label.clone() {
        Some(label) => vec![Effect::CloseWindow {
            instance_id: req.from.to_string(),
            request_uuid: req.uuid.to_string(),
            label,
        }],
        None => {
            st.respond_error(req.from, req.uuid, "closeResponse", "ErrorOnClose");
            Vec::new()
        }
    }
}
