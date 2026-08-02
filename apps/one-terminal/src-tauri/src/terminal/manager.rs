//! `TerminalManager` — global registry of all open Terminal OS windows.
//!
//! Registered once via `.manage()`. Layout commands look up their per-terminal
//! objects (`LayoutTree`, `WebviewPool`, `OverlayState`) by the invoking
//! window's label instead of injecting them as separate global `State` values.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use indexmap::IndexMap;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::layout::dashboard::DashboardRegistry;

use super::state::TerminalState;

// ── Internal storage ──────────────────────────────────────────────────────────

struct ManagerInner {
    /// Open terminals in creation order.
    terminals: IndexMap<String, Arc<TerminalState>>,
    /// Monotonically increasing counter for generating unique terminal IDs.
    /// Starts at 2; "terminal-main" occupies the implied slot 1.
    next_id: u32,
    /// In-memory-only active-dashboard exclusivity lock (Issue 15-D):
    /// dashboard id → id of the terminal currently displaying it. A
    /// dashboard can be active in at most one window at a time. Never
    /// persisted — killing the process drops every lock for free, so
    /// there's no "stale lock survived a crash" state to recover.
    active_locks: HashMap<String, String>,
}

// ── TerminalListItem ──────────────────────────────────────────────────────────

/// Rich Terminal descriptor — returned by `wm_list_terminals`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalListItem {
    pub id: String,
    pub name: String,
    pub active_dashboard: String,
    pub dashboard_count: usize,
}

// ── TerminalManager ───────────────────────────────────────────────────────────

pub struct TerminalManager {
    inner: Arc<RwLock<ManagerInner>>,
    /// Shared, process-wide dashboard registry — every `LayoutTree`
    /// constructed via `terminal::spawn` (and the main window's, in `lib.rs`)
    /// shares this same instance via `dashboards()` (Issue 15-A).
    dashboards: Arc<RwLock<DashboardRegistry>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ManagerInner {
                terminals: IndexMap::new(),
                next_id: 2,
                active_locks: HashMap::new(),
            })),
            dashboards: Arc::new(RwLock::new(DashboardRegistry::new())),
        }
    }

    /// Shared dashboard registry handle, passed into `LayoutTree::new` so
    /// every Terminal window's layout tree reads/writes the same dashboards.
    pub fn dashboards(&self) -> Arc<RwLock<DashboardRegistry>> {
        Arc::clone(&self.dashboards)
    }

    /// Load the shared dashboard registry from `<data_dir>/dashboards.json`,
    /// running the one-time Issue 15-B migration first if it doesn't exist
    /// yet but pre-existing per-terminal dashboard lists do. Must be called
    /// once, before any `LayoutTree::init` runs, so every terminal's session
    /// resolves its `active_dashboard` against fully-populated content.
    pub fn load_dashboards_registry(&self, data_dir: &std::path::Path) {
        let persisted = crate::layout::persist::load_or_migrate_registry(data_dir);
        let mut registry = self.dashboards.write().unwrap();
        for d in persisted.dashboards {
            registry.dashboards.insert(d.name.clone(), d);
        }
    }

    /// Register a freshly-spawned terminal. Replaces any existing entry with
    /// the same id (should not occur in normal usage).
    pub fn register(&self, state: Arc<TerminalState>) {
        self.inner
            .write()
            .unwrap()
            .terminals
            .insert(state.id.clone(), state);
    }

    /// Look up a terminal by its window label. Returns `None` if the label is
    /// not registered (e.g. the window was already closed).
    pub fn get(&self, id: &str) -> Option<Arc<TerminalState>> {
        self.inner.read().unwrap().terminals.get(id).cloned()
    }

    /// Remove and return a terminal entry. Used when a Terminal window closes.
    pub fn remove(&self, id: &str) -> Option<Arc<TerminalState>> {
        self.inner.write().unwrap().terminals.shift_remove(id)
    }

    /// All registered terminals in creation order.
    pub fn list(&self) -> Vec<Arc<TerminalState>> {
        self.inner
            .read()
            .unwrap()
            .terminals
            .values()
            .cloned()
            .collect()
    }

    /// Generate the next unique terminal window label (`"terminal-2"`,
    /// `"terminal-3"`, …). Increments the internal counter atomically.
    pub fn next_label(&self) -> String {
        let mut g = self.inner.write().unwrap();
        let id = g.next_id;
        g.next_id += 1;
        format!("terminal-{id}")
    }

    /// Build a `TerminalListItem` for every registered terminal and emit
    /// `wm:terminals` on `app`. Called after any change to the terminal list.
    pub fn emit_terminals(&self, app: &AppHandle) {
        let items: Vec<TerminalListItem> = self
            .list()
            .into_iter()
            .map(|t| {
                let name = t.name.read().unwrap().clone();
                let ds = t.layout_tree.dashboards_snapshot();
                TerminalListItem {
                    id: t.id.clone(),
                    name,
                    active_dashboard: ds.active,
                    dashboard_count: ds.dashboards.len(),
                }
            })
            .collect();
        let _ = app.emit("wm:terminals", &items);
    }

    /// Broadcast `wm:dashboards` to every registered terminal's chrome
    /// webview (Issue 15-C). Call this — instead of a single terminal's own
    /// `LayoutTree::emit_dashboards` — after any dashboard *registry*
    /// mutation (create/rename/close/reopen/delete/duplicate/reorder):
    /// since the registry is shared, every window's switcher and Manage
    /// drawer needs to reflect it immediately, not just the window that
    /// issued the command.
    ///
    /// This computes and emits each terminal's own `dashboards_snapshot()`
    /// individually rather than one shared payload — `DashboardsSnapshot`
    /// bundles registry-scoped fields (`dashboards`, `closedDashboards`,
    /// shared) with session-scoped ones (`active`, `dirty`, `autoSave`,
    /// per-window), so a single broadcast payload couldn't correctly serve
    /// every listener. Commands that only touch session state (switch,
    /// save, discard, set-auto-save) are unaffected by other windows and
    /// should keep calling `LayoutTree::emit_dashboards` on just their own
    /// terminal instead of this.
    pub fn emit_dashboards_all(&self, app: &AppHandle) {
        for t in self.list() {
            let snapshot = t.layout_tree.dashboards_snapshot();
            let chrome = format!("{}-chrome", t.id);
            if let Some(wv) = app.get_webview(&chrome) {
                let _ = wv.emit("wm:dashboards", &snapshot);
            }
        }
    }

    // ── Active-dashboard exclusivity lock (Issue 15-D) ────────────────────────

    /// Attempt to acquire `dashboard_id`'s lock for `terminal_id`, releasing
    /// `terminal_id`'s own previous lock entry as part of the same
    /// operation — a terminal can only ever hold one active-dashboard lock
    /// at a time, matching the one-active-dashboard-per-window invariant.
    /// An empty `dashboard_id` ("no active dashboard") is never locked and
    /// always succeeds, releasing whatever this terminal held before.
    ///
    /// Returns `Err(owning_terminal_name)` if `dashboard_id` is already
    /// locked by a *different* terminal. Re-acquiring a lock this same
    /// terminal already holds is a no-op success.
    pub fn acquire_dashboard_lock(&self, dashboard_id: &str, terminal_id: &str) -> Result<(), String> {
        let mut inner = self.inner.write().unwrap();
        if !dashboard_id.is_empty() {
            if let Some(owner) = inner.active_locks.get(dashboard_id) {
                if owner != terminal_id {
                    let owner_name = inner
                        .terminals
                        .get(owner)
                        .map(|t| t.name.read().unwrap().clone())
                        .unwrap_or_else(|| owner.clone());
                    return Err(owner_name);
                }
            }
        }
        inner.active_locks.retain(|_, v| v != terminal_id);
        if !dashboard_id.is_empty() {
            inner
                .active_locks
                .insert(dashboard_id.to_string(), terminal_id.to_string());
        }
        Ok(())
    }

    /// Release every lock entry owned by `terminal_id` (there should only
    /// ever be at most one). Called when the terminal's window closes
    /// (`wm_close_terminal`) or as part of `acquire_dashboard_lock` moving
    /// the lock elsewhere.
    pub fn release_dashboard_locks_for(&self, terminal_id: &str) {
        self.inner
            .write()
            .unwrap()
            .active_locks
            .retain(|_, v| v != terminal_id);
    }

    /// Name of the terminal holding `dashboard_id`'s lock, if it's locked by
    /// some terminal *other than* `excluding_terminal_id`. Used to reject a
    /// close/delete/switch that would otherwise interfere with another
    /// window's active dashboard — a terminal is always allowed to act on
    /// its own active dashboard (e.g. closing it and auto-switching away),
    /// so a lock this same terminal holds is not reported here.
    pub fn dashboard_lock_owner_name(
        &self,
        dashboard_id: &str,
        excluding_terminal_id: &str,
    ) -> Option<String> {
        if dashboard_id.is_empty() {
            return None;
        }
        let inner = self.inner.read().unwrap();
        let owner = inner.active_locks.get(dashboard_id)?;
        if owner == excluding_terminal_id {
            return None;
        }
        Some(
            inner
                .terminals
                .get(owner)
                .map(|t| t.name.read().unwrap().clone())
                .unwrap_or_else(|| owner.clone()),
        )
    }

    /// Every dashboard id currently locked by some terminal other than
    /// `excluding_terminal_id`. Used when a terminal needs to auto-pick a
    /// fallback active dashboard (e.g. after closing/deleting its own
    /// active one) — the fallback must skip anything already active
    /// elsewhere, or it would silently create the exact two-windows-same-
    /// dashboard conflict this lock exists to prevent.
    pub fn locked_dashboard_ids_excluding(&self, excluding_terminal_id: &str) -> HashSet<String> {
        self.inner
            .read()
            .unwrap()
            .active_locks
            .iter()
            .filter(|(_, owner)| owner.as_str() != excluding_terminal_id)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

impl Clone for TerminalManager {
    /// Clones the manager by sharing the same underlying `Arc` — both copies
    /// observe and mutate the same registry.
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            dashboards: Arc::clone(&self.dashboards),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::thread;

    use super::*;
    use crate::layout::persist::PersistedWindowConfig;
    use crate::layout::store::LayoutTree;
    use crate::terminal::state::{OverlayInner, TerminalState};
    use crate::webview_pool::WebviewPool;

    /// Register a minimal `TerminalState` fixture — no real Tauri app/window,
    /// just enough for the lock methods (which only need `id`/`name`) to work.
    fn test_terminal(manager: &TerminalManager, id: &str, name: &str) -> Arc<TerminalState> {
        let state = Arc::new(TerminalState {
            id: id.to_string(),
            name: RwLock::new(name.to_string()),
            layout_tree: LayoutTree::new(id, 800.0, 600.0, manager.dashboards()),
            overlay: Arc::new(Mutex::new(OverlayInner::default())),
            pool: WebviewPool::new(0),
            window_config: Arc::new(RwLock::new(PersistedWindowConfig::default())),
        });
        manager.register(Arc::clone(&state));
        state
    }

    #[test]
    fn switch_blocked_by_lock_names_the_owning_terminal() {
        let manager = TerminalManager::new();
        test_terminal(&manager, "terminal-a", "Terminal A");
        test_terminal(&manager, "terminal-b", "Terminal B");

        manager.acquire_dashboard_lock("dash-1", "terminal-a").unwrap();

        let err = manager
            .acquire_dashboard_lock("dash-1", "terminal-b")
            .unwrap_err();
        assert_eq!(err, "Terminal A", "must name the window that has it");
    }

    #[test]
    fn closing_releases_the_lock_immediately() {
        let manager = TerminalManager::new();
        test_terminal(&manager, "terminal-a", "Terminal A");
        manager.acquire_dashboard_lock("dash-1", "terminal-a").unwrap();

        manager.release_dashboard_locks_for("terminal-a");

        assert!(
            manager
                .dashboard_lock_owner_name("dash-1", "terminal-b")
                .is_none(),
            "another window must be able to acquire it right away"
        );
    }

    #[test]
    fn reacquiring_ones_own_lock_is_a_no_op() {
        let manager = TerminalManager::new();
        test_terminal(&manager, "terminal-a", "Terminal A");
        manager.acquire_dashboard_lock("dash-1", "terminal-a").unwrap();
        assert!(manager.acquire_dashboard_lock("dash-1", "terminal-a").is_ok());
    }

    #[test]
    fn a_terminal_only_ever_holds_one_lock_at_a_time() {
        let manager = TerminalManager::new();
        test_terminal(&manager, "terminal-a", "Terminal A");
        manager.acquire_dashboard_lock("dash-1", "terminal-a").unwrap();
        manager.acquire_dashboard_lock("dash-2", "terminal-a").unwrap();

        assert!(
            manager
                .dashboard_lock_owner_name("dash-1", "terminal-z")
                .is_none(),
            "switching to dash-2 must release the previous lock on dash-1"
        );
        assert_eq!(
            manager.dashboard_lock_owner_name("dash-2", "terminal-z"),
            Some("Terminal A".to_string())
        );
    }

    #[test]
    fn empty_dashboard_id_is_never_locked() {
        let manager = TerminalManager::new();
        test_terminal(&manager, "terminal-a", "Terminal A");
        manager.acquire_dashboard_lock("dash-1", "terminal-a").unwrap();
        // Switching to "no active dashboard" must release the prior lock.
        manager.acquire_dashboard_lock("", "terminal-a").unwrap();

        assert!(manager
            .dashboard_lock_owner_name("dash-1", "terminal-z")
            .is_none());
    }

    #[test]
    fn locked_dashboard_ids_excluding_omits_the_callers_own_lock() {
        let manager = TerminalManager::new();
        test_terminal(&manager, "terminal-a", "Terminal A");
        test_terminal(&manager, "terminal-b", "Terminal B");
        manager.acquire_dashboard_lock("dash-1", "terminal-a").unwrap();
        manager.acquire_dashboard_lock("dash-2", "terminal-b").unwrap();

        let excluding_a = manager.locked_dashboard_ids_excluding("terminal-a");
        assert!(excluding_a.contains("dash-2"));
        assert!(!excluding_a.contains("dash-1"));
    }

    #[test]
    fn two_terminals_racing_to_switch_to_the_same_dashboard() {
        let manager = Arc::new(TerminalManager::new());
        test_terminal(&manager, "terminal-a", "Terminal A");
        test_terminal(&manager, "terminal-b", "Terminal B");

        let m1 = Arc::clone(&manager);
        let m2 = Arc::clone(&manager);
        let t1 = thread::spawn(move || m1.acquire_dashboard_lock("dash-1", "terminal-a"));
        let t2 = thread::spawn(move || m2.acquire_dashboard_lock("dash-1", "terminal-b"));

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();

        // Exactly one of the two racing switches must win — never both, never neither.
        assert_ne!(
            r1.is_ok(),
            r2.is_ok(),
            "exactly one of the two racing acquires must succeed, got {r1:?} / {r2:?}"
        );
    }
}
