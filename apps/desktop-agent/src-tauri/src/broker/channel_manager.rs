use dashmap::DashMap;
use std::collections::HashMap;
use std::sync::Arc;

use super::types::{ChannelInfo, ChannelInfoSummary};

/// Thread-safe, clone-safe registry of FDC3 user channels and their
/// spoke memberships.
///
/// Uses `DashMap` so different channels can be mutated concurrently without
/// contending on a single global lock.
#[derive(Clone)]
pub struct ChannelManager {
    /// channel_id → ChannelInfo
    inner: Arc<DashMap<String, ChannelInfo>>,
}

impl ChannelManager {
    /// Create an empty `ChannelManager`. Call [`create`] to seed channels from config.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Insert a channel. No-op if a channel with this id already exists.
    pub fn create(&self, id: &str, display_name: &str, color: &str) {
        self.inner
            .entry(id.to_string())
            .or_insert_with(|| ChannelInfo {
                channel_id: id.to_string(),
                display_name: display_name.to_string(),
                color: color.to_string(),
                members: Vec::new(),
                last_context: HashMap::new(),
            });
    }

    /// Join a spoke to a channel, removing it from its previous channel first.
    /// Returns `Err` if `channel_id` is not a known channel.
    pub fn join(&self, instance_id: &str, channel_id: &str) -> Result<(), String> {
        if !self.inner.contains_key(channel_id) {
            return Err(format!("Unknown channel: {channel_id}"));
        }
        // Remove from any previous channel first.
        self.leave(instance_id);
        // Add to the new channel.
        if let Some(mut ch) = self.inner.get_mut(channel_id) {
            if !ch.members.contains(&instance_id.to_string()) {
                ch.members.push(instance_id.to_string());
            }
        }
        Ok(())
    }

    /// Remove a spoke from its current channel (if any).
    pub fn leave(&self, instance_id: &str) {
        for mut entry in self.inner.iter_mut() {
            entry.members.retain(|id| id != instance_id);
        }
    }

    /// Remove a spoke from all channels — called on disconnect.
    pub fn remove_member(&self, instance_id: &str) {
        self.leave(instance_id);
    }

    /// Return all `instance_id`s on a channel *except* the given one.
    /// Used by broadcast fan-out to collect targets without holding any
    /// write lock during the sends.
    pub fn members_except(&self, channel_id: &str, exclude: &str) -> Vec<String> {
        self.inner
            .get(channel_id)
            .map(|ch| {
                ch.members
                    .iter()
                    .filter(|id| id.as_str() != exclude)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Update the last-value cache for a channel.
    pub fn set_last_context(&self, channel_id: &str, context_type: &str, value: serde_json::Value) {
        if let Some(mut ch) = self.inner.get_mut(channel_id) {
            ch.last_context.insert(context_type.to_string(), value);
        }
    }

    /// Return the last-value context for a specific type on a channel, if any.
    #[allow(dead_code)]
    pub fn get_last_context(
        &self,
        channel_id: &str,
        context_type: &str,
    ) -> Option<serde_json::Value> {
        self.inner
            .get(channel_id)
            .and_then(|ch| ch.last_context.get(context_type).cloned())
    }

    /// Lightweight summaries for sending in the `Welcome` message.
    pub fn list_summaries(&self) -> Vec<ChannelInfoSummary> {
        let mut summaries: Vec<ChannelInfoSummary> = self
            .inner
            .iter()
            .map(|e| ChannelInfoSummary {
                channel_id: e.channel_id.clone(),
                display_name: e.display_name.clone(),
                color: e.color.clone(),
            })
            .collect();
        // Stable ordering for the dashboard and Welcome message.
        summaries.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        summaries
    }

    /// Full `ChannelInfo` snapshots for Tauri commands.
    pub fn list_all(&self) -> Vec<ChannelInfo> {
        let mut channels: Vec<ChannelInfo> = self.inner.iter().map(|e| e.value().clone()).collect();
        channels.sort_by(|a, b| a.channel_id.cmp(&b.channel_id));
        channels
    }
}
