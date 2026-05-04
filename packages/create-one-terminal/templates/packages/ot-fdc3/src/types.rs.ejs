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
    Pong,
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
