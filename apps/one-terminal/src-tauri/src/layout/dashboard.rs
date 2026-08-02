//! Dashboard management — named, switchable layout snapshots.
//!
//! Each dashboard is an independent layout state (tree + panel metadata)
//! stored under a user-visible name. The active dashboard is what the
//! window manager currently displays.
//!
//! Plan 15 splits what used to be a single per-Terminal `DashboardStore`
//! into two pieces living at different scopes:
//! - [`DashboardRegistry`] — the dashboards themselves. Shared process-wide:
//!   one instance, referenced by every Terminal window's `LayoutTree` via
//!   `TerminalManager`.
//! - [`DashboardSession`] — which dashboard a given window is displaying,
//!   and whether its live layout has unsaved edits. Stays per-Terminal.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::node::LayoutNode;
use super::persist::{PersistedLayout, PersistedLeafMeta};

fn new_dashboard_id() -> String {
    Uuid::new_v4().to_string()
}

// ── Persisted dashboard ───────────────────────────────────────────────────────

/// One dashboard's complete layout state with its display name.
/// This is what gets written to and read from `dashboards.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedDashboard {
    /// Stable identity, independent of `name` — survives renames. Generated
    /// once when the dashboard is created and never changed afterward.
    /// Dashboards persisted before this field existed have none on disk;
    /// `#[serde(default = ...)]` mints one on load so every in-memory
    /// `PersistedDashboard` always has an id, but that's only a stopgap —
    /// Issue 15-B's migration is what backfills a *stable* id that survives
    /// process restarts for pre-existing dashboards.
    #[serde(default = "new_dashboard_id")]
    pub id: String,
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
    /// FDC3 user channel newly-added widgets in this dashboard join by
    /// default. `None` = no default (widgets are born on no channel, as
    /// before this field existed). Set via the dashboard tab context menu's
    /// "Set default channel…" item, which also applies the channel to every
    /// widget already in the dashboard at the time it's set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_fdc3_channel: Option<String>,
    /// When `true`, this dashboard is hidden from the switcher and Manage
    /// drawer's main list — "closed" rather than deleted. Its layout, meta,
    /// and default channel are untouched; `DashboardRegistry::reopen` clears
    /// this flag to bring it back. Distinct from deletion
    /// (`DashboardRegistry::delete`), which removes the entry entirely and
    /// cannot be undone.
    #[serde(default)]
    pub closed: bool,
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
            id: new_dashboard_id(),
            name,
            tree: layout.tree,
            meta: layout.meta,
            active_panel: layout.active_panel,
            maximized_stack_id: layout.maximized_stack_id,
            default_fdc3_channel: None,
            closed: false,
        }
    }

    pub(super) fn empty(name: String) -> Self {
        Self {
            id: new_dashboard_id(),
            name,
            tree: None,
            meta: HashMap::new(),
            active_panel: None,
            maximized_stack_id: None,
            default_fdc3_channel: None,
            closed: false,
        }
    }
}

// ── Event payload ─────────────────────────────────────────────────────────────

/// Payload for the `wm:dashboards` event — describes the full dashboard list
/// state so the chrome can render a switcher without further round-trips.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardsSnapshot {
    /// Display name of this window's active dashboard (empty = none).
    /// Internally, `DashboardSession` tracks the same dashboard by stable
    /// `id` rather than name (Issue 15-A) so a rename issued by another
    /// window never orphans a session; this field is resolved back to the
    /// current name here for the existing wire format. Issue 15-E may widen
    /// this payload to also carry the id once the frontend needs it (e.g.
    /// for id-keyed lock indicators).
    pub active: String,
    pub auto_save: bool,
    /// `true` when `auto_save` is off and the live layout differs from the
    /// persisted active dashboard.
    pub dirty: bool,
    pub dashboards: Vec<String>,
    /// Names of dashboards hidden via `DashboardRegistry::close` — not
    /// deleted, just absent from `dashboards` above. Surfaced so the Manage
    /// drawer can list them with a "Reopen" action.
    pub closed_dashboards: Vec<String>,
    /// Total count of panels across all Dashboards that are currently
    /// parked off-screen (kept alive) instead of closed, because their
    /// owning Dashboard isn't the active one. Surfaced so the switcher UI
    /// can hint at background resource usage.
    pub parked_count: usize,
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

// ── DashboardRegistry ─────────────────────────────────────────────────────────

/// Shared, process-wide collection of dashboards. Every Terminal window's
/// `LayoutTree` holds a clone of the same `Arc<RwLock<DashboardRegistry>>`
/// (via `TerminalManager::dashboards`), so creating/renaming/closing/
/// deleting/reordering a dashboard in one window is immediately visible to
/// every other window that reads this registry.
///
/// An `IndexMap` is used so dashboards appear in insertion/user-defined order
/// while still supporting O(1) name lookups. Lookups by `id` are O(n) linear
/// scans over the map — dashboard counts are small (tens, not thousands) and
/// identity mutations (create/rename/delete) are infrequent, so a redundant
/// id→name index isn't worth the risk of it drifting out of sync with the map.
#[derive(Default)]
pub struct DashboardRegistry {
    pub dashboards: IndexMap<String, PersistedDashboard>,
}

impl DashboardRegistry {
    pub fn new() -> Self {
        Self {
            dashboards: IndexMap::new(),
        }
    }

    // ── Identity resolution ────────────────────────────────────────────────

    /// Resolve a dashboard's stable id to its current display name.
    /// Returns `None` for an empty id (the "no active dashboard" sentinel)
    /// or an id that no longer exists.
    pub fn name_of(&self, id: &str) -> Option<String> {
        if id.is_empty() {
            return None;
        }
        self.dashboards
            .values()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
    }

    /// Resolve a dashboard's current display name to its stable id.
    pub fn id_of(&self, name: &str) -> Option<String> {
        self.dashboards.get(name).map(|d| d.id.clone())
    }

    /// Look up a dashboard by its stable id.
    pub fn get_by_id(&self, id: &str) -> Option<&PersistedDashboard> {
        if id.is_empty() {
            return None;
        }
        self.dashboards.values().find(|d| d.id == id)
    }

    /// Name of the first open (non-closed) dashboard in store order, if any.
    /// Used by callers reassigning a session's `active` after the dashboard
    /// it pointed at closes or is deleted.
    pub fn first_open_name(&self) -> Option<String> {
        self.dashboards
            .iter()
            .find(|(_, d)| !d.closed)
            .map(|(k, _)| k.clone())
    }

    // ── Read methods ──────────────────────────────────────────────────────────

    /// Open dashboard names in their current display order — what the
    /// switcher and Manage drawer's main list show. Closed dashboards are
    /// excluded; see `list_closed`.
    pub fn list(&self) -> Vec<String> {
        self.dashboards
            .iter()
            .filter(|(_, d)| !d.closed)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Closed dashboard names, in store order. Surfaced so the Manage
    /// drawer can offer a "Reopen" action for each.
    pub fn list_closed(&self) -> Vec<String> {
        self.dashboards
            .iter()
            .filter(|(_, d)| d.closed)
            .map(|(k, _)| k.clone())
            .collect()
    }

    /// Build the `wm:dashboards` payload for a given window's session —
    /// registry content (shared) combined with that window's own session
    /// state (active/dirty/autoSave, per-window).
    pub fn snapshot(&self, session: &DashboardSession, parked_count: usize) -> DashboardsSnapshot {
        DashboardsSnapshot {
            active: self.name_of(&session.active).unwrap_or_default(),
            auto_save: session.auto_save,
            dirty: session.dirty,
            dashboards: self.list(),
            closed_dashboards: self.list_closed(),
            parked_count,
        }
    }

    // ── Mutation methods ────────────────────────────────────────────────────
    //
    // These are content-only — none of them touch any window's session
    // `active`. Callers (`LayoutTree`) that need "is `name` my own active
    // dashboard, and if so what do I do about it" resolve that separately
    // against their own `DashboardSession`.

    /// Overwrite dashboard `id`'s layout fields with `current`. No-op if no
    /// dashboard has that id (e.g. the session has no active dashboard).
    pub fn snapshot_current(&mut self, id: &str, current: PersistedLayout) {
        if let Some(dash) = self.dashboards.values_mut().find(|d| d.id == id) {
            dash.tree = current.tree;
            dash.meta = current.meta;
            dash.active_panel = current.active_panel;
            dash.maximized_stack_id = current.maximized_stack_id;
        }
    }

    /// Create a new dashboard pre-populated with `layout` under `name`,
    /// always under a fresh stable id (even if `layout` already carried
    /// one — e.g. duplicating a dashboard must never reuse the source's
    /// identity). Returns `false` if `name` is already taken.
    pub fn create_from(&mut self, name: String, layout: PersistedDashboard) -> bool {
        if self.dashboards.contains_key(&name) {
            return false;
        }
        let mut d = layout;
        d.name = name.clone();
        d.id = new_dashboard_id();
        self.dashboards.insert(name, d);
        true
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

    /// Rename a dashboard in place, preserving its position in the list and
    /// its stable `id`. Returns `false` if `old` doesn't exist or `new` is
    /// already taken.
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
        true
    }

    /// Permanently delete a dashboard — its layout, meta, and default
    /// channel are gone for good. Works on open or closed dashboards alike.
    /// See `close` for the non-destructive, reopenable alternative used by
    /// the dashboard tab's "Close dashboard" action.
    /// Returns `false` if `name` doesn't exist.
    pub fn delete(&mut self, name: &str) -> bool {
        self.dashboards.shift_remove(name).is_some()
    }

    /// Hide a dashboard from the switcher/Manage-drawer main list without
    /// deleting it. No-op success if `name` is already closed. Returns
    /// `false` if `name` doesn't exist.
    pub fn close(&mut self, name: &str) -> bool {
        let Some(dash) = self.dashboards.get_mut(name) else {
            return false;
        };
        dash.closed = true;
        true
    }

    /// Make a closed dashboard selectable again. Returns `false` if `name`
    /// doesn't exist.
    pub fn reopen(&mut self, name: &str) -> bool {
        let Some(dash) = self.dashboards.get_mut(name) else {
            return false;
        };
        dash.closed = false;
        true
    }

    /// Set (or clear) the default FDC3 channel for `name` and apply it to
    /// every widget currently persisted in that dashboard's `meta` map —
    /// active or not. When `name` is some window's active dashboard, that
    /// caller is still responsible for mirroring this into its live
    /// `LayoutTree` (`meta` here only reflects state, it isn't the live
    /// source of truth while the dashboard is active). Returns `false` if
    /// `name` doesn't exist.
    pub fn set_default_channel(&mut self, name: &str, channel: Option<String>) -> bool {
        let Some(dash) = self.dashboards.get_mut(name) else {
            return false;
        };
        dash.default_fdc3_channel = channel.clone();
        for meta in dash.meta.values_mut() {
            meta.fdc3_channel = channel.clone();
        }
        true
    }

    /// Bulk-set the keep-alive flag for every widget currently persisted in
    /// `name`'s `meta` map — active or not. Same active-dashboard caveat as
    /// `set_default_channel`. Returns `false` if `name` doesn't exist.
    pub fn set_all_keep_alive(&mut self, name: &str, keep_alive: bool) -> bool {
        let Some(dash) = self.dashboards.get_mut(name) else {
            return false;
        };
        for meta in dash.meta.values_mut() {
            meta.keep_alive = keep_alive;
        }
        true
    }

    /// Reorder dashboards to match `order` (the switcher/drawer only ever
    /// reorders *open* dashboards). Names in `order` absent from the store
    /// are ignored. Any dashboard not mentioned in `order` — i.e. every
    /// closed dashboard, which the reordering UI never sees — is preserved,
    /// appended after the reordered ones in its prior relative order, rather
    /// than being dropped from the store.
    pub fn reorder(&mut self, order: &[String]) {
        let mut next: IndexMap<String, PersistedDashboard> = IndexMap::with_capacity(order.len());
        for name in order {
            if let Some(d) = self.dashboards.shift_remove(name) {
                next.insert(name.clone(), d);
            }
        }
        for (k, v) in self.dashboards.drain(..) {
            next.insert(k, v);
        }
        self.dashboards = next;
    }

    // ── Serialisation ─────────────────────────────────────────────────────────

    /// Convert this window's session into a `TerminalPersist` for writing to
    /// its own per-terminal file — resolves the session's active `id` back
    /// to a name (the persisted format's `active_dashboard`). Carries no
    /// dashboard *content*; see `to_persisted` for that (Issue 15-B —
    /// dashboard content lives in the one shared `dashboards.json`, not
    /// duplicated per terminal). Callers that do a full write use
    /// `persist::save_terminal_dashboards`, which read-modify-writes the
    /// file so that `name` and `window` are preserved.
    pub fn to_terminal_persist(&self, session: &DashboardSession) -> super::persist::TerminalPersist {
        super::persist::TerminalPersist {
            active_dashboard: self.name_of(&session.active).unwrap_or_default(),
            auto_save: session.auto_save,
            ..Default::default()
        }
    }

    /// Convert to `PersistedDashboardRegistry` for writing to the one shared
    /// `<data_dir>/dashboards.json` (Issue 15-B).
    pub fn to_persisted(&self) -> super::persist::PersistedDashboardRegistry {
        super::persist::PersistedDashboardRegistry {
            dashboards: self.dashboards.values().cloned().collect(),
        }
    }
}

// ── DashboardSession ──────────────────────────────────────────────────────────

/// Per-Terminal-window session state: which dashboard this window is
/// currently displaying, and whether its live layout has unsaved edits.
/// The dashboards themselves live in the shared `DashboardRegistry`
/// (process-wide); only this is per-window.
pub struct DashboardSession {
    /// Stable id (see `PersistedDashboard::id`) of the dashboard this
    /// window currently displays. Empty string = no active dashboard.
    /// Deliberately identity-based rather than name-based: the registry is
    /// shared, so another window can rename the active dashboard out from
    /// under this session at any time (Issue 15-D) — an id reference
    /// survives that; a name reference would silently go stale.
    pub active: String,
    /// When `true`, every layout mutation is immediately written back to the
    /// active dashboard. When `false` the user must save explicitly.
    pub auto_save: bool,
    /// Set when `auto_save` is off and the live layout has diverged from the
    /// persisted active dashboard snapshot. Cleared on save, discard, or switch.
    pub dirty: bool,
}

impl DashboardSession {
    /// Build a session from a `TerminalPersist` loaded from disk, resolving
    /// the persisted `active_dashboard` (a name) to that dashboard's id via
    /// `registry` — the registry must already contain this terminal's
    /// dashboards (merged in by `LayoutTree::init`) by the time this runs.
    /// Falls back to the first open dashboard in `registry`, or no active
    /// dashboard, exactly as the pre-split `DashboardStore::from_persist` did.
    pub fn from_persist(
        persist: &super::persist::TerminalPersist,
        registry: &DashboardRegistry,
    ) -> Self {
        let active = registry
            .dashboards
            .get(&persist.active_dashboard)
            .filter(|d| !d.closed)
            .map(|d| d.id.clone())
            .or_else(|| {
                registry
                    .dashboards
                    .values()
                    .find(|d| !d.closed)
                    .map(|d| d.id.clone())
            })
            .unwrap_or_default();

        Self {
            active,
            auto_save: persist.auto_save,
            dirty: false,
        }
    }

    /// Build an empty session with no active dashboard. Used for
    /// freshly-spawned Terminal windows so the user starts with a blank slate.
    pub fn with_empty() -> Self {
        Self {
            active: String::new(),
            auto_save: true,
            dirty: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, RwLock};
    use std::thread;

    use super::*;

    #[test]
    fn every_created_dashboard_has_a_stable_id() {
        let mut registry = DashboardRegistry::new();
        assert!(registry.create("Trading".to_string()));
        let id = registry.id_of("Trading").expect("dashboard should exist");
        assert!(!id.is_empty());
    }

    #[test]
    fn rename_preserves_id_and_updates_the_name_lookup() {
        let mut registry = DashboardRegistry::new();
        registry.create("Trading".to_string());
        let id_before = registry.id_of("Trading").unwrap();

        assert!(registry.rename("Trading", "Trading (EU)".to_string()));

        let id_after = registry.id_of("Trading (EU)").unwrap();
        assert_eq!(id_before, id_after, "rename must not change identity");
        assert!(registry.id_of("Trading").is_none(), "old name should no longer resolve");
        assert_eq!(registry.name_of(&id_after).as_deref(), Some("Trading (EU)"));
    }

    #[test]
    fn rename_never_orphans_another_sessions_active_reference() {
        // Simulates window B holding "Trading" active/locked while window A
        // renames it — the exact scenario flagged in review as a gap the
        // stable id is meant to close (see Plan 15, Issue 15-A step 7).
        let mut registry = DashboardRegistry::new();
        registry.create("Trading".to_string());
        let mut session_b = DashboardSession::with_empty();
        session_b.active = registry.id_of("Trading").unwrap();

        registry.rename("Trading", "Trading (EU)".to_string());

        // Window B's session still resolves to a real, current dashboard.
        assert_eq!(
            registry.name_of(&session_b.active).as_deref(),
            Some("Trading (EU)")
        );
        assert!(registry.get_by_id(&session_b.active).is_some());
    }

    #[test]
    fn duplicate_gets_a_fresh_id_not_the_sources() {
        let mut registry = DashboardRegistry::new();
        registry.create("Trading".to_string());
        let source = registry.dashboards.get("Trading").unwrap().clone();
        let source_id = source.id.clone();

        assert!(registry.create_from("Trading (copy)".to_string(), source));

        let copy_id = registry.id_of("Trading (copy)").unwrap();
        assert_ne!(copy_id, source_id, "duplicate must not reuse the source's identity");
    }

    #[test]
    fn deserializing_a_pre_plan_15_dashboard_backfills_an_id() {
        // Dashboards persisted before this field existed have no `id` on
        // disk — confirms the interim `#[serde(default = ...)]` stopgap
        // (Issue 15-B's migration is what makes this permanent).
        let json = r#"{"name":"Legacy","tree":null,"meta":{}}"#;
        let d: PersistedDashboard = serde_json::from_str(json).unwrap();
        assert!(!d.id.is_empty());
        assert_eq!(d.name, "Legacy");
    }

    #[test]
    fn registry_shared_across_two_sessions_is_actually_shared() {
        // Stand-in for "two Terminal windows both reading wm_list_dashboards
        // see the same registry" (Issue 15-A acceptance criteria) without
        // needing a full Tauri harness — both sessions below hold the same
        // `Arc<RwLock<DashboardRegistry>>`, exactly as two `LayoutTree`s do
        // via `TerminalManager::dashboards()`.
        let shared = Arc::new(RwLock::new(DashboardRegistry::new()));

        shared.write().unwrap().create("From Window A".to_string());

        // "Window B" reads the same registry and sees it immediately.
        let seen_by_b = shared.read().unwrap().list();
        assert_eq!(seen_by_b, vec!["From Window A".to_string()]);
    }

    #[test]
    fn no_deadlock_under_concurrent_create_and_rename() {
        // Basic stress test for Issue 15-A's "no deadlock under concurrent
        // access from two windows" acceptance criterion.
        let shared = Arc::new(RwLock::new(DashboardRegistry::new()));
        let mut handles = Vec::new();
        for i in 0..8 {
            let shared = Arc::clone(&shared);
            handles.push(thread::spawn(move || {
                let name = format!("Dash {i}");
                shared.write().unwrap().create(name.clone());
                shared
                    .write()
                    .unwrap()
                    .rename(&name, format!("Dash {i} (renamed)"));
            }));
        }
        for h in handles {
            h.join().expect("worker thread should not panic");
        }
        assert_eq!(shared.read().unwrap().dashboards.len(), 8);
    }
}
