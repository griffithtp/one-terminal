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
//! - [`DashboardSession`] — which dashboards a given window has open as
//!   tabs, which one it is displaying, and whether its live layout has
//!   unsaved edits. Stays per-Terminal: a new Terminal starts with nothing
//!   open, and opens dashboards from the shared registry as it likes.

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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::persist::deserialize_channel_id"
    )]
    pub default_fdc3_channel: Option<String>,
    /// Legacy: before each Terminal window kept its own list of open
    /// dashboards (`DashboardSession::open`), "closed" was this one flag
    /// shared by every window. Now only read when migrating a Terminal
    /// saved before that (`persist::backfill_open_dashboards`); never set.
    #[serde(default)]
    pub closed: bool,
    /// Panels popped out into their own window. See
    /// [`FloatingPanel`](super::persist::FloatingPanel).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floating: Vec<super::persist::FloatingPanel>,
}

impl PersistedDashboard {
    /// Extract the layout fields (without the name) as a `PersistedLayout`.
    pub fn as_layout(&self) -> PersistedLayout {
        PersistedLayout {
            tree: self.tree.clone(),
            meta: self.meta.clone(),
            active_panel: self.active_panel.clone(),
            maximized_stack_id: self.maximized_stack_id.clone(),
            floating: self.floating.clone(),
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
            floating: layout.floating,
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
            floating: Vec::new(),
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
    /// current name here for the existing wire format.
    pub active: String,
    pub auto_save: bool,
    /// `true` when `auto_save` is off and the live layout differs from the
    /// persisted active dashboard.
    pub dirty: bool,
    /// Names of the dashboards open as tabs in *this* window, in tab order.
    pub dashboards: Vec<String>,
    /// Every other dashboard in the shared registry — not open in this
    /// window (it may be open in another), sorted by name. Surfaced so the
    /// Manage drawer and empty state can offer to open one.
    pub closed_dashboards: Vec<String>,
    /// Total count of panels across all Dashboards that are currently
    /// parked off-screen (kept alive) instead of closed, because their
    /// owning Dashboard isn't the active one. Surfaced so the switcher UI
    /// can hint at background resource usage.
    pub parked_count: usize,
    /// Dashboard name → display name of the *other* Terminal window
    /// currently holding it active (Issue 15-D's exclusivity lock). Never
    /// contains an entry for this window's own active dashboard — only
    /// dashboards locked *elsewhere* are worth showing a badge for. Keyed
    /// by name (not the internal stable id) to match every other field on
    /// this payload and avoid widening `dashboards` into a richer
    /// `{id,name}` shape just for this; resolved fresh on every snapshot,
    /// the same way `active` already is, so it's at most one broadcast
    /// tick stale if a rename races it — 15-C already re-broadcasts after
    /// every rename, so that's self-correcting.
    #[serde(default)]
    pub locked_by: HashMap<String, String>,
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
    /// The dashboard is currently active in a *different* Terminal window
    /// (Issue 15-D's exclusivity lock) — switching to it, or closing/
    /// deleting it, was blocked. `terminal_name` identifies the window that
    /// has it, so the frontend can show e.g. "open in Terminal 2". Never
    /// returned for a dashboard that's active in the *caller's own* window
    /// — a terminal is always allowed to act on its own active dashboard.
    LockedElsewhere { terminal_name: String },
    /// A runtime error that should not normally occur.
    Other { message: String },
}

impl std::fmt::Display for DashboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsConfirm => write!(f, "unsaved changes — confirm required"),
            Self::NotFound => write!(f, "dashboard not found"),
            Self::LockedElsewhere { terminal_name } => {
                write!(f, "dashboard is open in '{terminal_name}'")
            }
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
/// (via `TerminalManager::dashboards`), so creating/renaming/deleting a
/// dashboard in one window is immediately visible to every other window
/// that reads this registry. Which dashboards a window shows as tabs, and
/// in what order, is per-window — see `DashboardSession::open`.
///
/// An `IndexMap` keeps a stable order while still supporting O(1) name
/// lookups. Lookups by `id` are O(n) linear
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

    /// Every dashboard name not in `open`, sorted case-insensitively —
    /// the "other dashboards" a window can open.
    fn names_not_in(&self, open: &[String]) -> Vec<String> {
        let mut names: Vec<String> = self
            .dashboards
            .iter()
            .filter(|(_, d)| !open.contains(&d.id))
            .map(|(k, _)| k.clone())
            .collect();
        names.sort_by_key(|n| n.to_lowercase());
        names
    }

    /// Build the `wm:dashboards` payload for a given window's session —
    /// registry content (shared) combined with that window's own session
    /// state (open tabs/active/dirty/autoSave, per-window).
    pub fn snapshot(&self, session: &DashboardSession, parked_count: usize) -> DashboardsSnapshot {
        DashboardsSnapshot {
            active: self.name_of(&session.active).unwrap_or_default(),
            auto_save: session.auto_save,
            dirty: session.dirty,
            dashboards: session.open_names(self),
            closed_dashboards: self.names_not_in(&session.open),
            parked_count,
            // Lock ownership isn't known at this layer — `TerminalManager`
            // (the only thing that holds both the registry and the lock
            // map) fills this in; see `dashboards_snapshot_for`.
            locked_by: HashMap::new(),
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
            dash.floating = current.floating;
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

    /// Insert `dashboard` keeping its id, renaming it `"<name> (2)"`,
    /// `"<name> (3)"`, … if another dashboard already has its name. Names
    /// are unique per process but not across processes: two Terminals can
    /// each create a "Trading" before either sees the other's. Returns the
    /// name it was stored under.
    pub fn insert_unique(&mut self, mut dashboard: PersistedDashboard) -> String {
        if self.dashboards.contains_key(&dashboard.name) {
            let base = dashboard.name.clone();
            let mut n = 2;
            while self.dashboards.contains_key(&format!("{base} ({n})")) {
                n += 1;
            }
            dashboard.name = format!("{base} ({n})");
        }
        let name = dashboard.name.clone();
        self.dashboards.insert(name.clone(), dashboard);
        name
    }

    /// Apply changes another process made on disk
    /// (`persist::scan_registry_changes`). Dashboards in `live_ids` — active
    /// in one of this process's windows — are skipped: the window's live
    /// layout is the newer copy and will be saved over the file. Returns
    /// `true` if anything changed.
    pub fn apply_changes(
        &mut self,
        changes: Vec<super::persist::RegistryChange>,
        live_ids: &std::collections::HashSet<String>,
    ) -> bool {
        use super::persist::RegistryChange;
        let mut changed = false;
        for change in changes {
            match change {
                RegistryChange::Upserted(d) => {
                    if live_ids.contains(&d.id) {
                        continue;
                    }
                    let id = d.id.clone();
                    self.dashboards.retain(|_, existing| existing.id != id);
                    self.insert_unique(*d);
                    changed = true;
                }
                RegistryChange::Removed(id) => {
                    if live_ids.contains(&id) {
                        continue;
                    }
                    let before = self.dashboards.len();
                    self.dashboards.retain(|_, existing| existing.id != id);
                    changed |= self.dashboards.len() != before;
                }
            }
        }
        changed
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
    pub fn to_terminal_persist(
        &self,
        session: &DashboardSession,
    ) -> super::persist::TerminalPersist {
        super::persist::TerminalPersist {
            active_dashboard: self.name_of(&session.active).unwrap_or_default(),
            open_dashboards: Some(
                session
                    .open
                    .iter()
                    .filter(|id| self.get_by_id(id).is_some())
                    .cloned()
                    .collect(),
            ),
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

/// Per-Terminal-window session state: which dashboards this window has
/// open as tabs, which one it currently displays, and whether its live
/// layout has unsaved edits. The dashboards themselves live in the shared
/// `DashboardRegistry` (process-wide); only this is per-window.
pub struct DashboardSession {
    /// Stable id (see `PersistedDashboard::id`) of the dashboard this
    /// window currently displays. Empty string = no active dashboard.
    /// Deliberately identity-based rather than name-based: the registry is
    /// shared, so another window can rename the active dashboard out from
    /// under this session at any time (Issue 15-D) — an id reference
    /// survives that; a name reference would silently go stale.
    pub active: String,
    /// Stable ids of the dashboards open as tabs in this window, in tab
    /// order. Always contains `active` when that is non-empty. May hold ids
    /// another window or process has since deleted — every reader skips
    /// those (`open_names`), so deletes never need to reach into sessions.
    pub open: Vec<String>,
    /// When `true`, every layout mutation is immediately written back to the
    /// active dashboard. When `false` the user must save explicitly.
    pub auto_save: bool,
    /// Set when `auto_save` is off and the live layout has diverged from the
    /// persisted active dashboard snapshot. Cleared on save, discard, or switch.
    pub dirty: bool,
}

impl DashboardSession {
    /// Build a session from a `TerminalPersist` loaded from disk, resolving
    /// the persisted open tabs and `active_dashboard` (a name) against
    /// `registry`, which must already be loaded. A file from before
    /// per-window open lists has every dashboard not flagged `closed` open,
    /// as it did then. Falls back to the first open tab, or no active
    /// dashboard, if the saved active one is gone.
    pub fn from_persist(
        persist: &super::persist::TerminalPersist,
        registry: &DashboardRegistry,
    ) -> Self {
        let open: Vec<String> = match &persist.open_dashboards {
            Some(ids) => ids
                .iter()
                .filter(|id| registry.get_by_id(id).is_some())
                .cloned()
                .collect(),
            None => registry
                .dashboards
                .values()
                .filter(|d| !d.closed)
                .map(|d| d.id.clone())
                .collect(),
        };
        let active = registry
            .id_of(&persist.active_dashboard)
            .filter(|id| open.contains(id))
            .or_else(|| open.first().cloned())
            .unwrap_or_default();

        Self {
            active,
            open,
            auto_save: persist.auto_save,
            dirty: false,
        }
    }

    /// Build an empty session with no open or active dashboard. Used for
    /// freshly-spawned Terminal windows so the user starts with a blank slate.
    pub fn with_empty() -> Self {
        Self {
            active: String::new(),
            open: Vec::new(),
            auto_save: true,
            dirty: false,
        }
    }

    /// Names of this window's open tabs, in order, skipping any id no
    /// longer in `registry`.
    pub fn open_names(&self, registry: &DashboardRegistry) -> Vec<String> {
        self.open
            .iter()
            .filter_map(|id| registry.name_of(id))
            .collect()
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.open.iter().any(|o| o == id)
    }

    /// Add `id` as the last tab, if it isn't open already.
    pub fn open_tab(&mut self, id: &str) {
        if !id.is_empty() && !self.is_open(id) {
            self.open.push(id.to_string());
        }
    }

    /// Remove `id` from this window's tabs. Doesn't touch `active`.
    pub fn close_tab(&mut self, id: &str) {
        self.open.retain(|o| o != id);
    }

    /// Reorder tabs to match `order` (names). Tabs not mentioned keep their
    /// relative order after the reordered ones; names that aren't open
    /// here are ignored.
    pub fn reorder(&mut self, order: &[String], registry: &DashboardRegistry) {
        let mut next: Vec<String> = order
            .iter()
            .filter_map(|name| registry.id_of(name))
            .filter(|id| self.is_open(id))
            .collect();
        next.dedup();
        for id in &self.open {
            if !next.contains(id) {
                next.push(id.clone());
            }
        }
        self.open = next;
    }

    /// First open tab that still exists and isn't in `locked_elsewhere` —
    /// where this window falls back to when its active dashboard closes or
    /// is deleted. Must skip anything already active in another window
    /// (Issue 15-D), or auto-picking it would silently recreate the exact
    /// two-windows-same-dashboard conflict the lock exists to prevent.
    pub fn fallback_active(
        &self,
        registry: &DashboardRegistry,
        locked_elsewhere: &std::collections::HashSet<String>,
    ) -> String {
        self.open
            .iter()
            .find(|id| registry.get_by_id(id).is_some() && !locked_elsewhere.contains(*id))
            .cloned()
            .unwrap_or_default()
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
        assert!(
            registry.id_of("Trading").is_none(),
            "old name should no longer resolve"
        );
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
        assert_ne!(
            copy_id, source_id,
            "duplicate must not reuse the source's identity"
        );
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

        // "Window B" reads the same registry and sees it immediately — as
        // a dashboard it can open, not as one of its own tabs.
        let session_b = DashboardSession::with_empty();
        let snapshot = shared.read().unwrap().snapshot(&session_b, 0);
        assert!(snapshot.dashboards.is_empty());
        assert_eq!(
            snapshot.closed_dashboards,
            vec!["From Window A".to_string()]
        );
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

    #[test]
    fn open_tabs_are_per_session() {
        let mut registry = DashboardRegistry::new();
        registry.create("Trading".to_string());
        registry.create("Research".to_string());
        let trading = registry.id_of("Trading").unwrap();

        let mut a = DashboardSession::with_empty();
        a.open_tab(&trading);
        let b = DashboardSession::with_empty();

        let snap_a = registry.snapshot(&a, 0);
        assert_eq!(snap_a.dashboards, vec!["Trading".to_string()]);
        assert_eq!(snap_a.closed_dashboards, vec!["Research".to_string()]);

        let snap_b = registry.snapshot(&b, 0);
        assert!(
            snap_b.dashboards.is_empty(),
            "a new window starts with no tabs"
        );
        assert_eq!(
            snap_b.closed_dashboards,
            vec!["Research".to_string(), "Trading".to_string()]
        );
    }

    #[test]
    fn legacy_persist_opens_every_dashboard_not_flagged_closed() {
        let mut registry = DashboardRegistry::new();
        registry.create("Open".to_string());
        registry.create("Hidden".to_string());
        registry.dashboards.get_mut("Hidden").unwrap().closed = true;

        let persist = super::super::persist::TerminalPersist {
            active_dashboard: "Open".to_string(),
            open_dashboards: None,
            ..Default::default()
        };
        let session = DashboardSession::from_persist(&persist, &registry);
        assert_eq!(session.open_names(&registry), vec!["Open".to_string()]);
        assert_eq!(registry.name_of(&session.active).as_deref(), Some("Open"));
    }

    #[test]
    fn active_must_be_an_open_tab() {
        let mut registry = DashboardRegistry::new();
        registry.create("A".to_string());
        registry.create("B".to_string());
        let b = registry.id_of("B").unwrap();
        let persist = super::super::persist::TerminalPersist {
            active_dashboard: "A".to_string(),
            open_dashboards: Some(vec![b.clone()]),
            ..Default::default()
        };
        let session = DashboardSession::from_persist(&persist, &registry);
        assert_eq!(session.active, b);
    }

    #[test]
    fn reorder_only_moves_this_sessions_tabs() {
        let mut registry = DashboardRegistry::new();
        for n in ["A", "B", "C"] {
            registry.create(n.to_string());
        }
        let mut s = DashboardSession::with_empty();
        for n in ["A", "B", "C"] {
            s.open_tab(&registry.id_of(n).unwrap());
        }
        s.reorder(&["C".to_string(), "A".to_string()], &registry);
        assert_eq!(s.open_names(&registry), vec!["C", "A", "B"]);
    }

    #[test]
    fn insert_unique_disambiguates_a_name_from_another_process() {
        let mut registry = DashboardRegistry::new();
        registry.create("Trading".to_string());
        let theirs = PersistedDashboard::empty("Trading".to_string());
        assert_eq!(registry.insert_unique(theirs), "Trading (2)");
        assert_eq!(registry.dashboards.len(), 2);
    }

    #[test]
    fn apply_changes_keeps_dashboards_live_in_this_process() {
        use super::super::persist::RegistryChange;
        use std::collections::HashSet;

        let mut registry = DashboardRegistry::new();
        registry.create("Live".to_string());
        registry.create("Idle".to_string());
        let live = registry.id_of("Live").unwrap();
        let idle = registry.id_of("Idle").unwrap();

        let mut edited = registry.get_by_id(&live).unwrap().clone();
        edited.name = "Renamed elsewhere".to_string();
        let changed = registry.apply_changes(
            vec![
                RegistryChange::Upserted(Box::new(edited)),
                RegistryChange::Removed(idle.clone()),
            ],
            &HashSet::from([live.clone()]),
        );
        assert!(changed);
        assert_eq!(registry.name_of(&live).as_deref(), Some("Live"));
        assert!(registry.get_by_id(&idle).is_none());
    }
}
