use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use super::types::{BmpMessage, PeerInfo};

// ── Per-peer handle ────────────────────────────────────────────────────────────

/// Live connection state for a single external bridge peer.
#[derive(Clone, Debug)]
pub struct PeerHandle {
    pub desktop_agent_id: String,
    /// `"ws://127.0.0.1:{port}/v2/bridge"` for outbound connections;
    /// `"inbound::{app_id}"` for agents that connected to us.
    pub url: String,
    /// Drop or send here to push a BMP message to the peer's WebSocket.
    pub tx: UnboundedSender<BmpMessage>,
    /// Intent names this peer has registered as handlers.
    pub intent_listeners: Arc<DashMap<String, ()>>,
}

// ── Registry ───────────────────────────────────────────────────────────────────

/// Thread-safe registry of all connected external bridge peers.
///
/// Cheap to clone — every field is behind an `Arc`.  All methods are
/// lock-free at the entry level via DashMap shard-level locking.
#[derive(Clone)]
pub struct PeerRegistry {
    /// `desktop_agent_id` → `PeerHandle`
    peers: Arc<DashMap<String, PeerHandle>>,
    /// `intent_name` → first `desktop_agent_id` to register that intent
    intent_index: Arc<DashMap<String, String>>,
    /// URLs that are already connected (or being connected) — prevents duplicate tasks.
    known_urls: Arc<DashMap<String, ()>>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(DashMap::new()),
            intent_index: Arc::new(DashMap::new()),
            known_urls: Arc::new(DashMap::new()),
        }
    }

    // ── URL deduplication ─────────────────────────────────────────────────────

    /// Atomically claim a URL.  Returns `true` if the URL was new.
    /// Call `release_url` if the connection attempt fails so the URL can be
    /// retried on the next discovery scan.
    pub fn try_claim_url(&self, url: &str) -> bool {
        self.known_urls.insert(url.to_string(), ()).is_none()
    }

    pub fn release_url(&self, url: &str) {
        self.known_urls.remove(url);
    }

    // ── Peer lifecycle ────────────────────────────────────────────────────────

    pub fn register(&self, handle: PeerHandle) {
        self.peers.insert(handle.desktop_agent_id.clone(), handle);
    }

    pub fn unregister(&self, desktop_agent_id: &str) {
        if let Some((_, h)) = self.peers.remove(desktop_agent_id) {
            let intents: Vec<String> = h.intent_listeners.iter().map(|e| e.key().clone()).collect();
            for intent in intents {
                self.intent_index
                    .remove_if(&intent, |_, v| v.as_str() == desktop_agent_id);
            }
        }
    }

    // ── Intent listener management ────────────────────────────────────────────

    pub fn add_intent_listener(&self, desktop_agent_id: &str, intent: &str) {
        if let Some(h) = self.peers.get(desktop_agent_id) {
            h.intent_listeners.insert(intent.to_string(), ());
            drop(h); // release shard before touching intent_index
        }
        self.intent_index
            .entry(intent.to_string())
            .or_insert_with(|| desktop_agent_id.to_string());
    }

    pub fn remove_intent_listener(&self, desktop_agent_id: &str, intent: &str) {
        if let Some(h) = self.peers.get(desktop_agent_id) {
            h.intent_listeners.remove(intent);
        }
        self.intent_index
            .remove_if(intent, |_, v| v.as_str() == desktop_agent_id);
    }

    // ── Intent routing ────────────────────────────────────────────────────────

    /// Find the `desktop_agent_id` of a peer that handles `intent`, if any.
    pub fn find_intent_handler(&self, intent: &str) -> Option<String> {
        self.intent_index.get(intent).map(|e| e.value().clone())
    }

    /// Forward a `raiseIntent` to the registered peer handler.
    ///
    /// Returns `true` if a peer was found and the message was queued.
    pub fn forward_raise_intent(
        &self,
        intent: &str,
        context: &serde_json::Value,
        source_instance_id: &str,
        request_id: &str,
    ) -> bool {
        let agent_id = match self.find_intent_handler(intent) {
            Some(id) => id,
            None => return false,
        };
        // Clone the sender out of the DashMap shard lock before calling send.
        let tx = match self.peers.get(&agent_id) {
            Some(h) => h.tx.clone(),
            None => return false,
        };
        let msg = BmpMessage::new(
            "raiseIntentBridge",
            serde_json::json!({
                "intent": { "name": intent },
                "context": context,
                "appIdentifier": {
                    "appId": source_instance_id,
                    "instanceId": source_instance_id,
                    "desktopAgent": "desktop-agent"
                },
                "requestUuid": request_id
            }),
        );
        let _ = tx.send(msg);
        true
    }

    // ── Broadcast forwarding ──────────────────────────────────────────────────

    /// Push a context broadcast to every connected peer.
    /// Returns the number of peers the message was queued for.
    pub fn forward_broadcast(
        &self,
        channel_id: &str,
        context: &serde_json::Value,
        source_app_id: &str,
    ) -> usize {
        if self.peers.is_empty() {
            return 0;
        }
        let msg = BmpMessage::new(
            "broadcastBridge",
            serde_json::json!({
                "channelId": channel_id,
                "context": context,
                "source": {
                    "appId": source_app_id,
                    "desktopAgent": "desktop-agent"
                }
            }),
        );
        let mut count = 0usize;
        for entry in self.peers.iter() {
            if entry.tx.send(msg.clone()).is_ok() {
                count += 1;
            }
        }
        count
    }

    // ── Query ─────────────────────────────────────────────────────────────────

    pub fn list_all(&self) -> Vec<PeerInfo> {
        self.peers
            .iter()
            .map(|e| {
                let h = e.value();
                PeerInfo {
                    desktop_agent_id: h.desktop_agent_id.clone(),
                    url: h.url.clone(),
                    intent_listeners: h.intent_listeners.iter().map(|i| i.key().clone()).collect(),
                }
            })
            .collect()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}
