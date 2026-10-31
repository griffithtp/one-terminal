use serde::Serialize;
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

/// Current UTC time as RFC 3339 (`2026-10-31T09:30:00.123Z`), the DACP
/// `Timestamp` format.
pub fn iso_timestamp() -> String {
    format_iso(now_ms())
}

fn format_iso(ms: u64) -> String {
    let secs = ms / 1000;
    let (hour, min, sec) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    // Days-to-civil conversion (Howard Hinnant's algorithm).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{:03}Z",
        ms % 1000
    )
}

// ── Dashboard snapshots ────────────────────────────────────────────────────────

/// Live record for a single connected app instance, as shown on the dashboard.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowHandle {
    pub instance_id: String,
    pub app_id: String,
    pub display_name: Option<String>,
    pub connected_at: u64,
    /// The user channel this instance is currently joined to (if any).
    pub current_channel: Option<String>,
    /// Outbound sender — each string is one DACP JSON message for this instance.
    #[serde(skip)]
    pub tx: UnboundedSender<String>,
}

/// A user channel and its current members, as shown on the dashboard.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub channel_id: String,
    pub display_name: String,
    pub color: String,
    /// `instance_id`s currently joined to this channel.
    pub members: Vec<String>,
}

/// A single intent listener, used by the InteropDashboard resolver.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentHandlerInfo {
    pub intent: String,
    pub app_id: String,
    pub instance_id: String,
    pub display_name: Option<String>,
}

// ── Tauri event payloads (Desktop Agent Rust → dashboard window) ───────────────

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

/// Emitted as `cda:channel_changed` whenever a connection joins or leaves a
/// user channel, so the Desktop Agent UI reflects membership immediately.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaChannelChangedEvent {
    pub instance_id: String,
    pub app_id: String,
    /// The newly joined channel, or `None` after leaving.
    pub channel_id: Option<String>,
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

/// Everything the broker reports to the dashboard.
#[derive(Clone)]
pub enum BrokerEvent {
    Connected(CdaConnectedEvent),
    Disconnected(CdaDisconnectedEvent),
    ChannelChanged(CdaChannelChangedEvent),
    Context(CdaContextEvent),
    Intent(CdaIntentEvent),
}

/// Sink for [`BrokerEvent`]s, so the broker can run without a Tauri `AppHandle`.
pub trait BrokerEvents: Send + Sync {
    fn emit(&self, event: BrokerEvent);
}

#[cfg(test)]
mod tests {
    use super::format_iso;

    #[test]
    fn formats_rfc3339_timestamps() {
        assert_eq!(format_iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_iso(951_782_400_123), "2000-02-29T00:00:00.123Z");
        assert_eq!(format_iso(1_793_491_200_000), "2026-11-01T00:00:00.000Z");
    }
}
