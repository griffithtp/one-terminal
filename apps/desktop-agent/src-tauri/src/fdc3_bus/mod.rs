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
//! lives in [`crate::broker`]; spans and metrics live in [`instrument`].

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Query, State, WebSocketUpgrade,
    },
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{future::BoxFuture, SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::Instrument;

use crate::app_dir::AppRecord;
use crate::broker::{Broker, Effect};

pub mod admission;
mod instrument;
use instrument::{BusMetrics, HandshakeEnd};

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
    metrics: Arc<BusMetrics>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectParams {
    app_id: Option<String>,
}

pub fn router(broker: Broker, host: Arc<dyn BusHost>) -> Router {
    Router::new()
        .route("/fdc3", get(ws_handler))
        .with_state(BusState {
            broker,
            host,
            metrics: Arc::new(BusMetrics::new(&ot_telemetry::metrics::meter(
                "desktop-agent",
            ))),
        })
}

pub async fn start(port: u16, broker: Broker, host: Arc<dyn BusHost>) {
    let addr = format!("127.0.0.1:{port}");
    match TcpListener::bind(&addr).await {
        Ok(listener) => {
            tracing::info!("WebSocket server on ws://{addr}/fdc3");
            let _ = axum::serve(listener, router(broker, host)).await;
        }
        Err(e) => tracing::error!("cannot bind on {addr}: {e}"),
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    Query(params): Query<ConnectParams>,
    State(state): State<BusState>,
) -> axum::response::Response {
    let app_id = params.app_id.unwrap_or_else(|| "unknown-app".to_string());
    // Bind the claimed app id to the page's Origin before the socket opens:
    // see `admission` for the rule.
    let origin = headers
        .get(header::ORIGIN)
        .map(|v| v.to_str().unwrap_or("<non-ascii>"));
    let directory = state.host.directory().await;
    match admission::admit(origin, &app_id, &directory) {
        Ok(_) => ws
            .on_upgrade(move |socket| handle_connection(socket, app_id, state))
            .into_response(),
        Err(refused) => {
            state.metrics.handshake_failed(HandshakeEnd::Refused);
            tracing::warn!(
                app_id = %crate::broker::clip_untrusted(&app_id),
                origin = %crate::broker::clip_untrusted(origin.unwrap_or_default()),
                "refused bus connection: {refused}"
            );
            (StatusCode::FORBIDDEN, refused.to_string()).into_response()
        }
    }
}

async fn handle_connection(socket: WebSocket, app_id: String, state: BusState) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // ── Phase 1: identity handshake ───────────────────────────────────────────
    // Opened on the first text frame, not on accept: a page opening and
    // closing sockets in a loop must not flood the trace backend (the
    // handshake-failure counter still counts it).
    let mut connect_span: Option<tracing::Span> = None;
    let ended = |span: &Option<tracing::Span>, outcome: &str| {
        if let Some(span) = span {
            instrument::handshake_ended(span, outcome);
        }
    };
    let instance_id = loop {
        match stream.next().await {
            Some(Ok(Message::Text(txt))) => {
                let span = connect_span.get_or_insert_with(|| instrument::connect_span(&app_id));
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
                break span.in_scope(|| state.broker.connect(&app_id, attempt, &directory, tx));
            }
            Some(Ok(Message::Ping(d))) => {
                let _ = sink.send(Message::Pong(d)).await;
            }
            Some(Ok(Message::Close(_))) | None => {
                ended(&connect_span, "closed");
                state.metrics.handshake_failed(HandshakeEnd::Closed);
                return;
            }
            _ => {
                ended(&connect_span, "invalid");
                state.metrics.handshake_failed(HandshakeEnd::Invalid);
                return;
            }
        }
    };
    if let Some(span) = connect_span.take() {
        span.record("ot.instance_id", instance_id.as_str());
        instrument::handshake_ended(&span, "ok");
    }
    state.metrics.connected();
    tracing::info!(
        app_id = %crate::broker::clip_untrusted(&app_id),
        instance_id = %instance_id,
        "connected"
    );

    // ── Phase 2: message loop ─────────────────────────────────────────────────
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.tick().await; // the first tick completes immediately
    loop {
        tokio::select! {
            inbound = stream.next() => match inbound {
                Some(Ok(Message::Text(txt))) => {
                    let Ok(mut msg) = serde_json::from_str::<Value>(&txt) else { continue };
                    if msg["type"] == "WCP6Goodbye" {
                        break;
                    }
                    let directory = state.host.directory().await;
                    let label = instrument::message_label(msg["type"].as_str().unwrap_or_default());
                    match label {
                        Some(label) => {
                            let span = instrument::request_span(label, &msg, &instance_id);
                            instrument::link_trace(&span, &mut msg);
                            let started = Instant::now();
                            // Synchronous: no guard is held across an await.
                            let outcome = span.in_scope(|| {
                                let (effects, outcome) =
                                    state.broker.handle_with_outcome(&instance_id, &msg, &directory);
                                perform(effects, &state);
                                outcome
                            });
                            state.metrics.handled(label, outcome, started);
                        }
                        None => {
                            let effects = state.broker.handle(&instance_id, &msg, &directory);
                            perform(effects, &state);
                        }
                    }
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
    state.metrics.disconnected();
    tracing::info!(
        app_id = %crate::broker::clip_untrusted(&app_id),
        instance_id = %instance_id,
        "disconnected"
    );
}

/// Carry out broker side effects without blocking the connection's message loop.
fn perform(effects: Vec<Effect>, state: &BusState) {
    for effect in effects {
        let broker = state.broker.clone();
        let host = Arc::clone(&state.host);
        match effect {
            Effect::Launch { pending_id, app } => {
                // A child of the current request span; the launched process
                // gets it as TRACEPARENT, so the launch is one trace.
                let span = tracing::info_span!(
                    target: ot_telemetry::SPAN_TARGET,
                    "fdc3.launch",
                    app_id = %crate::broker::clip_untrusted(&app.app_id)
                );
                tokio::spawn(
                    async move {
                        let result = host.launch(*app).await;
                        broker.launch_finished(&pending_id, result);
                    }
                    .instrument(span),
                );
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
                    tracing::warn!("close failed for {label}: {e}");
                    broker.close_failed(&instance_id, &request_uuid);
                }
            }
        }
    }
}

#[cfg(test)]
mod upgrade_tests {
    use super::*;
    use crate::app_dir::AppDetails;
    use crate::broker::{BrokerEvent, BrokerEvents};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct NoEvents;
    impl BrokerEvents for NoEvents {
        fn emit(&self, _event: BrokerEvent) {}
    }

    struct Host(Vec<AppRecord>);
    impl BusHost for Host {
        fn directory(&self) -> BoxFuture<'_, Vec<AppRecord>> {
            Box::pin(async move { self.0.clone() })
        }
        fn launch(&self, _app: AppRecord) -> BoxFuture<'_, Result<Option<String>, String>> {
            Box::pin(async { Ok(None) })
        }
        fn close_window(&self, _label: &str) -> Result<(), String> {
            Ok(())
        }
    }

    async fn serve() -> std::net::SocketAddr {
        let host = Host(vec![AppRecord {
            app_id: "blotter".into(),
            details: Some(AppDetails {
                url: Some("https://apps.example.com/blotter/".into()),
            }),
            ..Default::default()
        }]);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let app = router(Broker::new(Arc::new(NoEvents)), Arc::new(host));
        tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        addr
    }

    /// Send a WebSocket upgrade request and return the response status code.
    async fn upgrade_status(addr: std::net::SocketAddr, app_id: &str, origin: Option<&str>) -> u16 {
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        let origin = origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "GET /fdc3?appId={app_id} HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n{origin}\r\n"
        );
        s.write_all(req.as_bytes()).await.unwrap();
        let mut buf = [0u8; 64];
        let n = s.read(&mut buf).await.unwrap();
        let head = String::from_utf8_lossy(&buf[..n]);
        head.split_whitespace().nth(1).unwrap().parse().unwrap()
    }

    #[tokio::test]
    async fn the_bus_refuses_unknown_origins_before_the_upgrade() {
        let addr = serve().await;
        // Non-browser and loopback clients: as before.
        assert_eq!(upgrade_status(addr, "anything", None).await, 101);
        assert_eq!(
            upgrade_status(addr, "anything", Some("http://localhost:3010")).await,
            101
        );
        // A registered origin, as its own app.
        assert_eq!(
            upgrade_status(addr, "blotter", Some("https://apps.example.com")).await,
            101
        );
        // A random web page, a registered origin claiming another id, an
        // opaque origin: refused.
        for (id, origin) in [
            ("blotter", "https://evil.example"),
            ("someone-else", "https://apps.example.com"),
            ("blotter", "null"),
        ] {
            assert_eq!(
                upgrade_status(addr, id, Some(origin)).await,
                403,
                "{id} {origin}"
            );
        }
    }
}
