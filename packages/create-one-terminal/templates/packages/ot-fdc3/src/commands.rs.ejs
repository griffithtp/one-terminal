use std::sync::Arc;
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Runtime, State};

use crate::state::OtFdc3State;
use crate::types::{
    new_uuid, AppIdentifier, AppIntent, AppMetadata, CdaRequest, DisplayMetadata,
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
    Ok(AppIdentifier { app_id, instance_id })
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
        channel_type: "user".to_string(),
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

/// Stub — the CDA TCP protocol does not carry per-channel context snapshots.
#[tauri::command]
pub async fn fdc3_get_current_context(
    _channel_id: String,
    _context_type: Option<String>,
    _state: State<'_, Arc<OtFdc3State>>,
) -> Result<Option<serde_json::Value>, String> {
    Ok(None)
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

/// Stub — returns an empty app list.  Extend once the CDA supports intent resolution queries.
#[tauri::command]
pub async fn fdc3_find_intent(
    intent: String,
    _context_type: Option<String>,
    _state: State<'_, Arc<OtFdc3State>>,
) -> Result<AppIntent, String> {
    Ok(AppIntent {
        intent: IntentMetadata {
            name: intent.clone(),
            display_name: intent,
        },
        apps: vec![],
    })
}

#[tauri::command]
pub async fn fdc3_find_intents_for_context(
    _context_type: String,
    _state: State<'_, Arc<OtFdc3State>>,
) -> Result<Vec<AppIntent>, String> {
    Ok(vec![])
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
    let target_instance_id = target
        .as_ref()
        .and_then(|t| t.get("instanceId"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let request_id = new_uuid();
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.pending_intents.insert(request_id.clone(), tx);

    state
        .send_request(&CdaRequest::RaiseIntent {
            intent,
            context,
            target_instance_id,
            request_id,
        })
        .await?;

    tokio::time::timeout(std::time::Duration::from_secs(10), rx)
        .await
        .map_err(|_| "raiseIntent timed out waiting for CDA response".to_string())?
        .map_err(|_| "raiseIntent response channel dropped".to_string())?
}

/// Stub — the CDA TCP protocol does not currently support `RaiseIntentForContext`.
#[tauri::command]
pub async fn fdc3_raise_intent_for_context(
    _source_instance_id: String,
    _context: serde_json::Value,
    _target: Option<serde_json::Value>,
    _state: State<'_, Arc<OtFdc3State>>,
) -> Result<RaiseIntentResult, String> {
    Err("raiseIntentForContext is not supported over the CDA TCP transport".to_string())
}

// ── App registry ───────────────────────────────────────────────────────────────

/// Stub — returns an empty list.  Extend once the CDA exposes a registry query.
#[tauri::command]
pub async fn fdc3_get_registered_apps(
    _state: State<'_, Arc<OtFdc3State>>,
) -> Result<Vec<AppMetadata>, String> {
    Ok(vec![])
}
