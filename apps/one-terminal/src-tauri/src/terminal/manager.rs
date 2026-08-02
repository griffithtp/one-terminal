//! `TerminalManager` — global registry of all open Terminal OS windows.
//!
//! Registered once via `.manage()`. Layout commands look up their per-terminal
//! objects (`LayoutTree`, `WebviewPool`, `OverlayState`) by the invoking
//! window's label instead of injecting them as separate global `State` values.

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
