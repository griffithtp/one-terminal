use dashmap::DashMap;
use std::sync::Arc;

/// Tracks which local spoke instances are registered as handlers for each intent.
///
/// Cheap to clone — all state is behind `Arc<DashMap<...>>`.
#[derive(Clone)]
pub struct IntentRegistry {
    /// `intent_name` → ordered `Vec<instance_id>` (first-registered-wins on delivery)
    inner: Arc<DashMap<String, Vec<String>>>,
}

impl IntentRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Register `instance_id` as a handler for `intent`.
    pub fn add(&self, instance_id: &str, intent: &str) {
        self.inner
            .entry(intent.to_string())
            .and_modify(|v| {
                if !v.contains(&instance_id.to_string()) {
                    v.push(instance_id.to_string());
                }
            })
            .or_insert_with(|| vec![instance_id.to_string()]);
    }

    /// Deregister `instance_id` as a handler for `intent`.
    pub fn remove(&self, instance_id: &str, intent: &str) {
        if let Some(mut entry) = self.inner.get_mut(intent) {
            entry.retain(|id| id != instance_id);
        }
    }

    /// Remove all intent registrations for `instance_id` (called on spoke disconnect).
    pub fn remove_all(&self, instance_id: &str) {
        for mut entry in self.inner.iter_mut() {
            entry.value_mut().retain(|id| id != instance_id);
        }
    }

    /// Return the `instance_id` of the first registered handler for `intent`, if any.
    pub fn find_handler(&self, intent: &str) -> Option<String> {
        self.inner
            .get(intent)
            .and_then(|v| v.first().cloned())
    }

    /// Return all (intent, Vec<instance_id>) pairs with at least one handler.
    /// Used by the dashboard InteropDashboard intent resolver.
    pub fn list_all(&self) -> Vec<(String, Vec<String>)> {
        self.inner
            .iter()
            .filter(|e| !e.value().is_empty())
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }
}
