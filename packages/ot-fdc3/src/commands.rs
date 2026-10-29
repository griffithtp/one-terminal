use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, Runtime, State};

use crate::state::OtFdc3State;
use crate::types::{
    new_uuid, AppIdentifier, AppIntent, AppMetadata, CdaRequest, CdaResponse, DisplayMetadata,
    FdcChannel, FdcChannelJoinedEvent, IntentMetadata, RaiseIntentResult,
};

// ── Registration ───────────────────────────────────────────────────────────────

/// Register this app with the CDA.  Starts the TCP client on the first call;
/// subsequent calls return the already-assigned `instanceId` once connected.
///
/// Blocks until the CDA `Welcome` is received (up to 10 seconds).
#[tauri::command]
pub async fn fdc3_register<R: Runtime>(
    app_id: String,
    window_label: String,
    state: State<'_, Arc<OtFdc3State>>,
    app: AppHandle<R>,
) -> Result<AppIdentifier, String> {
    // Only the first caller starts the TCP client.
    if !state.started.swap(true, Ordering::SeqCst) {
        let s = Arc::clone(&*state);
        let a = app.clone();
        let id = app_id.clone();
        tauri::async_runtime::spawn(crate::client::run(s, id, a));
    }
    let _ = window_label; // single-connection model; windowLabel reserved for future use
    let instance_id = state
        .wait_for_instance_id(std::time::Duration::from_secs(10))
        .await?;
    Ok(AppIdentifier {
        app_id,
        instance_id,
    })
}

// ── Channels ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fdc3_get_system_channels(
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<Vec<FdcChannel>, String> {
    let channels = state.channels.read().await;
    Ok(channels
        .iter()
        .map(|c| FdcChannel {
            id: c.channel_id.clone(),
            channel_type: "user".to_string(),
            display_metadata: Some(DisplayMetadata {
                name: Some(c.display_name.clone()),
                color: Some(c.color.clone()),
            }),
        })
        .collect())
}

#[tauri::command]
pub async fn fdc3_get_or_create_channel(
    id: String,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<FdcChannel, String> {
    let channels = state.channels.read().await;
    let found = channels.iter().find(|c| c.channel_id == id);
    Ok(FdcChannel {
        id: id.clone(),
        channel_type: if found.is_some() { "user" } else { "app" }.to_string(),
        display_metadata: found.map(|c| DisplayMetadata {
            name: Some(c.display_name.clone()),
            color: Some(c.color.clone()),
        }),
    })
}

#[tauri::command]
pub async fn fdc3_create_private_channel(
    _state: State<'_, Arc<OtFdc3State>>,
) -> Result<FdcChannel, String> {
    Ok(FdcChannel {
        id: format!("private-{}", new_uuid()),
        channel_type: "private".to_string(),
        display_metadata: None,
    })
}

#[tauri::command]
pub async fn fdc3_join_user_channel<R: Runtime>(
    instance_id: String,
    channel_id: String,
    state: State<'_, Arc<OtFdc3State>>,
    app: AppHandle<R>,
) -> Result<(), String> {
    let _ = instance_id;
    state
        .send_request(&CdaRequest::JoinChannel {
            channel_id: channel_id.clone(),
        })
        .await?;
    *state.current_channel.write().await = Some(channel_id.clone());
    let _ = app.emit("fdc3:channel_joined", FdcChannelJoinedEvent { channel_id });
    Ok(())
}

#[tauri::command]
pub async fn fdc3_leave_current_channel<R: Runtime>(
    instance_id: String,
    state: State<'_, Arc<OtFdc3State>>,
    app: AppHandle<R>,
) -> Result<(), String> {
    let _ = instance_id;
    state.send_request(&CdaRequest::LeaveChannel).await?;
    *state.current_channel.write().await = None;
    let _ = app.emit("fdc3:channel_left", ());
    Ok(())
}

#[tauri::command]
pub async fn fdc3_get_current_channel(
    instance_id: String,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<Option<FdcChannel>, String> {
    let _ = instance_id;
    let current = state.current_channel.read().await.clone();
    match current {
        None => Ok(None),
        Some(ch_id) => {
            let channels = state.channels.read().await;
            let info = channels.iter().find(|c| c.channel_id == ch_id);
            Ok(Some(FdcChannel {
                id: ch_id,
                channel_type: "user".to_string(),
                display_metadata: info.map(|c| DisplayMetadata {
                    name: Some(c.display_name.clone()),
                    color: Some(c.color.clone()),
                }),
            }))
        }
    }
}

/// Last context broadcast on `channel_id`, from the CDA's last-value cache.
/// `context_type = None` returns the most recent context of any type.
#[tauri::command]
pub async fn fdc3_get_current_context(
    channel_id: String,
    context_type: Option<String>,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<Option<serde_json::Value>, String> {
    match state
        .query(|request_id| CdaRequest::GetCurrentContext {
            channel_id,
            context_type,
            request_id,
        })
        .await?
    {
        CdaResponse::CurrentContext { context, .. } => Ok(context),
        other => Err(format!("unexpected CDA response: {other:?}")),
    }
}

// ── Context broadcasting ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn fdc3_broadcast(
    instance_id: String,
    context: serde_json::Value,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<(), String> {
    let _ = instance_id;
    let ch = state
        .current_channel
        .read()
        .await
        .clone()
        .ok_or_else(|| "broadcast called before joining a user channel".to_string())?;
    state
        .send_request(&CdaRequest::Broadcast {
            channel_id: ch,
            context,
        })
        .await
}

// ── Intents ────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn fdc3_add_intent_listener(
    instance_id: String,
    intent: String,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<(), String> {
    let _ = instance_id;
    state
        .send_request(&CdaRequest::AddIntentListener { intent })
        .await
}

#[tauri::command]
pub async fn fdc3_remove_intent_listener(
    instance_id: String,
    intent: String,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<(), String> {
    let _ = instance_id;
    state
        .send_request(&CdaRequest::RemoveIntentListener { intent })
        .await
}

/// Apps that can handle `intent` — running instances (with `instanceId`) and
/// App Directory entries that declare `listensFor` it (without).
#[tauri::command]
pub async fn fdc3_find_intent(
    intent: String,
    context_type: Option<String>,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<AppIntent, String> {
    let mut app_intents = query_intent_apps(&state, |request_id| CdaRequest::FindIntent {
        intent: intent.clone(),
        context_type,
        request_id,
    })
    .await?;
    Ok(app_intents.pop().unwrap_or(AppIntent {
        intent: IntentMetadata {
            name: intent.clone(),
            display_name: intent,
        },
        apps: vec![],
    }))
}

/// Every intent whose App Directory declaration accepts `context_type`.
#[tauri::command]
pub async fn fdc3_find_intents_for_context(
    context_type: String,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<Vec<AppIntent>, String> {
    query_intent_apps(&state, |request_id| CdaRequest::FindIntentsForContext {
        context_type,
        request_id,
    })
    .await
}

async fn query_intent_apps(
    state: &OtFdc3State,
    build: impl FnOnce(String) -> CdaRequest,
) -> Result<Vec<AppIntent>, String> {
    match state.query(build).await? {
        CdaResponse::IntentApps { app_intents, .. } => {
            Ok(app_intents.into_iter().map(AppIntent::from).collect())
        }
        other => Err(format!("unexpected CDA response: {other:?}")),
    }
}

fn target_field<'a>(target: &'a Option<serde_json::Value>, key: &str) -> Option<&'a str> {
    target.as_ref()?.get(key)?.as_str()
}

#[tauri::command]
pub async fn fdc3_raise_intent(
    source_instance_id: String,
    intent: String,
    context: serde_json::Value,
    target: Option<serde_json::Value>,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<RaiseIntentResult, String> {
    let _ = source_instance_id;
    let target_instance_id = target_field(&target, "instanceId").map(String::from);
    raise_intent(&state, intent, context, target_instance_id).await
}

async fn raise_intent(
    state: &OtFdc3State,
    intent: String,
    context: serde_json::Value,
    target_instance_id: Option<String>,
) -> Result<RaiseIntentResult, String> {
    let request_id = new_uuid();
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.pending_intents.insert(request_id.clone(), tx);

    if let Err(e) = state
        .send_request(&CdaRequest::RaiseIntent {
            intent,
            context,
            target_instance_id,
            request_id: request_id.clone(),
        })
        .await
    {
        state.pending_intents.remove(&request_id);
        return Err(e);
    }

    let result = tokio::time::timeout(std::time::Duration::from_secs(10), rx).await;
    state.pending_intents.remove(&request_id);
    result
        .map_err(|_| "raiseIntent timed out waiting for CDA response".to_string())?
        .map_err(|_| "raiseIntent response channel dropped".to_string())?
}

/// Resolve an intent from the context type, then raise it.
///
/// Looks up intents that accept `context.type` (narrowed to `target` when
/// given). One matching intent is raised directly; several are returned as
/// `needsResolution` so the caller can pick one and call `raiseIntent`.
#[tauri::command]
pub async fn fdc3_raise_intent_for_context(
    source_instance_id: String,
    context: serde_json::Value,
    target: Option<serde_json::Value>,
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<RaiseIntentResult, String> {
    let _ = source_instance_id;
    let context_type = context
        .get("type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "MalformedContext: context has no type".to_string())?
        .to_string();

    let mut app_intents =
        query_intent_apps(&state, |request_id| CdaRequest::FindIntentsForContext {
            context_type,
            request_id,
        })
        .await?;

    let target_app_id = target_field(&target, "appId");
    let target_instance_id = target_field(&target, "instanceId");
    if target.is_some() {
        for ai in &mut app_intents {
            ai.apps.retain(|a| match target_instance_id {
                Some(iid) => a.instance_id.as_deref() == Some(iid),
                None => Some(a.app_id.as_str()) == target_app_id,
            });
        }
        app_intents.retain(|ai| !ai.apps.is_empty());
    }

    match app_intents.len() {
        0 => Err("NoAppsFound".to_string()),
        1 => {
            let ai = app_intents.remove(0);
            // Deliver to a specific instance only when it is unambiguous;
            // otherwise let the CDA's own resolution pick the handler.
            let running: Vec<&str> = ai
                .apps
                .iter()
                .filter_map(|a| a.instance_id.as_deref())
                .collect();
            let instance = match (target_instance_id, running.as_slice()) {
                (Some(iid), _) => Some(iid.to_string()),
                (None, [only]) if target.is_some() => Some(only.to_string()),
                _ => None,
            };
            raise_intent(&state, ai.intent.name, context, instance).await
        }
        _ => Ok(RaiseIntentResult::NeedsResolution(app_intents)),
    }
}

// ── App registry ───────────────────────────────────────────────────────────────

/// All spoke instances currently connected to the CDA (TCP and WS bus).
#[tauri::command]
pub async fn fdc3_get_registered_apps(
    state: State<'_, Arc<OtFdc3State>>,
) -> Result<Vec<AppMetadata>, String> {
    match state
        .query(|request_id| CdaRequest::GetRegisteredApps { request_id })
        .await?
    {
        CdaResponse::RegisteredApps { apps, .. } => {
            Ok(apps.into_iter().map(AppMetadata::from).collect())
        }
        other => Err(format!("unexpected CDA response: {other:?}")),
    }
}
