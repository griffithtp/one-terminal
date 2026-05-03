use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use super::types::{WindowHandle, new_uuid, now_ms};

/// Thread-safe, clone-safe registry of all connected spoke app instances.
///
/// Uses `DashMap` instead of `Arc<Mutex<HashMap>>` for shard-level locking —
/// concurrent reads/writes on different keys never contend.
#[derive(Clone)]
pub struct WindowManager {
    inner: Arc<DashMap<String, WindowHandle>>,
}

impl WindowManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Register a new spoke. Generates a UUID instance_id, inserts the handle,
    /// and returns the new instance_id.
    pub fn register(
        &self,
        app_id: String,
        display_name: Option<String>,
        tx: UnboundedSender<String>,
    ) -> String {
        let instance_id = new_uuid();
        let handle = WindowHandle {
            instance_id: instance_id.clone(),
            app_id,
            display_name,
            connected_at: now_ms(),
            current_channel: None,
            tx,
        };
        self.inner.insert(instance_id.clone(), handle);
        instance_id
    }

    /// Remove a spoke on disconnect. Returns the removed handle if it existed.
    pub fn unregister(&self, instance_id: &str) -> Option<WindowHandle> {
        self.inner.remove(instance_id).map(|(_, h)| h)
    }

    /// Update the `current_channel` field on a spoke's handle (cosmetic —
    /// canonical membership lives in `ChannelManager`).
    pub fn set_channel(&self, instance_id: &str, channel_id: Option<String>) {
        if let Some(mut handle) = self.inner.get_mut(instance_id) {
            handle.current_channel = channel_id;
        }
    }

    /// Return a clone of a handle (for Tauri command reads).
    pub fn get(&self, instance_id: &str) -> Option<WindowHandle> {
        self.inner.get(instance_id).map(|h| h.clone())
    }

    /// Snapshot all handles as a `Vec` suitable for Tauri command serialisation.
    /// `WindowHandle.tx` is `#[serde(skip)]` so this is safe to serialise.
    pub fn list_all(&self) -> Vec<WindowHandle> {
        self.inner.iter().map(|e| e.value().clone()).collect()
    }

    /// Send a pre-serialised JSON string to a single spoke.
    ///
    /// The shard lock is held only long enough to clone the sender — the
    /// actual `mpsc::send` happens after the lock is released.
    /// Returns `false` if the spoke is not found or the channel is closed.
    pub fn send_to(&self, instance_id: &str, msg: &str) -> bool {
        // Clone the sender out of the dashmap entry, then drop the ref.
        let tx = match self.inner.get(instance_id) {
            Some(h) => h.tx.clone(),
            None => return false,
        };
        tx.send(msg.to_string()).is_ok()
    }

    /// Close a spoke's outbound channel, causing its TCP write task to exit.
    /// Equivalent to a server-initiated disconnect.
    pub fn close(&self, instance_id: &str) -> bool {
        // Removing the handle drops the `tx` sender; the receiver in the
        // handler's select! loop gets None and exits.
        self.inner.remove(instance_id).is_some()
    }
}
