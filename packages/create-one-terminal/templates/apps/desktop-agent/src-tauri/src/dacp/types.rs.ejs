use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Helpers ────────────────────────────────────────────────────────────────────

pub fn now_ms_str() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
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

// ── FDC3 2.2 Bridge Message Protocol ──────────────────────────────────────────

/// Top-level BMP envelope. Every WS frame is one JSON-serialised `BmpMessage`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BmpMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub meta: BmpMeta,
    #[serde(default)]
    pub payload: serde_json::Value,
}

impl BmpMessage {
    pub fn new(msg_type: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            msg_type: msg_type.into(),
            meta: BmpMeta {
                request_uuid: new_uuid(),
                response_uuid: None,
                timestamp: now_ms_str(),
                source: None,
                destination: None,
            },
            payload,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BmpMeta {
    pub request_uuid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_uuid: Option<String>,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<AgentIdentifier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<AgentIdentifier>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentIdentifier {
    pub app_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_agent: Option<String>,
}

// ── WCP2 handshake payloads ────────────────────────────────────────────────────

/// Payload sent inside a `WCPHello` when we connect to an external bridge.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WcpHelloPayload {
    pub desktop_agent_details: WcpAgentDetails,
    pub supported_fdc3_versions: Vec<String>,
    #[serde(default)]
    pub channels_to_join: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WcpAgentDetails {
    /// Always `"Desktop"` for a native Tauri agent.
    pub agent_type: String,
    pub id: AgentIdentifier,
}

// ── Tauri event payloads ───────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaPeerConnectedEvent {
    pub desktop_agent_id: String,
    pub url: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaPeerDisconnectedEvent {
    pub desktop_agent_id: String,
    pub url: String,
}

// ── IPC response types ─────────────────────────────────────────────────────────

/// Serialisable peer summary returned by the `cda_list_peers` Tauri command.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerInfo {
    pub desktop_agent_id: String,
    pub url: String,
    pub intent_listeners: Vec<String>,
}
