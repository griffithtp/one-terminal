use serde::{Deserialize, Serialize};

// ── TCP port ───────────────────────────────────────────────────────────────────

pub const CDA_TCP_PORT: u16 = 7890;

// ── Helpers ────────────────────────────────────────────────────────────────────

pub fn new_uuid() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        rng.gen::<u32>(),
        rng.gen::<u16>(),
        rng.gen::<u16>() & 0x0fff,
        (rng.gen::<u16>() & 0x3fff) | 0x8000,
        rng.gen::<u64>() & 0x0000_ffff_ffff_ffff,
    )
}

// ── Channel info (received in Welcome) ────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfoSummary {
    pub channel_id: String,
    pub display_name: String,
    pub color: String,
}

// ── IPC-facing types (returned to TypeScript) ─────────────────────────────────

/// Returned by `fdc3_register` — matches `AppIdentifier` in fdc3-client/types.ts.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppIdentifier {
    pub app_id: String,
    pub instance_id: String,
}

/// Returned by `fdc3_get_system_channels` — matches `Channel` in fdc3-client/types.ts.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FdcChannel {
    pub id: String,
    #[serde(rename = "type")]
    pub channel_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_metadata: Option<DisplayMetadata>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DisplayMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Returned by `fdc3_get_registered_apps`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMetadata {
    pub app_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Returned by `fdc3_find_intent` / `fdc3_find_intents_for_context`.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppIntent {
    pub intent: IntentMetadata,
    pub apps: Vec<AppMetadata>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentMetadata {
    pub name: String,
    pub display_name: String,
}

/// Returned by `fdc3_raise_intent` — matches `RaiseIntentResult` in fdc3-client/types.ts.
#[derive(Serialize)]
#[serde(tag = "kind", content = "data")]
pub enum RaiseIntentResult {
    #[serde(rename = "resolved")]
    Resolved(IntentResolution),
    #[serde(rename = "needsResolution")]
    NeedsResolution(Vec<AppIntent>),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentResolution {
    pub source: AppIdentifier,
    pub intent: String,
    pub version: String,
}

// ── CDA TCP protocol — outbound (spoke → CDA) ─────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(dead_code)]
pub enum CdaRequest {
    Hello {
        app_id: String,
        display_name: Option<String>,
    },
    JoinChannel {
        channel_id: String,
    },
    LeaveChannel,
    Broadcast {
        channel_id: String,
        context: serde_json::Value,
    },
    RaiseIntent {
        intent: String,
        context: serde_json::Value,
        target_instance_id: Option<String>,
        request_id: String,
    },
    AddIntentListener {
        intent: String,
    },
    RemoveIntentListener {
        intent: String,
    },
    GetCurrentContext {
        channel_id: String,
        context_type: Option<String>,
        request_id: String,
    },
    FindIntent {
        intent: String,
        context_type: Option<String>,
        request_id: String,
    },
    FindIntentsForContext {
        context_type: String,
        request_id: String,
    },
    GetRegisteredApps {
        request_id: String,
    },
    Ping,
}

// ── CDA TCP protocol — inbound (CDA → spoke) ──────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CdaResponse {
    Welcome {
        instance_id: String,
        channels: Vec<ChannelInfoSummary>,
    },
    ChannelJoined {
        channel_id: String,
    },
    ChannelLeft,
    ContextBroadcast {
        channel_id: String,
        context: serde_json::Value,
        source_instance_id: String,
        source_app_id: String,
    },
    IntentDelivery {
        intent: String,
        context: serde_json::Value,
        source_instance_id: String,
        /// May be absent in older CDA versions — defaults to empty string.
        #[serde(default)]
        source_app_id: String,
        request_id: String,
    },
    IntentResolved {
        intent: String,
        handler_instance_id: String,
        handler_app_id: String,
        request_id: String,
    },
    Error {
        code: String,
        message: String,
        request_id: Option<String>,
    },
    CurrentContext {
        context: Option<serde_json::Value>,
        request_id: String,
    },
    IntentApps {
        app_intents: Vec<AppIntentWire>,
        request_id: String,
    },
    RegisteredApps {
        apps: Vec<AppWire>,
        request_id: String,
    },
    Pong,
}

impl CdaResponse {
    /// Correlation id for query responses routed to `OtFdc3State::query`.
    pub fn query_request_id(&self) -> Option<&str> {
        match self {
            CdaResponse::CurrentContext { request_id, .. }
            | CdaResponse::IntentApps { request_id, .. }
            | CdaResponse::RegisteredApps { request_id, .. } => Some(request_id),
            _ => None,
        }
    }
}

/// Wire form of an FDC3 `AppIntent` as sent by the CDA.
#[derive(Debug, Deserialize)]
pub struct AppIntentWire {
    pub intent: String,
    pub display_name: String,
    pub apps: Vec<AppWire>,
}

/// Wire form of FDC3 `AppMetadata` — `instance_id` is `None` for App Directory
/// entries that are not running.
#[derive(Debug, Deserialize)]
pub struct AppWire {
    pub app_id: String,
    pub instance_id: Option<String>,
    pub name: Option<String>,
}

impl From<AppWire> for AppMetadata {
    fn from(w: AppWire) -> Self {
        AppMetadata {
            app_id: w.app_id,
            instance_id: w.instance_id,
            name: w.name,
        }
    }
}

impl From<AppIntentWire> for AppIntent {
    fn from(w: AppIntentWire) -> Self {
        AppIntent {
            intent: IntentMetadata {
                name: w.intent,
                display_name: w.display_name,
            },
            apps: w.apps.into_iter().map(AppMetadata::from).collect(),
        }
    }
}

// ── Tauri event payloads (Rust → frontend) ────────────────────────────────────

/// `fdc3:context` event — matches `ContextEvent` in fdc3-client/src/agent.ts.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FdcContextEvent {
    pub channel_id: String,
    pub context: serde_json::Value,
    pub source_app_id: String,
    pub source_instance_id: String,
}

/// `fdc3:intent` event — matches `IntentEvent` in fdc3-client/src/agent.ts.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FdcIntentEvent {
    pub intent: String,
    pub context: serde_json::Value,
    pub source_app_id: String,
    pub source_instance_id: String,
    pub request_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FdcChannelJoinedEvent {
    pub channel_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FdcReadyEvent {
    pub instance_id: String,
    pub channels: Vec<ChannelInfoSummary>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FdcErrorEvent {
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wire shapes must match `CdaResponse` in apps/desktop-agent/src-tauri/src/broker/types.rs.
    #[test]
    fn parses_query_responses() {
        let line = r#"{"type":"intent_apps","request_id":"r1","app_intents":[{"intent":"ViewChart","display_name":"View Chart","apps":[{"app_id":"chart","instance_id":"i1","name":"Chart"},{"app_id":"chart","instance_id":null,"name":null}]}]}"#;
        let msg: CdaResponse = serde_json::from_str(line).unwrap();
        assert_eq!(msg.query_request_id(), Some("r1"));
        let CdaResponse::IntentApps { app_intents, .. } = msg else {
            panic!("wrong variant")
        };
        let ai = AppIntent::from(app_intents.into_iter().next().unwrap());
        assert_eq!(ai.intent.name, "ViewChart");
        assert_eq!(ai.apps.len(), 2);
        assert_eq!(ai.apps[0].instance_id.as_deref(), Some("i1"));

        let line = r#"{"type":"current_context","request_id":"r2","context":null}"#;
        let msg: CdaResponse = serde_json::from_str(line).unwrap();
        assert!(matches!(
            msg,
            CdaResponse::CurrentContext { context: None, .. }
        ));

        let line = r#"{"type":"registered_apps","request_id":"r3","apps":[]}"#;
        assert!(serde_json::from_str::<CdaResponse>(line).is_ok());
    }

    #[test]
    fn serialises_query_requests() {
        let req = CdaRequest::FindIntent {
            intent: "ViewChart".into(),
            context_type: Some("fdc3.instrument".into()),
            request_id: "r1".into(),
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["type"], "find_intent");
        assert_eq!(v["context_type"], "fdc3.instrument");
    }
}
