//! Dashboard management — named, switchable layout snapshots.
//!
//! Each dashboard is an independent layout state (tree + panel metadata)
//! stored under a user-visible name. The active dashboard is what the
//! window manager currently displays.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::node::LayoutNode;
use super::persist::{PersistedLayout, PersistedLeafMeta};

// ── Persisted dashboard ───────────────────────────────────────────────────────

/// One dashboard's complete layout state with its display name.
/// This is what gets written to and read from `dashboards.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedDashboard {
    pub name: String,
    /// Root of the layout tree. `None` = empty layout.
    pub tree: Option<LayoutNode>,
    /// Per-leaf metadata keyed by webview label.
    pub meta: HashMap<String, PersistedLeafMeta>,
    /// Label of the active panel, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_panel: Option<String>,
    /// Stable id of the maximised Stack, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximized_stack_id: Option<String>,
}

impl PersistedDashboard {
    /// Extract the layout fields (without the name) as a `PersistedLayout`.
    pub fn as_layout(&self) -> PersistedLayout {
        PersistedLayout {
            tree: self.tree.clone(),
            meta: self.meta.clone(),
            active_panel: self.active_panel.clone(),
            maximized_stack_id: self.maximized_stack_id.clone(),
        }
    }

    /// Build a `PersistedDashboard` from a name and a `PersistedLayout`.
    pub fn from_layout(name: String, layout: PersistedLayout) -> Self {
        Self {
            name,
            tree: layout.tree,
            meta: layout.meta,
            active_panel: layout.active_panel,
            maximized_stack_id: layout.maximized_stack_id,
        }
    }

    pub(super) fn empty(name: String) -> Self {
        Self {
            name,
            tree: None,
            meta: HashMap::new(),
            active_panel: None,
            maximized_stack_id: None,
        }
    }
}

// ── Event payload ─────────────────────────────────────────────────────────────

/// Payload for the `wm:dashboards` event — describes the full dashboard list
/// state so the chrome can render a switcher without further round-trips.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardsSnapshot {
    pub active: String,
    pub auto_save: bool,
    /// `true` when `auto_save` is off and the live layout differs from the
    /// persisted active dashboard.
    pub dirty: bool,
    pub dashboards: Vec<String>,
}

// ── Error type ────────────────────────────────────────────────────────────────

/// Typed error returned by dashboard-mutating commands so the frontend can
/// distinguish actionable states (e.g. "show a save/discard dialog") from
/// unexpected failures.
#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum DashboardError {
    /// `auto_save` is off and the live layout has unsaved changes.
    /// The frontend should prompt the user to save or discard before switching.
    NeedsConfirm,
    /// The requested dashboard name does not exist.
    NotFound,
    /// A runtime error that should not normally occur.
    Other { message: String },
}

impl std::fmt::Display for DashboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsConfirm => write!(f, "unsaved changes — confirm required"),
            Self::NotFound => write!(f, "dashboard not found"),
            Self::Other { message } => write!(f, "{message}"),
        }
    }
}

impl From<String> for DashboardError {
    fn from(message: String) -> Self {
        Self::Other { message }
    }
}

// ── DashboardStore ────────────────────────────────────────────────────────────

/// In-memory collection of dashboards for one Terminal window.
///
/// An `IndexMap` is used so dashboards appear in insertion/user-defined order
/// while still supporting O(1) name lookups.
pub struct DashboardStore {
    /// Name of the currently active dashboard.
    pub active: String,
    /// When `true`, every layout mutation is immediately written back to the
    /// active dashboard. When `false` the user must save explicitly.
    pub auto_save: bool,
    /// Set when `auto_save` is off and the live layout has diverged from the
    /// persisted active dashboard snapshot. Cleared on save, discard, or switch.
    pub dirty: bool,
    pub dashboards: IndexMap<String, PersistedDashboard>,
}

impl DashboardStore {
    /// Build from a `TerminalPersist` loaded from disk.
    pub fn from_persist(persist: super::persist::TerminalPersist) -> Self {
        let mut dashboards: IndexMap<String, PersistedDashboard> =
            IndexMap::with_capacity(persist.dashboards.len());
        for d in persist.dashboards {
            dashboards.insert(d.name.clone(), d);
        }

        let active = if dashboards.contains_key(&persist.active_dashboard) {
            persist.active_dashboard
        } else if let Some(first) = dashboards.keys().next() {
            first.clone()
        } else {
            let name = "Default".to_string();
            dashboards.insert(name.clone(), PersistedDashboard::empty(name.clone()));
            name
        };

        Self {
            active,
            auto_save: persist.auto_save,
            dirty: false,
            dashboards,
        }
    }

    /// Create a store with a single empty dashboard called "Default".
    /// Used as the initial state before `init` loads from disk.
    pub fn with_default() -> Self {
        let name = "Default".to_string();
        let mut dashboards = IndexMap::new();
        dashboards.insert(name.clone(), PersistedDashboard::empty(name.clone()));
        Self {
            active: name,
            auto_save: true,
            dirty: false,
            dashboards,
        }
    }

    // ── Read methods ──────────────────────────────────────────────────────────

    /// Dashboard names in their current display order.
    pub fn list(&self) -> Vec<String> {
        self.dashboards.keys().cloned().collect()
    }

    /// Snapshot of the full dashboard state for the `wm:dashboards` event.
    pub fn as_snapshot(&self) -> DashboardsSnapshot {
        DashboardsSnapshot {
            active: self.active.clone(),
            auto_save: self.auto_save,
            dirty: self.dirty,
            dashboards: self.list(),
        }
    }

    /// Return the layout fields for the active dashboard, if it exists.
    pub fn load_active(&self) -> Option<PersistedLayout> {
        self.dashboards.get(&self.active).map(|d| d.as_layout())
    }

    /// Returns `true` if `current` differs from the persisted active dashboard.
    /// Comparison is done via JSON serialisation — cheap enough for infrequent
    /// dirty-check calls.
    #[allow(dead_code)]
    pub fn is_dirty(&self, current: &PersistedLayout) -> bool {
        let Some(dash) = self.dashboards.get(&self.active) else {
            return true;
        };
        let stored = dash.as_layout();
        serde_json::to_string(current).ok() != serde_json::to_string(&stored).ok()
    }

    // ── Mutation methods ──────────────────────────────────────────────────────

    /// Overwrite the active dashboard's layout fields with `current`.
    pub fn snapshot_current(&mut self, current: PersistedLayout) {
        if let Some(dash) = self.dashboards.get_mut(&self.active) {
            dash.tree = current.tree;
            dash.meta = current.meta;
            dash.active_panel = current.active_panel;
            dash.maximized_stack_id = current.maximized_stack_id;
        }
    }

    /// Create a new empty dashboard with `name`. Returns `false` if the name
    /// is already taken.
    pub fn create(&mut self, name: String) -> bool {
        if self.dashboards.contains_key(&name) {
            return false;
        }
        self.dashboards
            .insert(name.clone(), PersistedDashboard::empty(name));
        true
    }

    /// Rename a dashboard in place, preserving its position in the list.
    /// Returns `false` if `old` doesn't exist or `new` is already taken.
    pub fn rename(&mut self, old: &str, new: String) -> bool {
        if !self.dashboards.contains_key(old) || self.dashboards.contains_key(&new) {
            return false;
        }
        // IndexMap has no rename-in-place; rebuild preserving order.
        let mut next: IndexMap<String, PersistedDashboard> =
            IndexMap::with_capacity(self.dashboards.len());
        for (k, mut v) in self.dashboards.drain(..) {
            if k == old {
                v.name = new.clone();
                next.insert(new.clone(), v);
            } else {
                next.insert(k, v);
            }
        }
        self.dashboards = next;
        if self.active == old {
            self.active = new;
        }
        true
    }

    /// Delete a dashboard. The last dashboard cannot be deleted.
    /// Returns `false` if `name` doesn't exist or is the only dashboard.
    pub fn delete(&mut self, name: &str) -> bool {
        if self.dashboards.len() <= 1 || !self.dashboards.contains_key(name) {
            return false;
        }
        self.dashboards.shift_remove(name);
        if self.active == name {
            if let Some(first) = self.dashboards.keys().next() {
                self.active = first.clone();
            }
        }
        true
    }

    /// Reorder dashboards to match `order`. Names absent from the store are
    /// ignored; names in the store absent from `order` are dropped.
    pub fn reorder(&mut self, order: &[String]) {
        let mut next: IndexMap<String, PersistedDashboard> =
            IndexMap::with_capacity(order.len());
        for name in order {
            if let Some(d) = self.dashboards.shift_remove(name) {
                next.insert(name.clone(), d);
            }
        }
        self.dashboards = next;
    }

    // ── Serialisation ─────────────────────────────────────────────────────────

    /// Convert to `TerminalPersist` for writing to disk.
    pub fn to_terminal_persist(&self) -> super::persist::TerminalPersist {
        super::persist::TerminalPersist {
            active_dashboard: self.active.clone(),
            auto_save: self.auto_save,
            dashboards: self.dashboards.values().cloned().collect(),
        }
    }
}
