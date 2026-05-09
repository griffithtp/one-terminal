use std::sync::atomic::AtomicBool;

use dashmap::DashMap;
use tokio::sync::{mpsc::UnboundedSender, oneshot, watch, Mutex, RwLock};

use crate::types::{CdaRequest, ChannelInfoSummary, RaiseIntentResult};

pub struct OtFdc3State {
    // ── Connection identity ───────────────────────────────────────────────────
    /// Set to `Some(instance_id)` once the CDA Welcome is received;
    /// reset to `None` on disconnect.  Used by `fdc3_register` to block-wait.
    pub instance_id_tx: watch::Sender<Option<String>>,
    pub instance_id: watch::Receiver<Option<String>>,

    // ── Channel state ─────────────────────────────────────────────────────────
    /// Full channel list received in Welcome.
    pub channels: RwLock<Vec<ChannelInfoSummary>>,
    /// Currently joined user channel.
    pub current_channel: RwLock<Option<String>>,

    // ── Outbound TCP queue ────────────────────────────────────────────────────
    /// `None` while disconnected; set in `client::connect_once`.
    pub tx: Mutex<Option<UnboundedSender<String>>>,

    // ── Pending intent requests ───────────────────────────────────────────────
    /// Maps `request_id` → oneshot sender so `fdc3_raise_intent` can block
    /// until the CDA sends back `IntentResolved` (or errors out).
    pub pending_intents: DashMap<String, oneshot::Sender<Result<RaiseIntentResult, String>>>,

    // ── Lifecycle ─────────────────────────────────────────────────────────────
    /// Guards TCP client startup — only the first `fdc3_register` call starts it.
    pub started: AtomicBool,
}

impl OtFdc3State {
    pub fn new() -> Self {
        let (tx, rx) = watch::channel(None);
        Self {
            instance_id_tx: tx,
            instance_id: rx,
            channels: RwLock::new(Vec::new()),
            current_channel: RwLock::new(None),
            tx: Mutex::new(None),
            pending_intents: DashMap::new(),
            started: AtomicBool::new(false),
        }
    }

    // ── Internal send ─────────────────────────────────────────────────────────

    pub async fn send_request(&self, req: &CdaRequest) -> Result<(), String> {
        let json = serde_json::to_string(req).map_err(|e| e.to_string())?;
        let guard = self.tx.lock().await;
        match guard.as_ref() {
            Some(tx) => tx.send(json).map_err(|e| e.to_string()),
            None => Err("not connected to CDA".to_string()),
        }
    }

    // ── Public helpers (for use by e.g. mock streamers) ───────────────────────

    /// Returns `true` once the CDA handshake has completed.
    pub fn is_connected(&self) -> bool {
        self.instance_id.borrow().is_some()
    }

    /// Returns the current `instance_id`, or `None` if not yet connected.
    pub fn get_instance_id(&self) -> Option<String> {
        self.instance_id.borrow().clone()
    }

    /// Broadcast `context` on `channel_id` via the CDA.
    /// Silently returns `Err` if not connected — callers may ignore it.
    pub async fn broadcast_raw(
        &self,
        channel_id: &str,
        context: serde_json::Value,
    ) -> Result<(), String> {
        self.send_request(&CdaRequest::Broadcast {
            channel_id: channel_id.to_string(),
            context,
        })
        .await
    }

    /// Join `channel_id` on the CDA.
    pub async fn join_channel_raw(&self, channel_id: &str) -> Result<(), String> {
        self.send_request(&CdaRequest::JoinChannel {
            channel_id: channel_id.to_string(),
        })
        .await
    }

    // ── Connection wait ───────────────────────────────────────────────────────

    /// Block until `instance_id` is populated, or `timeout` elapses.
    pub async fn wait_for_instance_id(
        &self,
        timeout: std::time::Duration,
    ) -> Result<String, String> {
        let mut rx = self.instance_id.clone();
        tokio::time::timeout(timeout, async move {
            loop {
                {
                    if let Some(id) = rx.borrow().clone() {
                        return Ok(id);
                    }
                }
                rx.changed()
                    .await
                    .map_err(|_| "instance_id watch channel dropped".to_string())?;
            }
        })
        .await
        .map_err(|_| {
            "timed out waiting for CDA connection — is desktop-agent running?".to_string()
        })?
    }
}

impl Default for OtFdc3State {
    fn default() -> Self {
        Self::new()
    }
}
