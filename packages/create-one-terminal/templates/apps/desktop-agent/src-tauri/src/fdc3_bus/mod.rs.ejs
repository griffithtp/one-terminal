//! FDC3 Bus WebSocket server  —  `ws://127.0.0.1:7891/fdc3`
//!
//! Browser apps (ticker-plant, chart-viewer, …) connect here via
//! `fdc3-plugin.js` using the native WebSocket API.  This sidesteps the
//! same-origin restriction that prevents BroadcastChannel from working
//! between a Tauri webview (`tauri://localhost`) and ordinary browser tabs.
//!
//! Protocol
//! ────────
//! Both directions use JSON text frames.
//!
//! Inbound  (browser → server):  `fdc3:*` types, camelCase fields
//! Outbound (server → browser):  `fdc3:*` types, camelCase fields
//!
//! Bus clients are registered in `WindowManager` with the same `mpsc`
//! infrastructure as TCP spokes, so the broker's `fan_out_broadcast` and
//! `deliver_intent` functions route to them automatically.  The outbound
//! loop converts the `CdaResponse` JSON (snake_case) to `fdc3:*` JSON
//! (camelCase) before writing to the WebSocket sink.

use axum::{
    Router,
    extract::{
        State,
        WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    response::IntoResponse,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::app_dir::AppDirectoryCache;
use crate::broker::{
    CdaConnectedEvent, CdaDisconnectedEvent,
    ChannelManager, IntentRegistry, WindowManager, now_ms,
};
use crate::dacp::PeerRegistry;
use crate::tcp::handler::{fan_out_broadcast, deliver_intent};

// ── Axum shared state ─────────────────────────────────────────────────────────

#[derive(Clone)]
struct BusState {
    window_manager: WindowManager,
    channel_manager: ChannelManager,
    intent_registry: IntentRegistry,
    peer_registry: PeerRegistry,
    app_dir_cache: AppDirectoryCache,
    app: AppHandle,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn start(
    port: u16,
    window_manager: WindowManager,
    channel_manager: ChannelManager,
    intent_registry: IntentRegistry,
    peer_registry: PeerRegistry,
    app_dir_cache: AppDirectoryCache,
    app: AppHandle,
) {
    let state = BusState {
        window_manager,
        channel_manager,
        intent_registry,
        peer_registry,
        app_dir_cache,
        app,
    };

    let router = Router::new()
        .route("/fdc3", get(ws_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    match TcpListener::bind(&addr).await {
        Ok(listener) => {
            println!("[fdc3-bus] WebSocket server on ws://{addr}/fdc3");
            let _ = axum::serve(listener, router).await;
        }
        Err(e) => eprintln!("[fdc3-bus] cannot bind on {addr}: {e}"),
    }
}

// ── WebSocket upgrade handler ─────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<BusState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_connection(socket, state))
}

// ── Per-connection coroutine ──────────────────────────────────────────────────

async fn handle_connection(socket: WebSocket, state: BusState) {
    let (mut sink, mut stream) = socket.split();

    // ── Phase 1: wait for fdc3:hello ─────────────────────────────────────────
    let (instance_id, app_id, hello_request_id, mut rx) = loop {
        match stream.next().await {
            Some(Ok(Message::Text(txt))) => {
                let Ok(v) = serde_json::from_str::<Value>(&txt) else { continue };
                if v.get("type").and_then(|t| t.as_str()) != Some("fdc3:hello") {
                    continue;
                }
                let app_id = v
                    .get("appId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let request_id = v
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let (tx, rx) = mpsc::unbounded_channel::<String>();
                let iid = state.window_manager.register(app_id.clone(), None, tx);
                break (iid, app_id, request_id, rx);
            }
            Some(Ok(Message::Ping(d))) => {
                let _ = sink.send(Message::Pong(d)).await;
            }
            _ => return,
        }
    };

    // Send fdc3:welcome  (channels serialise camelCase via ChannelInfoSummary)
    let channels =
        serde_json::to_value(state.channel_manager.list_summaries()).unwrap_or(Value::Array(vec![]));
    let welcome = json!({
        "type":       "fdc3:welcome",
        "instanceId": instance_id,
        "channels":   channels,
        "requestId":  hello_request_id,
    });
    if sink
        .send(Message::Text(serde_json::to_string(&welcome).unwrap_or_default()))
        .await
        .is_err()
    {
        state.window_manager.unregister(&instance_id);
        return;
    }

    let _ = state.app.emit(
        "cda:connected",
        CdaConnectedEvent {
            instance_id:  instance_id.clone(),
            app_id:       app_id.clone(),
            display_name: None,
            connected_at: now_ms(),
        },
    );
    println!("[fdc3-bus] connected: {app_id} ({instance_id})");

    // ── Phase 2: message loop ─────────────────────────────────────────────────
    loop {
        tokio::select! {
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(txt))) => {
                    let Ok(v) = serde_json::from_str::<Value>(&txt) else { continue };
                    let Some(msg_type) = v.get("type").and_then(|t| t.as_str()).map(str::to_owned)
                        else { continue };

                    match msg_type.as_str() {

                        // ── Broadcast ─────────────────────────────────────
                        "fdc3:broadcast" => {
                            let channel_id = str_field(&v, "channelId");
                            let context    = v.get("context").cloned().unwrap_or(Value::Null);
                            if !channel_id.is_empty() {
                                fan_out_broadcast(
                                    &instance_id,
                                    &app_id,
                                    &channel_id,
                                    context,
                                    &state.window_manager,
                                    &state.channel_manager,
                                    &state.peer_registry,
                                    &state.app,
                                );
                            }
                        }

                        // ── RaiseIntent ───────────────────────────────────
                        "fdc3:raiseIntent" => {
                            let intent     = str_field(&v, "intent");
                            let context    = v.get("context").cloned().unwrap_or(Value::Null);
                            let target     = v.get("targetInstanceId")
                                .and_then(|t| t.as_str())
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned);
                            let request_id = str_field(&v, "requestId");
                            deliver_intent(
                                &instance_id,
                                &intent,
                                context,
                                target.as_deref(),
                                &request_id,
                                &state.window_manager,
                                &state.intent_registry,
                                &state.peer_registry,
                                &state.app_dir_cache,
                                &state.app,
                            ).await;
                        }

                        // ── JoinChannel ───────────────────────────────────
                        "fdc3:joinChannel" => {
                            let channel_id = str_field(&v, "channelId");
                            let request_id = str_field(&v, "requestId");
                            match state.channel_manager.join(&instance_id, &channel_id) {
                                Ok(()) => {
                                    state.window_manager.set_channel(
                                        &instance_id,
                                        Some(channel_id.clone()),
                                    );
                                    let reply = serde_json::to_string(&json!({
                                        "type":      "fdc3:channelJoined",
                                        "channelId": channel_id,
                                        "requestId": request_id,
                                    }))
                                    .unwrap_or_default();
                                    state.window_manager.send_to(&instance_id, &reply);
                                }
                                Err(msg) => {
                                    let err = serde_json::to_string(&json!({
                                        "type":      "fdc3:error",
                                        "code":      "CHANNEL_NOT_FOUND",
                                        "message":   msg,
                                        "requestId": request_id,
                                    }))
                                    .unwrap_or_default();
                                    state.window_manager.send_to(&instance_id, &err);
                                }
                            }
                        }

                        // ── LeaveChannel ──────────────────────────────────
                        "fdc3:leaveChannel" => {
                            state.channel_manager.leave(&instance_id);
                            state.window_manager.set_channel(&instance_id, None);
                        }

                        // ── IntentListener management ─────────────────────
                        "fdc3:addIntentListener" => {
                            let intent = str_field(&v, "intent");
                            if !intent.is_empty() {
                                state.intent_registry.add(&instance_id, &intent);
                                println!("[fdc3-bus] {app_id} ({instance_id}) +intent: {intent}");
                            }
                        }
                        "fdc3:removeIntentListener" => {
                            let intent = str_field(&v, "intent");
                            if !intent.is_empty() {
                                state.intent_registry.remove(&instance_id, &intent);
                            }
                        }

                        // ── Open app ──────────────────────────────────────
                        "fdc3:open" => {
                            let target_app_id = str_field(&v, "appId");
                            let request_id    = str_field(&v, "requestId");
                            let apps = state.app_dir_cache.list_all().await;
                            match apps.iter().find(|a| a.app_id == target_app_id) {
                                Some(rec) => {
                                    let url = rec
                                        .details
                                        .as_ref()
                                        .and_then(|d| d.url.as_deref())
                                        .unwrap_or("")
                                        .to_string();
                                    let reply = serde_json::to_string(&json!({
                                        "type":      "fdc3:openResult",
                                        "appId":     target_app_id,
                                        "requestId": request_id,
                                    }))
                                    .unwrap_or_default();
                                    state.window_manager.send_to(&instance_id, &reply);
                                    if !url.is_empty() {
                                        if rec.app_type == "onlineNative" {
                                            let label = format!(
                                                "app-{}",
                                                rec.app_id.replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
                                            );
                                            if let Some(win) = state.app.get_webview_window(&label) {
                                                let _: Result<(), _> = win.set_focus();
                                            } else if let Ok(parsed) = url.parse::<tauri::Url>() {
                                                let title = rec.title.clone()
                                                    .unwrap_or_else(|| rec.name.clone());
                                                let _ = tauri::WebviewWindowBuilder::new(
                                                    &state.app,
                                                    label,
                                                    tauri::WebviewUrl::External(parsed),
                                                )
                                                .title(title)
                                                .inner_size(1280.0, 800.0)
                                                .build();
                                            }
                                        } else {
                                            let _ = tauri_plugin_opener::open_url(&url, None::<&str>);
                                        }
                                    }
                                }
                                None => {
                                    let err = serde_json::to_string(&json!({
                                        "type":      "fdc3:error",
                                        "code":      "APP_NOT_FOUND",
                                        "message":   format!("App '{target_app_id}' not in directory"),
                                        "requestId": request_id,
                                    }))
                                    .unwrap_or_default();
                                    state.window_manager.send_to(&instance_id, &err);
                                }
                            }
                        }

                        // ── Goodbye ───────────────────────────────────────
                        "fdc3:goodbye" => break,

                        _ => {}
                    }
                }
                Some(Ok(Message::Ping(d))) => {
                    let _ = sink.send(Message::Pong(d)).await;
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
            outbound = rx.recv() => match outbound {
                Some(cda_json) => {
                    if let Some(fdc3_json) = cda_to_fdc3(&cda_json) {
                        if sink.send(Message::Text(fdc3_json)).await.is_err() {
                            break;
                        }
                    }
                }
                None => break,
            },
        }
    }

    // ── Phase 3: cleanup ─────────────────────────────────────────────────────
    state.intent_registry.remove_all(&instance_id);
    state.channel_manager.remove_member(&instance_id);
    state.window_manager.unregister(&instance_id);
    let _ = state.app.emit(
        "cda:disconnected",
        CdaDisconnectedEvent {
            instance_id: instance_id.clone(),
            app_id:      app_id.clone(),
        },
    );
    println!("[fdc3-bus] disconnected: {app_id} ({instance_id})");
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Extract a string field from a JSON object, returning "" if absent.
fn str_field(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// Convert a `CdaResponse` JSON string (snake_case fields, snake_case type tag)
/// to the corresponding `fdc3:*` JSON string (camelCase fields) expected by
/// `fdc3-plugin.js`.
///
/// Messages already carrying an `fdc3:` prefix (e.g. fdc3:channelJoined
/// injected by the joinChannel handler) are passed through unchanged.
///
/// Returns `None` for CdaResponse types that don't need forwarding to the
/// browser (pong, welcome, channel_left).
fn cda_to_fdc3(s: &str) -> Option<String> {
    let v: Value = serde_json::from_str(s).ok()?;
    let msg_type = v.get("type")?.as_str()?;

    // Pass through messages already in fdc3: format.
    if msg_type.starts_with("fdc3:") {
        return Some(s.to_string());
    }

    let out: Value = match msg_type {
        "context_broadcast" => json!({
            "type":             "fdc3:contextBroadcast",
            "channelId":        v["channel_id"],
            "context":          v["context"],
            "sourceInstanceId": v["source_instance_id"],
            "sourceAppId":      v["source_app_id"],
        }),
        "intent_delivery" => json!({
            "type":             "fdc3:intentDelivery",
            "intent":           v["intent"],
            "context":          v["context"],
            "sourceInstanceId": v["source_instance_id"],
            "targetInstanceId": Value::Null,
            "deliveryId":       v["request_id"],
        }),
        "intent_resolved" => json!({
            "type":               "fdc3:intentResolved",
            "intent":             v["intent"],
            "handlerInstanceId":  v["handler_instance_id"],
            "handlerAppId":       v["handler_app_id"],
            "requestId":          v["request_id"],
        }),
        "error" => {
            let mut out = json!({
                "type":    "fdc3:error",
                "code":    v["code"],
                "message": v["message"],
            });
            if let Some(rid) = v.get("request_id") {
                out["requestId"] = rid.clone();
            }
            out
        }
        // welcome, channel_joined, channel_left, pong are handled directly
        // or not needed — skip them.
        _ => return None,
    };

    serde_json::to_string(&out).ok()
}
