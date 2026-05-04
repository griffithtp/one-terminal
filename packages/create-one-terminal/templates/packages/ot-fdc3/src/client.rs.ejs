use std::sync::Arc;

use tauri::{AppHandle, Emitter, Runtime};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::state::OtFdc3State;
use crate::types::{
    CdaRequest, CdaResponse,
    FdcChannelJoinedEvent, FdcContextEvent, FdcErrorEvent,
    FdcIntentEvent, FdcReadyEvent, RaiseIntentResult, IntentResolution, AppIdentifier,
    CDA_TCP_PORT,
};

fn broker_port() -> u16 {
    std::env::var("OT_TCP_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(CDA_TCP_PORT)
}

/// Entry point spawned on the first `fdc3_register` call.
/// Reconnects automatically with a 2-second delay on each disconnect.
pub async fn run<R: Runtime>(state: Arc<OtFdc3State>, app_id: String, app: AppHandle<R>) {
    loop {
        connect_once(state.clone(), app_id.clone(), app.clone()).await;

        // Clear sender and instance_id so callers know we are disconnected.
        *state.tx.lock().await = None;
        let _ = state.instance_id_tx.send(None);

        eprintln!("[ot-fdc3] disconnected from CDA — retrying in 2 s");
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

async fn connect_once<R: Runtime>(
    state: Arc<OtFdc3State>,
    app_id: String,
    app: AppHandle<R>,
) {
    let stream = match TcpStream::connect(("127.0.0.1", broker_port())).await {
        Ok(s)  => s,
        Err(e) => {
            eprintln!("[ot-fdc3] cannot connect to CDA: {e}");
            return;
        }
    };

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    // ── Handshake — Hello ─────────────────────────────────────────────────────
    let hello = CdaRequest::Hello {
        app_id: app_id.clone(),
        display_name: Some(app_id.clone()),
    };
    let hello_json = match serde_json::to_string(&hello) {
        Ok(j)  => j,
        Err(e) => { eprintln!("[ot-fdc3] serialise Hello: {e}"); return; }
    };
    if let Err(e) = writer.write_all(format!("{hello_json}\n").as_bytes()).await {
        eprintln!("[ot-fdc3] send Hello: {e}");
        return;
    }

    // ── Wait for Welcome ──────────────────────────────────────────────────────
    let (instance_id, channels) = loop {
        match lines.next_line().await {
            Ok(Some(line)) => match serde_json::from_str::<CdaResponse>(&line) {
                Ok(CdaResponse::Welcome { instance_id, channels }) => {
                    break (instance_id, channels);
                }
                Ok(_)  => continue,
                Err(e) => {
                    eprintln!("[ot-fdc3] handshake parse error: {e}");
                    return;
                }
            },
            Ok(None) | Err(_) => {
                eprintln!("[ot-fdc3] CDA closed connection during handshake");
                return;
            }
        }
    };

    println!("[ot-fdc3] connected — app_id={app_id} instance_id={instance_id}");

    // ── Update state ──────────────────────────────────────────────────────────
    *state.channels.write().await = channels.clone();
    let _ = state.instance_id_tx.send(Some(instance_id.clone()));

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    *state.tx.lock().await = Some(tx);

    let _ = app.emit("fdc3:ready", FdcReadyEvent { instance_id, channels });

    // ── Writer task ───────────────────────────────────────────────────────────
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if writer.write_all(format!("{line}\n").as_bytes()).await.is_err() {
                break;
            }
        }
    });

    // ── Inbound message loop ──────────────────────────────────────────────────
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => handle_line(&line, &state, &app).await,
            Ok(None) | Err(_) => break,
        }
    }
}

async fn handle_line<R: Runtime>(
    line: &str,
    state: &Arc<OtFdc3State>,
    app: &AppHandle<R>,
) {
    let msg = match serde_json::from_str::<CdaResponse>(line) {
        Ok(m)  => m,
        Err(e) => {
            eprintln!("[ot-fdc3] parse error: {e} — raw: {line}");
            return;
        }
    };

    match msg {
        CdaResponse::ContextBroadcast {
            channel_id, context, source_instance_id, source_app_id,
        } => {
            let _ = app.emit("fdc3:context", FdcContextEvent {
                channel_id,
                context,
                source_app_id,
                source_instance_id,
            });
        }

        CdaResponse::IntentDelivery {
            intent, context, source_instance_id, source_app_id, request_id,
        } => {
            let _ = app.emit("fdc3:intent", FdcIntentEvent {
                intent,
                context,
                source_app_id,
                source_instance_id,
                request_id,
            });
        }

        CdaResponse::IntentResolved {
            intent, handler_instance_id, handler_app_id, request_id,
        } => {
            if let Some((_, tx)) = state.pending_intents.remove(&request_id) {
                let resolution = RaiseIntentResult::Resolved(IntentResolution {
                    source: AppIdentifier {
                        app_id: handler_app_id,
                        instance_id: handler_instance_id,
                    },
                    intent,
                    version: "2.2".to_string(),
                });
                let _ = tx.send(Ok(resolution));
            }
        }

        CdaResponse::ChannelJoined { channel_id } => {
            *state.current_channel.write().await = Some(channel_id.clone());
            let _ = app.emit("fdc3:channel_joined", FdcChannelJoinedEvent { channel_id });
        }

        CdaResponse::ChannelLeft => {
            *state.current_channel.write().await = None;
            let _ = app.emit("fdc3:channel_left", ());
        }

        CdaResponse::Error { code, message, request_id } => {
            // Reject any pending intent waiting for this request_id.
            if let Some(rid) = &request_id {
                if let Some((_, tx)) = state.pending_intents.remove(rid) {
                    let _ = tx.send(Err(format!("[{code}] {message}")));
                }
            }
            let _ = app.emit("fdc3:error", FdcErrorEvent { code, message, request_id });
        }

        CdaResponse::Pong | CdaResponse::Welcome { .. } => {}
    }
}
