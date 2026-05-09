use futures_util::{SinkExt, StreamExt};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use super::registry::{PeerHandle, PeerRegistry};
use super::types::{
    new_uuid, now_ms_str, AgentIdentifier, BmpMessage, BmpMeta, CdaPeerConnectedEvent,
    CdaPeerDisconnectedEvent, WcpAgentDetails, WcpHelloPayload,
};

// ── Outbound client connection ─────────────────────────────────────────────────

/// Connect to a remote FDC3 2.2 bridge at `url`, complete the WCP2 handshake,
/// register the peer in `registry`, then run the bidirectional message loop.
///
/// On clean or unclean disconnect the peer is unregistered and the URL claim
/// released so the discovery loop can retry on the next scan.
pub async fn connect_to_peer(url: String, registry: PeerRegistry, app: AppHandle) {
    let ws_result = tokio_tungstenite::connect_async(&url).await;
    let ws_stream = match ws_result {
        Ok((s, _)) => s,
        Err(e) => {
            eprintln!("[dacp] cannot connect to {url}: {e}");
            registry.release_url(&url);
            return;
        }
    };

    let (mut sink, mut stream) = ws_stream.split();

    // ── Send WCPHello ─────────────────────────────────────────────────────────
    let hello = BmpMessage {
        msg_type: "WCPHello".to_string(),
        meta: BmpMeta {
            request_uuid: new_uuid(),
            response_uuid: None,
            timestamp: now_ms_str(),
            source: None,
            destination: None,
        },
        payload: serde_json::to_value(WcpHelloPayload {
            desktop_agent_details: WcpAgentDetails {
                agent_type: "Desktop".to_string(),
                id: AgentIdentifier {
                    app_id: "desktop-agent".to_string(),
                    instance_id: None,
                    desktop_agent: None,
                },
            },
            supported_fdc3_versions: vec!["2.2".to_string()],
            channels_to_join: vec![],
        })
        .unwrap_or_default(),
    };

    let hello_json = match serde_json::to_string(&hello) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("[dacp] serialise WCPHello: {e}");
            registry.release_url(&url);
            return;
        }
    };
    if sink.send(Message::Text(hello_json)).await.is_err() {
        eprintln!("[dacp] send WCPHello to {url} failed");
        registry.release_url(&url);
        return;
    }

    // ── Wait for WCPHelloResponse ─────────────────────────────────────────────
    let desktop_agent_id = loop {
        match stream.next().await {
            Some(Ok(Message::Text(txt))) => {
                match serde_json::from_str::<BmpMessage>(&txt) {
                    Ok(msg) if msg.msg_type == "WCPHelloResponse" => {
                        let id = msg
                            .payload
                            .get("desktopAgentId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown-peer")
                            .to_string();
                        break id;
                    }
                    Ok(_) => continue, // ignore non-handshake messages
                    Err(e) => {
                        eprintln!("[dacp] handshake parse error from {url}: {e}");
                        registry.release_url(&url);
                        return;
                    }
                }
            }
            Some(Ok(Message::Close(_))) | None => {
                eprintln!("[dacp] peer {url} closed during handshake");
                registry.release_url(&url);
                return;
            }
            _ => continue,
        }
    };

    println!("[dacp] connected to peer '{desktop_agent_id}' @ {url}");

    // ── Register ──────────────────────────────────────────────────────────────
    let (tx, mut rx) = mpsc::unbounded_channel::<BmpMessage>();
    registry.register(PeerHandle {
        desktop_agent_id: desktop_agent_id.clone(),
        url: url.clone(),
        tx,
        intent_listeners: std::sync::Arc::new(dashmap::DashMap::new()),
    });
    let _ = app.emit(
        "cda:peer_connected",
        CdaPeerConnectedEvent {
            desktop_agent_id: desktop_agent_id.clone(),
            url: url.clone(),
        },
    );

    // ── Message loop ──────────────────────────────────────────────────────────
    loop {
        tokio::select! {
            msg = stream.next() => match msg {
                Some(Ok(Message::Text(txt))) => {
                    if let Ok(bmp) = serde_json::from_str::<BmpMessage>(&txt) {
                        handle_inbound_bmp(&bmp, &desktop_agent_id, &registry);
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
                None => break, // sender dropped
            },
        }
    }

    // ── Cleanup ───────────────────────────────────────────────────────────────
    registry.unregister(&desktop_agent_id);
    registry.release_url(&url);
    let _ = app.emit(
        "cda:peer_disconnected",
        CdaPeerDisconnectedEvent {
            desktop_agent_id: desktop_agent_id.clone(),
            url: url.clone(),
        },
    );
    println!("[dacp] peer '{desktop_agent_id}' disconnected");
}

// ── Shared inbound message handler ────────────────────────────────────────────

/// Process an inbound BMP message from a connected peer.
///
/// Called from both the outbound client path (`peer.rs`) and the inbound server
/// path (`server.rs`).  Currently tracks intent listener registrations; relay
/// for `broadcastBridge` and `raiseIntentBridge` can be extended here.
pub(super) fn handle_inbound_bmp(bmp: &BmpMessage, peer_id: &str, registry: &PeerRegistry) {
    match bmp.msg_type.as_str() {
        "addIntentListenerRequest" | "addIntentListenerBridge" => {
            if let Some(intent) = extract_intent_name(&bmp.payload) {
                registry.add_intent_listener(peer_id, intent);
                println!("[dacp] peer '{peer_id}' registered intent listener: {intent}");
            }
        }
        "removeIntentListenerRequest" | "removeIntentListenerBridge" => {
            if let Some(intent) = extract_intent_name(&bmp.payload) {
                registry.remove_intent_listener(peer_id, intent);
            }
        }
        other => {
            // Unhandled message types are logged; relay logic can be added here.
            println!("[dacp] unhandled '{other}' from peer '{peer_id}'");
        }
    }
}

fn extract_intent_name(payload: &serde_json::Value) -> Option<&str> {
    payload
        .get("intent")
        .and_then(|v| v.get("name"))
        .and_then(|v| v.as_str())
}
