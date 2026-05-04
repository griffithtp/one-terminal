use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc::UnboundedSender;

// ── Helpers ────────────────────────────────────────────────────────────────────

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

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

// ── Constants ──────────────────────────────────────────────────────────────────

// ── Registry types ─────────────────────────────────────────────────────────────

/// Live record for a single connected spoke app instance.
/// Stored in `WindowManager` keyed by `instance_id`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowHandle {
    pub instance_id: String,
    pub app_id: String,
    pub display_name: Option<String>,
    pub connected_at: u64,
    /// The channel this spoke is currently joined to (if any).
    pub current_channel: Option<String>,
    /// Outbound sender — write a JSON line here to push to this spoke's TCP conn.
    #[serde(skip)]
    pub tx: UnboundedSender<String>,
}

/// Channel descriptor stored in `ChannelManager`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub channel_id: String,
    pub display_name: String,
    pub color: String,
    /// `instance_id`s currently joined to this channel.
    pub members: Vec<String>,
    /// Last-value cache: context_type → last broadcast Context JSON value.
    #[serde(skip)]
    pub last_context: HashMap<String, serde_json::Value>,
}

/// Lightweight channel descriptor sent inside `Welcome` — omits member list
/// and last-context cache so it can be serialised freely.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfoSummary {
    pub channel_id: String,
    pub display_name: String,
    pub color: String,
}

// ── TCP message protocol — inbound (spoke → CDA) ──────────────────────────────

/// Every message sent from a spoke over TCP is one newline-terminated JSON line
/// discriminated by the `"type"` field.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CdaRequest {
    /// First message after TCP connection opens. Must precede all others.
    Hello {
        app_id: String,
        display_name: Option<String>,
    },
    /// Join a named user channel, leaving any previous channel first.
    JoinChannel { channel_id: String },
    /// Leave the current channel without joining another.
    LeaveChannel,
    /// Broadcast a context object to all other members on the same channel.
    Broadcast {
        channel_id: String,
        context: serde_json::Value,
    },
    /// Raise a named intent, optionally targeting a specific spoke instance.
    RaiseIntent {
        intent: String,
        context: serde_json::Value,
        target_instance_id: Option<String>,
        /// Caller-supplied correlation ID echoed back in `IntentResolved`.
        request_id: String,
    },
    /// Register this spoke as a handler for a named intent.
    AddIntentListener { intent: String },
    /// Deregister this spoke as a handler for a named intent.
    RemoveIntentListener { intent: String },
    /// Keep-alive ping; CDA replies with `Pong`.
    Ping,
}

// ── TCP message protocol — outbound (CDA → spoke) ─────────────────────────────

/// Every message pushed from the CDA to a spoke over TCP is one newline-
/// terminated JSON line discriminated by the `"type"` field.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CdaResponse {
    /// Sent immediately after a valid `Hello` is processed.
    Welcome {
        instance_id: String,
        channels: Vec<ChannelInfoSummary>,
    },
    /// ACK for `JoinChannel`.
    ChannelJoined { channel_id: String },
    /// ACK for `LeaveChannel`.
    ChannelLeft,
    /// Pushed to all other members when a spoke broadcasts on a channel.
    ContextBroadcast {
        channel_id: String,
        context: serde_json::Value,
        source_instance_id: String,
        source_app_id: String,
    },
    /// Pushed to the target spoke when an intent is raised against it.
    IntentDelivery {
        intent: String,
        context: serde_json::Value,
        source_instance_id: String,
        request_id: String,
    },
    /// Pushed back to the intent raiser confirming a handler was found.
    IntentResolved {
        intent: String,
        handler_instance_id: String,
        handler_app_id: String,
        request_id: String,
    },
    /// Error from the CDA to a spoke.
    Error {
        code: CdaErrorCode,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
    /// Response to `Ping`.
    Pong,
}

/// Wire-safe error code variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CdaErrorCode {
    ChannelNotFound,
    NoIntentHandlers,
    MalformedMessage,
    HandlerTimeout,
}

// ── Intent handler descriptor (local + bridged) ───────────────────────────────

/// A single intent handler entry, used by the InteropDashboard resolver.
/// `desktop_agent_id` is `None` for handlers on this local CDA and `Some`
/// for handlers registered by an external bridge peer.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentHandlerInfo {
    pub intent: String,
    pub app_id: String,
    pub instance_id: String,
    pub display_name: Option<String>,
    /// `None` → local spoke; `Some(id)` → handler lives on peer bridge `id`.
    pub desktop_agent_id: Option<String>,
}

// ── Tauri event payloads (CDA Rust → CDA dashboard window) ────────────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaConnectedEvent {
    pub instance_id: String,
    pub app_id: String,
    pub display_name: Option<String>,
    pub connected_at: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaDisconnectedEvent {
    pub instance_id: String,
    pub app_id: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaContextEvent {
    pub channel_id: String,
    pub context: serde_json::Value,
    pub source_instance_id: String,
    pub source_app_id: String,
    pub recipient_count: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaIntentEvent {
    pub intent: String,
    pub source_instance_id: String,
    pub handler_instance_id: String,
    pub request_id: String,
}
