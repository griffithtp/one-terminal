//! `TerminalManager` — global registry of all open Terminal OS windows.
//!
//! Registered once via `.manage()`. Layout commands look up their per-terminal
//! objects (`LayoutTree`, `WebviewPool`, `OverlayState`) by the invoking
//! window's label instead of injecting them as separate global `State` values.

use std::sync::{Arc, RwLock};

use indexmap::IndexMap;

use super::state::TerminalState;

// ── Internal storage ──────────────────────────────────────────────────────────

#[allow(dead_code)]
struct ManagerInner {
    /// Open terminals in creation order.
    terminals: IndexMap<String, Arc<TerminalState>>,
    /// Monotonically increasing counter for generating unique terminal IDs.
    /// Starts at 2; "terminal-main" occupies the implied slot 1.
    next_id: u32,
}

// ── TerminalManager ───────────────────────────────────────────────────────────

// Methods used only in later PRs; silence dead-code warnings on the scaffold.
#[allow(dead_code)]
pub struct TerminalManager {
    inner: Arc<RwLock<ManagerInner>>,
}

#[allow(dead_code)]
impl TerminalManager {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(ManagerInner {
                terminals: IndexMap::new(),
                next_id: 2,
            })),
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
}

impl Clone for TerminalManager {
    /// Clones the manager by sharing the same underlying `Arc` — both copies
    /// observe and mutate the same registry.
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
