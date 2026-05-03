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
use tauri::{AppHandle, Emitter};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::peer::handle_inbound_bmp;
use super::registry::{PeerHandle, PeerRegistry};
use super::types::{
    BmpMessage, BmpMeta, CdaPeerConnectedEvent, CdaPeerDisconnectedEvent,
    new_uuid, now_ms_str,
};

// ── Axum state ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct ServerState {
    registry: PeerRegistry,
    app: AppHandle,
}

// ── Entry point ────────────────────────────────────────────────────────────────

/// Bind an axum WebSocket server on `127.0.0.1:{port}/v2/bridge` to accept
/// inbound connections from external FDC3 2.2 desktop agents.
pub async fn start(registry: PeerRegistry, app: AppHandle, port: u16) {
    let state = ServerState { registry, app };
    let router = Router::new()
        .route("/v2/bridge", get(ws_handler))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    match TcpListener::bind(&addr).await {
        Ok(listener) => {
            println!("[dacp] bridge WS server listening on ws://{addr}/v2/bridge");
            let _ = axum::serve(listener, router).await;
        }
        Err(e) => eprintln!("[dacp] cannot bind WS server on {addr}: {e}"),
    }
}

// ── WebSocket upgrade handler ─────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<ServerState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| accept_connection(socket, state))
}

// ── Per-connection coroutine ──────────────────────────────────────────────────

async fn accept_connection(socket: WebSocket, state: ServerState) {
    let (mut sink, mut stream) = socket.split();

    // ── Phase 1: wait for WCPHello ────────────────────────────────────────────
    let (app_id, hello_uuid) = loop {
        match stream.next().await {
            Some(Ok(Message::Text(txt))) => {
                match serde_json::from_str::<BmpMessage>(&txt) {
                    Ok(msg) if msg.msg_type == "WCPHello" => {
                        let aid = msg
                            .payload
                            .get("desktopAgentDetails")
                            .and_then(|d| d.get("id"))
                            .and_then(|id| id.get("appId"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        break (aid, msg.meta.request_uuid);
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        eprintln!("[dacp] server: handshake parse error: {e}");
                        return;
                    }
                }
            }
            _ => return,
        }
    };

    // Assign an agent ID for this inbound connection.
    let desktop_agent_id = format!("peer-{}", new_uuid());

    // ── Send WCPHelloResponse ─────────────────────────────────────────────────
    let response = BmpMessage {
        msg_type: "WCPHelloResponse".to_string(),
        meta: BmpMeta {
            request_uuid: new_uuid(),
            response_uuid: Some(hello_uuid),
            timestamp: now_ms_str(),
            source: None,
            destination: None,
        },
        payload: serde_json::json!({
            "desktopAgentId": desktop_agent_id,
            "agentDetails": {
                "agentType": "Desktop",
                "id": { "appId": "desktop-agent" }
            }
        }),
    };
    if let Ok(json) = serde_json::to_string(&response) {
        if sink.send(Message::Text(json)).await.is_err() {
            return;
        }
    }

    println!("[dacp] server: inbound peer '{desktop_agent_id}' (app: {app_id})");

    // ── Register peer ─────────────────────────────────────────────────────────
    let (tx, mut rx) = mpsc::unbounded_channel::<BmpMessage>();
    let url = format!("inbound::{app_id}");
    state.registry.register(PeerHandle {
        desktop_agent_id: desktop_agent_id.clone(),
        url: url.clone(),
        tx,
        intent_listeners: std::sync::Arc::new(dashmap::DashMap::new()),
    });
    let _ = state.app.emit(
        "cda:peer_connected",
        CdaPeerConnectedEvent {
            desktop_agent_id: desktop_agent_id.clone(),
            url: url.clone(),
        },
    );

    // ── Phase 2: message loop ─────────────────────────────────────────────────
    loop {
        tokio::select! {
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(txt))) => {
                    if let Ok(bmp) = serde_json::from_str::<BmpMessage>(&txt) {
                        handle_inbound_bmp(&bmp, &desktop_agent_id, &state.registry);
                    }
                }
                Some(Ok(Message::Ping(data))) => {
                    let _ = sink.send(Message::Pong(data)).await;
                }
                Some(Ok(Message::Close(_))) | None => break,
                _ => {}
            },
            outbound = rx.recv() => match outbound {
                Some(bmp) => {
                    if let Ok(json) = serde_json::to_string(&bmp) {
                        if sink.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                }
                None => break,
            },
        }
    }

    // ── Phase 3: cleanup ──────────────────────────────────────────────────────
    state.registry.unregister(&desktop_agent_id);
    let _ = state.app.emit(
        "cda:peer_disconnected",
        CdaPeerDisconnectedEvent {
            desktop_agent_id: desktop_agent_id.clone(),
            url,
        },
    );
    println!("[dacp] server: peer '{desktop_agent_id}' disconnected");
}
