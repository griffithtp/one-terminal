//! FDC3 Bus WebSocket server  —  `ws://127.0.0.1:7891/fdc3?appId=<appId>`
//!
//! Browser apps connect here via `fdc3-plugin.js`, which runs the official
//! `@finos/fdc3-agent-proxy` over this socket. A WebSocket also sidesteps the
//! same-origin restriction that keeps BroadcastChannel from working between a
//! Tauri webview (`tauri://localhost`) and ordinary browser tabs.
//!
//! Protocol
//! ────────
//! JSON text frames carrying FDC3 3.0 Desktop Agent Communication Protocol
//! messages. The first frame must be `WCP4ValidateAppIdentity`, answered by
//! `WCP5ValidateAppIdentityResponse`; `WCP6Goodbye` ends the session. The steps
//! of the Web Connection Protocol before that exist to find an agent through
//! postMessage, which doesn't apply to a direct socket.
//!
//! This module only moves frames and performs [`Effect`]s. All FDC3 behaviour
//! lives in [`crate::broker`].

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{future::BoxFuture, SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::app_dir::AppRecord;
use crate::broker::{Broker, Effect};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// What the bus needs from the application around it.
pub trait BusHost: Send + Sync + 'static {
    /// A current snapshot of the App Directory.
    fn directory(&self) -> BoxFuture<'_, Vec<AppRecord>>;
    /// Launch a new instance of `app`, returning the label of the window if it
    /// is one the agent can close later.
    fn launch(&self, app: AppRecord) -> BoxFuture<'_, Result<Option<String>, String>>;
    /// Close a window the agent opened.
    fn close_window(&self, label: &str) -> Result<(), String>;
}

#[derive(Clone)]
struct BusState {
    broker: Broker,
    host: Arc<dyn BusHost>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectParams {
    app_id: Option<String>,
}

pub fn router(broker: Broker, host: Arc<dyn BusHost>) -> Router {
    Router::new()
        .route("/fdc3", get(ws_handler))
        .with_state(BusState { broker, host })
}

pub async fn start(port: u16, broker: Broker, host: Arc<dyn BusHost>) {
    let addr = format!("127.0.0.1:{port}");
    match TcpListener::bind(&addr).await {
        Ok(listener) => {
            println!("[fdc3-bus] WebSocket server on ws://{addr}/fdc3");
            let _ = axum::serve(listener, router(broker, host)).await;
        }
        Err(e) => eprintln!("[fdc3-bus] cannot bind on {addr}: {e}"),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<ConnectParams>,
    State(state): State<BusState>,
) -> impl IntoResponse {
    let app_id = params.app_id.unwrap_or_else(|| "unknown-app".to_string());
    ws.on_upgrade(move |socket| handle_connection(socket, app_id, state))
}

async fn handle_connection(socket: WebSocket, app_id: String, state: BusState) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // ── Phase 1: identity handshake ───────────────────────────────────────────
    let instance_id = loop {
        match stream.next().await {
            Some(Ok(Message::Text(txt))) => {
                let Ok(msg) = serde_json::from_str::<Value>(&txt) else {
                    continue;
                };
                if msg["type"] != "WCP4ValidateAppIdentity" {
                    continue;
                }
                let attempt = msg["meta"]["connectionAttemptUuid"]
                    .as_str()
                    .unwrap_or_default();
                let directory = state.host.directory().await;
                // `tx` moves into the broker so that a forced disconnect drops
                // the only sender and ends the message loop below.
                break state.broker.connect(&app_id, attempt, &directory, tx);
            }
            Some(Ok(Message::Ping(d))) => {
                let _ = sink.send(Message::Pong(d)).await;
            }
            _ => return,
        }
    };
    println!("[fdc3-bus] connected: {app_id} ({instance_id})");

    // ── Phase 2: message loop ─────────────────────────────────────────────────
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.tick().await; // the first tick completes immediately
    loop {
        tokio::select! {
            inbound = stream.next() => match inbound {
                Some(Ok(Message::Text(txt))) => {
                    let Ok(msg) = serde_json::from_str::<Value>(&txt) else { continue };
                    if msg["type"] == "WCP6Goodbye" {
                        break;
                    }
                    let directory = state.host.directory().await;
                    let effects = state.broker.handle(&instance_id, &msg, &directory);
                    perform(effects, &state);
                }
                Some(Ok(Message::Ping(d))) => {
                    let _ = sink.send(Message::Pong(d)).await;
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
            outbound = rx.recv() => match outbound {
                Some(json) => {
                    if sink.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            _ = heartbeat.tick() => {
                if sink.send(Message::Text(Broker::heartbeat_event())).await.is_err() {
                    break;
                }
            }
        }
    }

    // ── Phase 3: cleanup ──────────────────────────────────────────────────────
    state.broker.disconnect(&instance_id);
    println!("[fdc3-bus] disconnected: {app_id} ({instance_id})");
}

/// Carry out broker side effects without blocking the connection's message loop.
fn perform(effects: Vec<Effect>, state: &BusState) {
    for effect in effects {
        let broker = state.broker.clone();
        let host = state.host.clone();
        match effect {
            Effect::Launch { pending_id, app } => {
                tokio::spawn(async move {
                    let result = host.launch(*app).await;
                    broker.launch_finished(&pending_id, result);
                });
            }
            Effect::ExpireAfter { pending_id, after } => {
                tokio::spawn(async move {
                    tokio::time::sleep(after).await;
                    broker.expire(&pending_id);
                });
            }
            Effect::CloseWindow {
                instance_id,
                request_uuid,
                label,
            } => {
                if let Err(e) = host.close_window(&label) {
                    eprintln!("[fdc3-bus] close failed for {label}: {e}");
                    broker.close_failed(&instance_id, &request_uuid);
                }
            }
        }
    }
}
