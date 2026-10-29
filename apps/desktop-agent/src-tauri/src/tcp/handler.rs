use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use crate::app_dir::{AppDirectoryCache, AppRecord, CdaIntentNeedsResolutionEvent};
use crate::broker::{
    now_ms, AppIntentInfo, CdaChannelChangedEvent, CdaConnectedEvent, CdaContextEvent, CdaDisconnectedEvent, CdaErrorCode,
    CdaIntentEvent, CdaRequest, CdaResponse, ChannelManager, IntentAppInfo, IntentRegistry,
    WindowManager,
};
use crate::dacp::PeerRegistry;

/// Top-level per-connection coroutine.  Three phases:
///
/// 1. **Handshake** — read lines until a valid `Hello` arrives; register the
///    spoke, send `Welcome`, and emit `cda:connected`.
/// 2. **Message loop** — `tokio::select!` between inbound TCP lines and the
///    outbound mpsc channel.
/// 3. **Cleanup** — deregister the spoke from all registries and emit
///    `cda:disconnected`.
pub async fn handle_connection(
    stream: TcpStream,
    window_manager: WindowManager,
    channel_manager: ChannelManager,
    intent_registry: IntentRegistry,
    peer_registry: PeerRegistry,
    app_dir_cache: AppDirectoryCache,
    app: AppHandle,
) {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();

    // ── Phase 1: Hello handshake ──────────────────────────────────────────────
    let (instance_id, app_id) = loop {
        let mut line = String::new();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<CdaRequest>(trimmed) {
                    Ok(CdaRequest::Hello {
                        app_id,
                        display_name,
                    }) => {
                        let iid = window_manager.register(
                            app_id.clone(),
                            display_name.clone(),
                            tx.clone(),
                        );
                        let welcome = CdaResponse::Welcome {
                            instance_id: iid.clone(),
                            channels: channel_manager.list_summaries(),
                        };
                        let json = serde_json::to_string(&welcome).unwrap_or_default();
                        if write_half
                            .write_all(format!("{json}\n").as_bytes())
                            .await
                            .is_err()
                        {
                            window_manager.unregister(&iid);
                            return;
                        }
                        let _ = app.emit(
                            "cda:connected",
                            CdaConnectedEvent {
                                instance_id: iid.clone(),
                                app_id: app_id.clone(),
                                display_name,
                                connected_at: now_ms(),
                            },
                        );
                        println!("[cda] spoke registered: {app_id} ({iid})");
                        break (iid, app_id);
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("[cda] handshake parse error: {e}"),
                }
            }
        }
    };

    // ── Phase 2: message loop ──────────────────────────────────────────────────
    loop {
        let mut line = String::new();
        tokio::select! {
            result = reader.read_line(&mut line) => {
                match result {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<CdaRequest>(trimmed) {
                            Ok(req) => dispatch(
                                req,
                                &instance_id,
                                &app_id,
                                &window_manager,
                                &channel_manager,
                                &intent_registry,
                                &peer_registry,
                                &app_dir_cache,
                                &app,
                            ).await,
                            Err(e) => {
                                eprintln!("[cda] parse error from {instance_id}: {e}");
                                let err = serde_json::to_string(&CdaResponse::Error {
                                    code: CdaErrorCode::MalformedMessage,
                                    message: e.to_string(),
                                    request_id: None,
                                })
                                .unwrap_or_default();
                                if write_half
                                    .write_all(format!("{err}\n").as_bytes())
                                    .await
                                    .is_err()
                                {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            outbound = rx.recv() => {
                match outbound {
                    Some(msg) => {
                        if write_half
                            .write_all(format!("{msg}\n").as_bytes())
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    None => break, // sender dropped — server-initiated disconnect
                }
            }
        }
    }

    // ── Phase 3: cleanup ───────────────────────────────────────────────────────
    intent_registry.remove_all(&instance_id);
    channel_manager.remove_member(&instance_id);
    window_manager.unregister(&instance_id);
    let _ = app.emit(
        "cda:disconnected",
        CdaDisconnectedEvent {
            instance_id: instance_id.clone(),
            app_id: app_id.clone(),
        },
    );
    println!("[cda] spoke disconnected: {app_id} ({instance_id})");
}

// ── Dispatch ───────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
async fn dispatch(
    req: CdaRequest,
    instance_id: &str,
    app_id: &str,
    window_manager: &WindowManager,
    channel_manager: &ChannelManager,
    intent_registry: &IntentRegistry,
    peer_registry: &PeerRegistry,
    app_dir_cache: &AppDirectoryCache,
    app: &AppHandle,
) {
    match req {
        CdaRequest::JoinChannel { channel_id } => {
            match channel_manager.join(instance_id, &channel_id) {
                Ok(()) => {
                    window_manager.set_channel(instance_id, Some(channel_id.clone()));
                    let _ = app.emit(
                        "cda:channel_changed",
                        CdaChannelChangedEvent {
                            instance_id: instance_id.to_string(),
                            app_id: app_id.to_string(),
                            channel_id: Some(channel_id.clone()),
                        },
                    );
                    let resp = serde_json::to_string(&CdaResponse::ChannelJoined { channel_id })
                        .unwrap_or_default();
                    window_manager.send_to(instance_id, &resp);
                }
                Err(msg) => {
                    let err = serde_json::to_string(&CdaResponse::Error {
                        code: CdaErrorCode::ChannelNotFound,
                        message: msg,
                        request_id: None,
                    })
                    .unwrap_or_default();
                    window_manager.send_to(instance_id, &err);
                }
            }
        }

        CdaRequest::LeaveChannel => {
            channel_manager.leave(instance_id);
            window_manager.set_channel(instance_id, None);
            let _ = app.emit(
                "cda:channel_changed",
                CdaChannelChangedEvent {
                    instance_id: instance_id.to_string(),
                    app_id: app_id.to_string(),
                    channel_id: None,
                },
            );
            let resp = serde_json::to_string(&CdaResponse::ChannelLeft).unwrap_or_default();
            window_manager.send_to(instance_id, &resp);
        }

        CdaRequest::Broadcast {
            channel_id,
            context,
        } => {
            fan_out_broadcast(
                instance_id,
                app_id,
                &channel_id,
                context,
                window_manager,
                channel_manager,
                peer_registry,
                app,
            );
        }

        CdaRequest::RaiseIntent {
            intent,
            context,
            target_instance_id,
            request_id,
        } => {
            deliver_intent(
                instance_id,
                &intent,
                context,
                target_instance_id.as_deref(),
                &request_id,
                window_manager,
                intent_registry,
                peer_registry,
                app_dir_cache,
                app,
            )
            .await;
        }

        CdaRequest::AddIntentListener { intent } => {
            intent_registry.add(instance_id, &intent);
            println!("[cda] {app_id} ({instance_id}) added intent listener: {intent}");
        }

        CdaRequest::RemoveIntentListener { intent } => {
            intent_registry.remove(instance_id, &intent);
            println!("[cda] {app_id} ({instance_id}) removed intent listener: {intent}");
        }

        CdaRequest::GetCurrentContext {
            channel_id,
            context_type,
            request_id,
        } => {
            let context = channel_manager.get_last_context(&channel_id, context_type.as_deref());
            reply(
                instance_id,
                &CdaResponse::CurrentContext {
                    context,
                    request_id,
                },
                window_manager,
            );
        }

        CdaRequest::FindIntent {
            intent,
            context_type,
            request_id,
        } => {
            let app_intent = find_intent_apps(
                &intent,
                context_type.as_deref(),
                window_manager,
                intent_registry,
                app_dir_cache,
            )
            .await;
            reply(
                instance_id,
                &CdaResponse::IntentApps {
                    app_intents: vec![app_intent],
                    request_id,
                },
                window_manager,
            );
        }

        CdaRequest::FindIntentsForContext {
            context_type,
            request_id,
        } => {
            let app_intents = find_intents_for_context(
                &context_type,
                window_manager,
                intent_registry,
                app_dir_cache,
            )
            .await;
            reply(
                instance_id,
                &CdaResponse::IntentApps {
                    app_intents,
                    request_id,
                },
                window_manager,
            );
        }

        CdaRequest::GetRegisteredApps { request_id } => {
            let apps = window_manager
                .list_all()
                .into_iter()
                .map(|h| IntentAppInfo {
                    app_id: h.app_id,
                    instance_id: Some(h.instance_id),
                    name: h.display_name,
                })
                .collect();
            reply(
                instance_id,
                &CdaResponse::RegisteredApps { apps, request_id },
                window_manager,
            );
        }

        CdaRequest::Ping => {
            let pong = serde_json::to_string(&CdaResponse::Pong).unwrap_or_default();
            window_manager.send_to(instance_id, &pong);
        }

        CdaRequest::Hello { .. } => {
            // Duplicate Hello after handshake — ignore.
        }
    }
}

// ── Broadcast fan-out ─────────────────────────────────────────────────────────

/// Fan-out a context broadcast to all local channel members and all connected
/// external bridge peers.
#[allow(clippy::too_many_arguments)]
pub(crate) fn fan_out_broadcast(
    source_instance_id: &str,
    source_app_id: &str,
    channel_id: &str,
    context: serde_json::Value,
    window_manager: &WindowManager,
    channel_manager: &ChannelManager,
    peer_registry: &PeerRegistry,
    app: &AppHandle,
) {
    let context_type = context
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    channel_manager.set_last_context(channel_id, &context_type, context.clone());

    let targets = channel_manager.members_except(channel_id, source_instance_id);

    let payload = serde_json::to_string(&CdaResponse::ContextBroadcast {
        channel_id: channel_id.to_string(),
        context: context.clone(),
        source_instance_id: source_instance_id.to_string(),
        source_app_id: source_app_id.to_string(),
    })
    .unwrap_or_default();

    let mut recipient_count = 0usize;
    for target_id in &targets {
        if window_manager.send_to(target_id, &payload) {
            recipient_count += 1;
        }
    }

    recipient_count += peer_registry.forward_broadcast(channel_id, &context, source_app_id);

    let _ = app.emit(
        "cda:context",
        CdaContextEvent {
            channel_id: channel_id.to_string(),
            context,
            source_instance_id: source_instance_id.to_string(),
            source_app_id: source_app_id.to_string(),
            recipient_count,
        },
    );
}

// ── Intent delivery ───────────────────────────────────────────────────────────

/// Deliver an intent using a four-tier resolution strategy:
///
/// 1. **Explicit target** — `target_instance_id` is set and found locally.
/// 2. **Local registry** — `IntentRegistry` has a spoke that registered this intent.
/// 3. **Peer bridge** — `PeerRegistry` has an external agent that registered this intent.
/// 4. **App Directory** — cache has apps declaring `listensFor` this intent;
///    emit `cda:intent_needs_resolution` so the frontend resolver can launch one.
///
/// If all tiers fail, `Error { NoIntentHandlers }` is sent to the raiser.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn deliver_intent(
    source_instance_id: &str,
    intent: &str,
    context: serde_json::Value,
    target_instance_id: Option<&str>,
    request_id: &str,
    window_manager: &WindowManager,
    intent_registry: &IntentRegistry,
    peer_registry: &PeerRegistry,
    app_dir_cache: &AppDirectoryCache,
    app: &AppHandle,
) {
    // Tier 1 & 2: explicit target or local intent registry
    let resolved_local = match target_instance_id {
        Some(tid) => {
            if window_manager.get(tid).is_some() {
                Some(tid.to_string())
            } else {
                None
            }
        }
        None => intent_registry.find_handler(intent),
    };

    match resolved_local {
        Some(target_id) => {
            let handler = match window_manager.get(&target_id) {
                Some(h) => h,
                None => {
                    send_no_handler_error(source_instance_id, intent, request_id, window_manager);
                    return;
                }
            };

            let source_app_id = window_manager
                .get(source_instance_id)
                .map(|h| h.app_id)
                .unwrap_or_default();
            let delivery = serde_json::to_string(&CdaResponse::IntentDelivery {
                intent: intent.to_string(),
                context,
                source_instance_id: source_instance_id.to_string(),
                source_app_id,
                request_id: request_id.to_string(),
            })
            .unwrap_or_default();
            window_manager.send_to(&target_id, &delivery);

            let resolved = serde_json::to_string(&CdaResponse::IntentResolved {
                intent: intent.to_string(),
                handler_instance_id: target_id.clone(),
                handler_app_id: handler.app_id.clone(),
                request_id: request_id.to_string(),
            })
            .unwrap_or_default();
            window_manager.send_to(source_instance_id, &resolved);

            let _ = app.emit(
                "cda:intent",
                CdaIntentEvent {
                    intent: intent.to_string(),
                    source_instance_id: source_instance_id.to_string(),
                    handler_instance_id: target_id,
                    request_id: request_id.to_string(),
                },
            );
        }

        None => {
            // Tier 3: forward to a connected bridge peer
            if peer_registry.forward_raise_intent(intent, &context, source_instance_id, request_id)
            {
                println!("[cda] intent '{intent}' forwarded to peer bridge");
                return;
            }

            // Tier 4: consult App Directory cache for installable candidates
            let candidates = app_dir_cache.handlers_for_intent(intent).await;
            if !candidates.is_empty() {
                println!(
                    "[cda] intent '{intent}' needs resolution — {} App Dir candidate(s)",
                    candidates.len()
                );
                let _ = app.emit(
                    "cda:intent_needs_resolution",
                    CdaIntentNeedsResolutionEvent {
                        intent: intent.to_string(),
                        context,
                        source_instance_id: source_instance_id.to_string(),
                        request_id: request_id.to_string(),
                        candidates,
                    },
                );
            } else {
                send_no_handler_error(source_instance_id, intent, request_id, window_manager);
            }
        }
    }
}

// ── Intent discovery ──────────────────────────────────────────────────────────

/// Whether `app` declares `listensFor[intent]` and that declaration accepts
/// `context_type`. An empty `contexts` list is treated as "accepts any".
fn app_accepts(app: &AppRecord, intent: &str, context_type: Option<&str>) -> Option<bool> {
    let def = app
        .interop
        .as_ref()?
        .intents
        .as_ref()?
        .listens_for
        .as_ref()?
        .get(intent)?;
    Some(match context_type {
        Some(ct) => def.contexts.is_empty() || def.contexts.iter().any(|c| c == ct),
        None => true,
    })
}

/// Collect every app that can handle `intent`: running spokes that registered
/// an intent listener, then App Directory entries that declare `listensFor`
/// the intent. Running spokes whose App Directory record explicitly rejects
/// `context_type` are excluded.
pub(crate) async fn find_intent_apps(
    intent: &str,
    context_type: Option<&str>,
    window_manager: &WindowManager,
    intent_registry: &IntentRegistry,
    app_dir_cache: &AppDirectoryCache,
) -> AppIntentInfo {
    let directory = app_dir_cache.list_all().await;
    let mut apps = Vec::new();

    for iid in intent_registry.handlers(intent) {
        let Some(handle) = window_manager.get(&iid) else {
            continue;
        };
        let record = directory.iter().find(|a| a.app_id == handle.app_id);
        if record.and_then(|r| app_accepts(r, intent, context_type)) == Some(false) {
            continue;
        }
        apps.push(IntentAppInfo {
            app_id: handle.app_id,
            instance_id: Some(handle.instance_id),
            name: handle.display_name,
        });
    }

    let mut display_name = None;
    for app in &directory {
        if app_accepts(app, intent, context_type) != Some(true) {
            continue;
        }
        if display_name.is_none() {
            display_name = app
                .interop
                .as_ref()
                .and_then(|i| i.intents.as_ref())
                .and_then(|i| i.listens_for.as_ref())
                .and_then(|lf| lf.get(intent))
                .and_then(|d| d.display_name.clone());
        }
        apps.push(IntentAppInfo {
            app_id: app.app_id.clone(),
            instance_id: None,
            name: Some(app.title.clone().unwrap_or_else(|| app.name.clone())),
        });
    }

    AppIntentInfo {
        intent: intent.to_string(),
        display_name: display_name.unwrap_or_else(|| intent.to_string()),
        apps,
    }
}

/// Collect every intent whose App Directory declaration lists `context_type`
/// in its `contexts`, each with its handling apps. Intents with no apps are
/// omitted.
pub(crate) async fn find_intents_for_context(
    context_type: &str,
    window_manager: &WindowManager,
    intent_registry: &IntentRegistry,
    app_dir_cache: &AppDirectoryCache,
) -> Vec<AppIntentInfo> {
    let mut intents: Vec<String> = app_dir_cache
        .list_all()
        .await
        .iter()
        .filter_map(|a| a.interop.as_ref()?.intents.as_ref()?.listens_for.as_ref())
        .flat_map(|lf| {
            lf.iter()
                .filter(|(_, def)| def.contexts.iter().any(|c| c == context_type))
                .map(|(name, _)| name.clone())
        })
        .collect();
    intents.sort();
    intents.dedup();

    let mut out = Vec::new();
    for intent in intents {
        let ai = find_intent_apps(
            &intent,
            Some(context_type),
            window_manager,
            intent_registry,
            app_dir_cache,
        )
        .await;
        if !ai.apps.is_empty() {
            out.push(ai);
        }
    }
    out
}

fn reply(instance_id: &str, resp: &CdaResponse, window_manager: &WindowManager) {
    let json = serde_json::to_string(resp).unwrap_or_default();
    window_manager.send_to(instance_id, &json);
}

fn send_no_handler_error(
    instance_id: &str,
    intent: &str,
    request_id: &str,
    window_manager: &WindowManager,
) {
    let err = serde_json::to_string(&CdaResponse::Error {
        code: CdaErrorCode::NoIntentHandlers,
        message: format!("No local, bridge, or App Directory handler found for intent '{intent}'"),
        request_id: Some(request_id.to_string()),
    })
    .unwrap_or_default();
    window_manager.send_to(instance_id, &err);
}
