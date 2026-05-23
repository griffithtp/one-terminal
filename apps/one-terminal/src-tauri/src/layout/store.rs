//! `LayoutTree` — thread-safe store for the N-ary layout tree.
//!
//! One `LayoutTree` lives inside each `TerminalState`. Commands reach it via
//! `manager.get(window.label())?.layout_tree`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, Window,
};
use uuid::Uuid;

use super::dashboard::{DashboardError, DashboardStore, DashboardsSnapshot};
use super::docking::{add_leaf_as_sibling, append_to_stack_at, is_stack_at, move_leaf, DropZone};
use super::host::{compute_host_layout, HostLayout};
use super::node::{Direction, LayoutNode};
use super::persist::{self, PersistedLayout, PersistedLeafMeta};
use super::reflow::{park_offscreen, reflow_layout, SPLITTER_THICKNESS, TAB_STRIP_HEIGHT};
use super::{LayoutSnapshot, PanelBounds, SplitDir, HEADER_HEIGHT};

/// Content metadata per Leaf — webviews are positioned by label, but the
/// frontend needs (app_id, url, title) to render headers and launch chips.
#[derive(Clone, Debug)]
struct LeafMeta {
    app_id: String,
    url: String,
    title: String,
    /// Engine binding the panel was opened for. `None` ⇒ this WM host's
    /// own engine (no override). Kept so layout persistence + reflow
    /// can round-trip the binding.
    engine_binding: Option<ot_core::engine::EngineBinding>,
    /// User-set display name override. `None` means show the app-provided `title`.
    display_name: Option<String>,
    /// Webview zoom multiplier. Default `1.0`; valid range `0.5..=2.0`.
    zoom_factor: f64,
}

/// Content metadata for a new panel, passed as a unit to [`LayoutTree::add_panel`]
/// and [`LayoutTree::add_panel_with_label`].
pub struct PanelSpec {
    pub app_id: String,
    pub url: String,
    pub title: String,
    pub engine_binding: Option<ot_core::engine::EngineBinding>,
}

#[derive(Default)]
struct Inner {
    root: Option<LayoutNode>,
    /// Per-leaf content metadata, keyed by webview `label`.
    meta: HashMap<String, LeafMeta>,
    /// Label of the leaf currently considered "active" — drives where a new
    /// panel lands when `wm_open` is called without an explicit target, and
    /// which tab of a Stack is visible. `None` iff tree is empty.
    active_panel: Option<String>,
    /// When `Some`, the Stack with this id fills the whole content area and
    /// every other leaf is parked offscreen. Tracked by id (not path) so
    /// tab adds/removes/moves that reshape sibling indices don't drop the
    /// state — the id is stable until the stack itself dissolves (last tab
    /// removed), at which point `resolve_maximized_path` returns None and
    /// the normal layout resumes.
    maximized_stack_id: Option<String>,
    width: f64,
    height: f64,
}

#[derive(Clone)]
pub struct LayoutTree {
    inner: Arc<RwLock<Inner>>,
    /// Named layout snapshots. Populated from disk in `init`; the active
    /// dashboard is the source of truth for what gets restored on startup.
    dashboard_store: Arc<RwLock<DashboardStore>>,
    /// Set once during `init` so all subsequent `schedule_save` calls can
    /// resolve `app_data_dir` without threading it through every call site.
    app: Arc<OnceLock<AppHandle>>,
    /// Handle for the pending debounced save task. Replaced on every mutation
    /// so rapid changes coalesce into a single disk write.
    save_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    /// Stable identifier for this Terminal instance. Used as the prefix for
    /// all panel labels (`<terminal_id>-panel-<uuid>`) so that multiple
    /// Terminals can coexist without label collisions.
    terminal_id: Arc<str>,
}

impl LayoutTree {
    pub fn new(terminal_id: &str, width: f64, height: f64) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                root: None,
                meta: HashMap::new(),
                active_panel: None,
                maximized_stack_id: None,
                width,
                height,
            })),
            dashboard_store: Arc::new(RwLock::new(DashboardStore::with_empty())),
            app: Arc::new(OnceLock::new()),
            save_handle: Arc::new(Mutex::new(None)),
            terminal_id: Arc::from(terminal_id),
        }
    }

    /// Store the `AppHandle` without loading any persisted state.
    ///
    /// Retained for call sites that need to wire up the AppHandle before
    /// calling `init` separately. Most callers should prefer `init` directly.
    pub fn register_app_handle(&self, app: &AppHandle) {
        let _ = self.app.set(app.clone());
    }

    /// Store the `AppHandle` and load any persisted layout from disk.
    ///
    /// Must be called once from the Tauri `setup` closure before any panels
    /// are opened.  Safe to call on any clone of `LayoutTree`; the underlying
    /// `Arc`s ensure the state is shared.
    pub fn init(&self, app: &AppHandle) -> Result<(), String> {
        let _ = self.app.set(app.clone());

        let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;

        // Use terminal_id-aware load so each Terminal reads its own file.
        // For "terminal-main" this also handles migration from the legacy layout.json.
        let terminal_persist_opt = if self.terminal_id.as_ref() == "terminal-main" {
            persist::load_terminal(&data_dir)
        } else {
            persist::load_terminal_for(&self.terminal_id, &data_dir)
        };
        if let Some(terminal_persist) = terminal_persist_opt {
            let ds = DashboardStore::from_persist(terminal_persist);

            // Load the active dashboard's layout into Inner.
            if let Some(layout) = ds.load_active() {
                let mut g = self.inner.write().unwrap();
                g.root = layout.tree;
                g.meta = layout
                    .meta
                    .into_iter()
                    .map(|(label, pm)| {
                        (
                            label,
                            LeafMeta {
                                app_id: pm.app_id,
                                url: pm.url,
                                title: pm.title,
                                engine_binding: pm.engine_binding,
                                display_name: pm.display_name,
                                zoom_factor: pm.zoom_factor,
                            },
                        )
                    })
                    .collect();
                g.active_panel = layout.active_panel;
                g.maximized_stack_id = layout.maximized_stack_id;
            }

            *self.dashboard_store.write().unwrap() = ds;
        }

        Ok(())
    }

    /// Return every (label, url) pair for leaves currently in the tree so
    /// the startup path can recreate webviews for a restored layout.
    pub fn panels_for_restore(&self) -> Vec<(String, String)> {
        let g = self.inner.read().unwrap();
        let Some(root) = g.root.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        collect_panel_urls(root, &g.meta, &mut out);
        out
    }

    pub fn set_size(&self, width: f64, height: f64) {
        let mut g = self.inner.write().unwrap();
        g.width = width;
        g.height = height;
    }

    pub fn size(&self) -> (f64, f64) {
        let g = self.inner.read().unwrap();
        (g.width, g.height)
    }

    /// Read-only access to the root. Returns `None` if the tree is empty.
    pub fn with_root<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&LayoutNode) -> R,
    {
        let g = self.inner.read().unwrap();
        g.root.as_ref().map(f)
    }

    /// Replace the root wholesale. Clears meta/active tracking since labels
    /// in the new tree may not match prior state — callers are responsible
    /// for repopulating metadata if needed.
    pub fn set_root(&self, root: Option<LayoutNode>) {
        {
            let mut g = self.inner.write().unwrap();
            g.root = root;
            g.meta.clear();
            g.active_panel = None;
            g.maximized_stack_id = None;
        }
        self.schedule_save(500);
    }

    /// Toggle the maximize state for the Stack at `path`. If this stack is
    /// already maximized, restores the normal layout. Otherwise captures the
    /// stack's stable id so the maximize state survives tab adds/removes
    /// that shift sibling indices. Rejects paths that don't resolve to a
    /// Stack so callers can dispatch without pre-checks. Returns `true` iff
    /// state changed.
    pub fn toggle_maximize_stack(&self, path: &[usize]) -> bool {
        let changed = {
            let mut g = self.inner.write().unwrap();
            let Some(root) = g.root.as_ref() else {
                return false;
            };
            let Some(node) = node_at(root, path) else {
                return false;
            };
            let LayoutNode::Stack {
                id,
                active,
                children,
                ..
            } = node
            else {
                return false;
            };
            if g.maximized_stack_id.as_deref() == Some(id.as_str()) {
                g.maximized_stack_id = None;
                true
            } else {
                // Pin active_panel to the maximized stack's active tab so the default
                // `wm_open` path (target = active_panel when unspecified) tab-inserts
                // into the maximized group rather than whatever the user touched last.
                let active_idx = (*active).min(children.len().saturating_sub(1));
                let active_label = children.get(active_idx).and_then(|c| match c {
                    LayoutNode::Leaf { label, .. } => Some(label.clone()),
                    _ => None,
                });
                let id = id.clone();
                g.maximized_stack_id = Some(id);
                if let Some(label) = active_label {
                    g.active_panel = Some(label);
                }
                true
            }
        };
        if changed {
            self.schedule_save(500);
        }
        changed
    }

    /// Currently-active panel label, if any.
    #[allow(dead_code)]
    pub fn active_panel(&self) -> Option<String> {
        self.inner.read().unwrap().active_panel.clone()
    }

    /// Mark `label` as the active panel. Caller is responsible for ensuring
    /// `label` exists in the tree.
    #[allow(dead_code)]
    pub fn set_active_panel(&self, label: &str) {
        {
            self.inner.write().unwrap().active_panel = Some(label.to_string());
        }
        self.schedule_save(500);
    }

    /// Reflow every webview from the current tree and window dimensions.
    /// Content area starts below the global chrome header (HEADER_HEIGHT).
    /// When a stack is maximized, every leaf outside it is parked offscreen
    /// and the maximized stack is reflowed at the full content rect.
    pub fn reflow(&self, app: &AppHandle) {
        let g = self.inner.read().unwrap();
        let Some(root) = g.root.as_ref() else { return };
        let h = (g.height - HEADER_HEIGHT).max(0.0);

        if let Some(stack_id) = g.maximized_stack_id.as_deref() {
            let mut path = Vec::new();
            if find_stack_path_by_id(root, stack_id, &mut path) {
                if let Some(stack_node) = node_at(root, &path) {
                    park_offscreen(root, app);
                    reflow_layout(stack_node, app, 0.0, HEADER_HEIGHT, g.width, h);
                    return;
                }
            }
        }

        reflow_layout(root, app, 0.0, HEADER_HEIGHT, g.width, h);
    }

    /// Compute the current host-shell projection (tab strips + splitter handles).
    pub fn host_snapshot(&self) -> HostLayout {
        let g = self.inner.read().unwrap();
        let h = (g.height - HEADER_HEIGHT).max(0.0);
        let titles: HashMap<String, String> = g
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.title.clone()))
            .collect();
        let app_ids: HashMap<String, String> = g
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.app_id.clone()))
            .collect();
        let display_names: HashMap<String, Option<String>> = g
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.display_name.clone()))
            .collect();
        let zoom_factors: HashMap<String, f64> = g
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.zoom_factor))
            .collect();
        let mut max_path_buf = Vec::new();
        let max_path = g
            .maximized_stack_id
            .as_deref()
            .zip(g.root.as_ref())
            .and_then(|(id, root)| {
                if find_stack_path_by_id(root, id, &mut max_path_buf) {
                    Some(max_path_buf.as_slice())
                } else {
                    None
                }
            });
        compute_host_layout(
            g.root.as_ref(),
            max_path,
            0.0,
            HEADER_HEIGHT,
            g.width,
            h,
            &titles,
            &app_ids,
            &display_names,
            &zoom_factors,
        )
    }

    /// Emit the host-shell projection (tab strips + splitter handles) so the
    /// chrome webview can render its overlays on top of the panel "holes".
    /// Matches reflow's HEADER_HEIGHT offset so overlays align with webviews.
    pub fn emit_host(&self, app: &AppHandle) {
        let payload = self.host_snapshot();
        let chrome = format!("{}-chrome", self.terminal_id);
        if let Some(wv) = app.get_webview(&chrome) {
            let _ = wv.emit("wm:host-layout", &payload);
        }
    }

    /// Update the display title for the panel identified by `label`.
    /// Returns `true` if the panel exists (and was updated); `false` otherwise.
    pub fn rename_panel(&self, label: &str, title: &str) -> bool {
        let found = {
            let mut g = self.inner.write().unwrap();
            let Some(meta) = g.meta.get_mut(label) else {
                return false;
            };
            meta.title = title.to_string();
            true
        };
        if found {
            self.schedule_save(500);
        }
        found
    }

    /// Set the user-facing display name override for `label`.
    /// `None` clears the override, reverting to the app-provided title.
    pub fn set_display_name(&self, label: &str, name: Option<String>) -> bool {
        let found = {
            let mut g = self.inner.write().unwrap();
            let Some(meta) = g.meta.get_mut(label) else {
                return false;
            };
            meta.display_name = name;
            true
        };
        if found {
            // Rename/zoom are infrequent — write immediately, no debounce.
            self.schedule_save(0);
        }
        found
    }

    /// Set the zoom factor for `label`. Clamps to `0.5..=2.0`.
    /// Returns the clamped value, or `None` if the panel doesn't exist.
    pub fn set_zoom_factor(&self, label: &str, zoom_factor: f64) -> Option<f64> {
        let clamped = zoom_factor.clamp(0.5, 2.0);
        let found = {
            let mut g = self.inner.write().unwrap();
            let meta = g.meta.get_mut(label)?;
            meta.zoom_factor = clamped;
            true
        };
        if found {
            self.schedule_save(0);
            Some(clamped)
        } else {
            None
        }
    }

    /// Synthesize a `LayoutSnapshot` for the legacy `wm:layout` event. Only
    /// leaves *not* inside a Stack are surfaced as panels — Stack members get
    /// their headers from the tab strip (`wm:host-layout`) instead.
    pub fn snapshot(&self) -> Option<LayoutSnapshot> {
        let g = self.inner.read().unwrap();
        let root = g.root.as_ref()?;
        let h = (g.height - HEADER_HEIGHT).max(0.0);
        let mut panels = Vec::new();
        walk_for_snapshot(
            root,
            0.0,
            HEADER_HEIGHT,
            g.width,
            h,
            false,
            &mut panels,
            &g.meta,
        );
        Some(LayoutSnapshot {
            panels,
            dividers: Vec::new(),
            window_width: g.width,
            window_height: g.height,
        })
    }

    /// Park every Leaf offscreen — used at the start of a tab drag so the
    /// chrome webview becomes fully visible (panels sit above the chrome in
    /// z-order, so the ghost + drop indicators can't paint through them).
    /// The webview processes stay alive; a subsequent `reflow` restores them.
    pub fn park_all(&self, app: &AppHandle) {
        let g = self.inner.read().unwrap();
        if let Some(root) = g.root.as_ref() {
            park_offscreen(root, app);
        }
    }

    /// Add a new panel to the tree, generating a fresh webview label.
    ///
    /// Placement logic:
    /// - Tree empty → new leaf becomes root (bare Leaf, no Stack wrapper).
    /// - `dir = Some(d)` → split `target` (or active, or first) along `d`.
    /// - `dir = None`  → tab-insert:
    ///   • target's parent is a Stack → append as new sibling tab.
    ///   • otherwise                  → wrap target + new leaf into a Stack.
    ///
    /// The new leaf becomes the active panel. Returns the new webview label.
    pub fn add_panel(
        &self,
        spec: PanelSpec,
        target: Option<&str>,
        dir: Option<SplitDir>,
    ) -> String {
        let label = format!("{}-panel-{}", self.terminal_id, short_id());
        self.insert_panel(label, spec, target, dir)
    }

    /// Like [`add_panel`] but uses a caller-supplied `label` instead of
    /// generating one. Used by the webview pool so the pre-created blank
    /// webview's label and the tree's panel id stay in sync.
    pub fn add_panel_with_label(
        &self,
        label: &str,
        spec: PanelSpec,
        target: Option<&str>,
        dir: Option<SplitDir>,
    ) -> String {
        self.insert_panel(label.to_string(), spec, target, dir)
    }

    fn insert_panel(
        &self,
        label: String,
        spec: PanelSpec,
        target: Option<&str>,
        dir: Option<SplitDir>,
    ) -> String {
        {
            let mut g = self.inner.write().unwrap();
            g.meta.insert(
                label.clone(),
                LeafMeta {
                    app_id: spec.app_id,
                    url: spec.url,
                    title: spec.title,
                    engine_binding: spec.engine_binding,
                    display_name: None,
                    zoom_factor: 1.0,
                },
            );

            let leaf = LayoutNode::Leaf {
                label: label.clone(),
                weight: 1.0,
            };

            // Empty tree — install bare leaf.
            if g.root.is_none() {
                g.root = Some(leaf);
                g.active_panel = Some(label.clone());
            } else {
                // Resolve the target label: explicit arg > active_panel > first leaf.
                let target_label = target
                    .map(|s| s.to_string())
                    .or_else(|| g.active_panel.clone())
                    .or_else(|| first_leaf_label(g.root.as_ref().unwrap()));

                let Some(target_label) = target_label else {
                    // Unreachable given root is Some, but guard anyway.
                    g.root = Some(leaf);
                    g.active_panel = Some(label.clone());
                    // Drop g before schedule_save.
                    drop(g);
                    self.schedule_save(500);
                    return label;
                };

                // Find target path.
                let root = g.root.as_mut().unwrap();
                let mut target_path = Vec::new();
                if !super::docking::find_leaf_path(root, &target_label, &mut target_path) {
                    // Target not in tree — fall back to first leaf.
                    target_path.clear();
                    if let Some(first) = first_leaf_label(root) {
                        super::docking::find_leaf_path(root, &first, &mut target_path);
                    }
                }

                match dir {
                    Some(d) => {
                        let zone = match d {
                            SplitDir::Horizontal => DropZone::Right,
                            SplitDir::Vertical => DropZone::Bottom,
                        };
                        add_leaf_as_sibling(&mut g.root, &target_path, zone, leaf);
                    }
                    None => {
                        let parent_is_stack = if target_path.is_empty() {
                            is_stack_at(g.root.as_ref().unwrap(), &[])
                        } else {
                            let parent_path = &target_path[..target_path.len() - 1];
                            is_stack_at(g.root.as_ref().unwrap(), parent_path)
                        };

                        if parent_is_stack {
                            let parent_path = if target_path.is_empty() {
                                Vec::new()
                            } else {
                                target_path[..target_path.len() - 1].to_vec()
                            };
                            append_to_stack_at(g.root.as_mut().unwrap(), &parent_path, leaf);
                        } else {
                            add_leaf_as_sibling(&mut g.root, &target_path, DropZone::Center, leaf);
                        }
                    }
                }

                g.active_panel = Some(label.clone());
            }
        } // write lock released here
        self.schedule_save(500);
        label
    }

    /// Remove a panel: drop its leaf from the tree, forget its metadata, and
    /// pick a replacement active panel if we removed the current one.
    /// Returns `true` if the leaf was found and removed.
    pub fn remove_panel(&self, label: &str) -> bool {
        let removed = {
            let mut g = self.inner.write().unwrap();
            let removed = super::docking::remove_leaf(&mut g.root, label);
            if !removed {
                return false;
            }
            g.meta.remove(label);
            if g.active_panel.as_deref() == Some(label) {
                let fallback = g.maximized_stack_id.as_deref().and_then(|id| {
                    g.root
                        .as_ref()
                        .and_then(|r| first_leaf_in_stack_with_id(r, id))
                });
                g.active_panel = fallback.or_else(|| g.root.as_ref().and_then(first_leaf_label));
            }
            true
        };
        if removed {
            self.schedule_save(500);
        }
        removed
    }

    /// Move the Leaf with `source_label` to `target_path` under `zone` semantics.
    /// When `zone == Center` and `insert_index` is `Some(i)`, the leaf is
    /// inserted at position `i` in the target Stack's children rather than
    /// appended. Also makes the moved leaf active.
    /// Returns `true` if the tree was mutated.
    pub fn move_leaf(
        &self,
        source_label: &str,
        target_path: &[usize],
        zone: DropZone,
        insert_index: Option<usize>,
    ) -> bool {
        let changed = {
            let mut g = self.inner.write().unwrap();
            let changed = move_leaf(&mut g.root, source_label, target_path, zone, insert_index);
            if changed {
                g.active_panel = Some(source_label.to_string());
            }
            changed
        };
        if changed {
            self.schedule_save(500);
        }
        changed
    }

    /// Swap the positions of two leaves identified by label. Used by panel-
    /// header drag (chrome-drawn header on non-stack leaves).
    pub fn swap_leaves(&self, a: &str, b: &str) -> bool {
        if a == b {
            return false;
        }
        let changed = {
            let mut g = self.inner.write().unwrap();
            let Some(root) = g.root.as_mut() else {
                return false;
            };
            swap_leaves_inner(root, a, b)
        };
        if changed {
            self.schedule_save(500);
        }
        changed
    }

    /// Set the active tab on the Stack at `path`. Also marks the chosen leaf
    /// as the active panel so subsequent `wm_open` calls target its group.
    pub fn set_active_tab(&self, path: &[usize], index: usize) -> bool {
        let changed = {
            let mut g = self.inner.write().unwrap();
            let changed = super::docking::set_active_tab(&mut g.root, path, index);
            if changed {
                if let Some(root) = g.root.as_ref() {
                    let mut active_path = path.to_vec();
                    active_path.push(index);
                    if let Some(label) = leaf_label_at(root, &active_path) {
                        g.active_panel = Some(label);
                    }
                }
            }
            changed
        };
        if changed {
            self.schedule_save(500);
        }
        changed
    }

    /// Remove the Leaf with `label` from the tree (and simplify). Preserved
    /// for callers that only manipulate the tree without touching meta state —
    /// prefer `remove_panel` when managing a full panel lifecycle.
    pub fn remove_leaf(&self, label: &str) -> bool {
        self.remove_panel(label)
    }

    /// Collect every leaf label under the Stack at `path` (depth-first).
    /// Returns an empty vec if the tree is empty or `path` doesn't land on
    /// a Stack. Read-only — used by `wm_close_stack` to find the set of
    /// webviews to destroy for a "Close group" action.
    pub fn labels_in_stack(&self, path: &[usize]) -> Vec<String> {
        let g = self.inner.read().unwrap();
        let Some(root) = g.root.as_ref() else {
            return Vec::new();
        };
        if !is_stack_at(root, path) {
            return Vec::new();
        }
        let mut node = root;
        for &i in path {
            match node {
                LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
                    let Some(next) = children.get(i) else {
                        return Vec::new();
                    };
                    node = next;
                }
                LayoutNode::Leaf { .. } => return Vec::new(),
            }
        }
        let mut out = Vec::new();
        collect_leaf_labels(node, &mut out);
        out
    }

    /// Place the boundary between `children[child_index]` and
    /// `children[child_index + 1]` of the Splitter at `path` under the cursor
    /// at window-coordinates `(px, py)`. Preserves `w_i + w_{i+1}` so untouched
    /// siblings keep their share.
    ///
    /// Returns `true` iff the mutation was applied (path valid, splitter has
    /// an adjacent pair at `child_index`).
    pub fn resize_splitter(&self, path: &[usize], child_index: usize, px: f64, py: f64) -> bool {
        let changed = {
            let mut g = self.inner.write().unwrap();
            let (w, h) = (g.width, (g.height - HEADER_HEIGHT).max(0.0));
            let Some(root) = g.root.as_mut() else {
                return false;
            };
            apply_splitter_drag(root, path, child_index, 0.0, HEADER_HEIGHT, w, h, px, py)
        };
        if changed {
            self.schedule_save(500);
        }
        changed
    }

    // ── Dashboard operations ──────────────────────────────────────────────────

    /// Emit the current dashboard list state as `wm:dashboards`.
    pub fn emit_dashboards(&self, app: &AppHandle) {
        let snapshot = self.dashboard_store.read().unwrap().as_snapshot();
        let chrome = format!("{}-chrome", self.terminal_id);
        if let Some(wv) = app.get_webview(&chrome) {
            let _ = wv.emit("wm:dashboards", &snapshot);
        }
    }

    /// Return the current dashboard list state without emitting an event.
    pub fn dashboards_snapshot(&self) -> DashboardsSnapshot {
        self.dashboard_store.read().unwrap().as_snapshot()
    }

    /// Write the current `DashboardStore` to disk immediately without touching
    /// `Inner`. Used by metadata-only mutations (create, rename, delete, reorder).
    pub fn persist_dashboards(&self) {
        let Some(app) = self.app.get() else { return };
        let terminal_persist = self.dashboard_store.read().unwrap().to_terminal_persist();
        if let Ok(data_dir) = app.path().app_data_dir() {
            let tid = self.terminal_id.to_string();
            if let Err(e) = persist::save_terminal_dashboards(&tid, &terminal_persist, &data_dir) {
                eprintln!("[layout] persist_dashboards: {e}");
            }
        }
    }

    /// Expose a write-locked reference to the dashboard store for commands
    /// that need to mutate it directly (create / rename / delete / reorder).
    pub fn with_dashboard_store_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut DashboardStore) -> R,
    {
        f(&mut self.dashboard_store.write().unwrap())
    }

    /// Set auto-save mode. When switching from off→on, the live layout is
    /// immediately snapshotted and persisted, clearing the dirty flag.
    pub fn set_auto_save(&self, enabled: bool) {
        let should_flush = {
            let mut ds = self.dashboard_store.write().unwrap();
            let was_off = !ds.auto_save;
            ds.auto_save = enabled;
            was_off && enabled && ds.dirty
        };
        if should_flush {
            self.schedule_save(0);
        }
    }

    /// Snapshot the current live layout into the active dashboard slot and
    /// write immediately to disk. Clears the dirty flag.
    pub fn save_dashboard(&self) {
        let layout = {
            let g = self.inner.read().unwrap();
            snapshot_for_persist(&g)
        };
        let terminal_persist = {
            let mut ds = self.dashboard_store.write().unwrap();
            ds.snapshot_current(layout);
            ds.dirty = false;
            ds.to_terminal_persist()
        };
        let Some(app) = self.app.get() else { return };
        if let Ok(data_dir) = app.path().app_data_dir() {
            let tid = self.terminal_id.to_string();
            if let Err(e) = persist::save_terminal_dashboards(&tid, &terminal_persist, &data_dir) {
                eprintln!("[layout] save_dashboard: {e}");
            }
        }
    }

    /// Reload the saved active dashboard snapshot into `Inner`, discarding all
    /// unsaved live changes, and reconcile panel webviews on the main thread.
    /// Clears the dirty flag. `win` is required to create any missing webviews.
    pub fn discard_dashboard(&self, win: &Window, app: &AppHandle) -> Result<(), DashboardError> {
        // Collect what's currently alive so we can diff.
        let current_labels: Vec<String> = {
            let g = self.inner.read().unwrap();
            let mut labels = Vec::new();
            if let Some(root) = &g.root {
                collect_leaf_labels(root, &mut labels);
            }
            labels
        };

        // Load the clean snapshot and reset Inner.
        let clean_layout = {
            let mut ds = self.dashboard_store.write().unwrap();
            ds.dirty = false;
            ds.load_active()
        };
        let panels_to_create = self.apply_layout_to_inner(clean_layout);

        // Reconcile webviews on the main thread.
        self.reconcile_panel_webviews(current_labels, panels_to_create, win, app)?;

        self.reflow(app);
        self.emit_host(app);
        if let Some(snap) = self.snapshot() {
            let chrome = format!("{}-chrome", self.terminal_id);
            if let Some(wv) = app.get_webview(&chrome) {
                let _ = wv.emit("wm:layout", &snap);
            }
        }
        Ok(())
    }

    /// Switch the active dashboard to `name`, performing the full webview
    /// destroy/recreate lifecycle.
    ///
    /// Returns `DashboardError::NeedsConfirm` when `auto_save` is off and the
    /// live layout has unsaved changes — the frontend must show a
    /// save/discard/cancel dialog and retry.
    pub fn switch_dashboard(
        &self,
        name: &str,
        win: &Window,
        app: &AppHandle,
    ) -> Result<(), DashboardError> {
        // Validate and check dirty state without holding any write lock.
        {
            let ds = self.dashboard_store.read().unwrap();
            if ds.active == name {
                return Ok(());
            }
            if !ds.dashboards.contains_key(name) {
                return Err(DashboardError::NotFound);
            }
            if !ds.auto_save && ds.dirty {
                return Err(DashboardError::NeedsConfirm);
            }
        }

        // Collect current panel labels (to destroy later).
        let current_labels: Vec<String> = {
            let g = self.inner.read().unwrap();
            let mut labels = Vec::new();
            if let Some(root) = &g.root {
                collect_leaf_labels(root, &mut labels);
            }
            labels
        };

        // Snapshot outgoing dashboard if auto_save is on, then switch active.
        let new_layout = {
            let mut ds = self.dashboard_store.write().unwrap();
            if ds.auto_save {
                let layout = {
                    let g = self.inner.read().unwrap();
                    snapshot_for_persist(&g)
                };
                ds.snapshot_current(layout);
            }
            ds.active = name.to_string();
            ds.dirty = false;
            ds.load_active()
        };

        // Load new dashboard into Inner and collect panels to create.
        let panels_to_create = self.apply_layout_to_inner(new_layout);

        // Destroy old webviews + create new ones on the main thread.
        self.reconcile_panel_webviews(current_labels, panels_to_create, win, app)?;

        // Persist the updated store (new active + snapshot of outgoing dashboard).
        self.schedule_save(0);

        self.reflow(app);
        self.emit_host(app);
        if let Some(snap) = self.snapshot() {
            let chrome = format!("{}-chrome", self.terminal_id);
            if let Some(wv) = app.get_webview(&chrome) {
                let _ = wv.emit("wm:layout", &snap);
            }
        }

        Ok(())
    }

    /// Delete a dashboard by name, reconciling panel webviews when the active
    /// dashboard is the one being removed. Returns `false` if `name` doesn't
    /// exist. Allowed to delete the last dashboard, leaving the terminal empty.
    pub fn delete_dashboard(
        &self,
        name: &str,
        win: &Window,
        app: &AppHandle,
    ) -> Result<bool, DashboardError> {
        // Check existence and whether this is the active dashboard.
        let (is_active, current_labels) = {
            let ds = self.dashboard_store.read().unwrap();
            if !ds.dashboards.contains_key(name) {
                return Ok(false);
            }
            let is_active = ds.active == name;
            let labels = if is_active {
                let g = self.inner.read().unwrap();
                let mut out = Vec::new();
                if let Some(root) = &g.root {
                    collect_leaf_labels(root, &mut out);
                }
                out
            } else {
                vec![]
            };
            (is_active, labels)
        };

        self.with_dashboard_store_mut(|ds| ds.delete(name));

        if !is_active {
            return Ok(true);
        }

        // Deleted the active dashboard — load whatever is now active (or None).
        let new_layout = self.dashboard_store.read().unwrap().load_active();
        let panels_to_create = self.apply_layout_to_inner(new_layout);
        self.reconcile_panel_webviews(current_labels, panels_to_create, win, app)?;

        self.reflow(app);
        self.emit_host(app);
        let chrome = format!("{}-chrome", self.terminal_id);
        if let Some(wv) = app.get_webview(&chrome) {
            if let Some(snap) = self.snapshot() {
                let _ = wv.emit("wm:layout", &snap);
            } else {
                let _ = wv.emit("wm:layout", serde_json::Value::Null);
            }
        }

        Ok(true)
    }

    /// Load a `PersistedLayout` (or empty layout when `None`) into `Inner`.
    /// Returns the `(label, url)` pairs of all panels in the new layout,
    /// ordered depth-first.
    fn apply_layout_to_inner(&self, layout: Option<PersistedLayout>) -> Vec<(String, String)> {
        match layout {
            Some(l) => {
                let mut panels = Vec::new();
                {
                    let mut g = self.inner.write().unwrap();
                    g.root = l.tree;
                    g.meta = l
                        .meta
                        .into_iter()
                        .map(|(label, pm)| {
                            let url = pm.url.clone();
                            let meta = LeafMeta {
                                app_id: pm.app_id,
                                url,
                                title: pm.title,
                                engine_binding: pm.engine_binding,
                                display_name: pm.display_name,
                                zoom_factor: pm.zoom_factor,
                            };
                            (label, meta)
                        })
                        .collect();
                    g.active_panel = l.active_panel;
                    g.maximized_stack_id = l.maximized_stack_id;

                    // Collect panel pairs depth-first from the new tree.
                    if let Some(root) = &g.root {
                        collect_panel_urls(root, &g.meta, &mut panels);
                    }
                }
                panels
            }
            None => {
                let mut g = self.inner.write().unwrap();
                g.root = None;
                g.meta.clear();
                g.active_panel = None;
                g.maximized_stack_id = None;
                Vec::new()
            }
        }
    }

    /// Close all webviews whose label is in `to_destroy`, then open fresh
    /// webviews for each `(label, url)` pair in `to_create`. Dispatched on
    /// the main thread because `add_child` requires it on Windows/macOS.
    fn reconcile_panel_webviews(
        &self,
        to_destroy: Vec<String>,
        to_create: Vec<(String, String)>,
        win: &Window,
        app: &AppHandle,
    ) -> Result<(), DashboardError> {
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_for_main = app.clone();
        let win_for_main = win.clone();

        app.run_on_main_thread(move || {
            let result = (|| -> Result<(), String> {
                for label in &to_destroy {
                    if let Some(wv) = app_for_main.get_webview(label) {
                        wv.close().map_err(|e| e.to_string())?;
                    }
                }
                for (label, url) in &to_create {
                    if let Ok(parsed) = url.parse::<tauri::Url>() {
                        win_for_main
                            .add_child(
                                WebviewBuilder::new(label, WebviewUrl::External(parsed)),
                                LogicalPosition::new(0.0, 0.0),
                                LogicalSize::new(1.0, 1.0),
                            )
                            .map_err(|e| e.to_string())?;
                    } else {
                        eprintln!("[layout] switch: invalid url '{url}' for panel '{label}'");
                    }
                }
                Ok(())
            })();
            let _ = tx.send(result);
        })
        .map_err(|e| DashboardError::Other {
            message: e.to_string(),
        })?;

        rx.recv()
            .map_err(|e| DashboardError::Other {
                message: e.to_string(),
            })?
            .map_err(DashboardError::from)
    }
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Snapshot the current `Inner` state into a `PersistedLayout` for disk write.
fn snapshot_for_persist(inner: &Inner) -> PersistedLayout {
    let meta = inner
        .meta
        .iter()
        .map(|(label, m)| {
            (
                label.clone(),
                PersistedLeafMeta {
                    app_id: m.app_id.clone(),
                    url: m.url.clone(),
                    title: m.title.clone(),
                    engine_binding: m.engine_binding.clone(),
                    display_name: m.display_name.clone(),
                    zoom_factor: m.zoom_factor,
                },
            )
        })
        .collect();
    PersistedLayout {
        tree: inner.root.clone(),
        meta,
        active_panel: inner.active_panel.clone(),
        maximized_stack_id: inner.maximized_stack_id.clone(),
    }
}

impl LayoutTree {
    /// Capture a `PersistedLayout` snapshot and spawn a task to write it to
    /// disk after `debounce_ms` milliseconds.  Any previously-pending write
    /// task is aborted so rapid mutations coalesce into one disk write.
    /// Pass `debounce_ms = 0` for an immediate write (rename / zoom ops).
    fn schedule_save(&self, debounce_ms: u64) {
        let Some(app) = self.app.get() else { return };

        let data_dir = match app.path().app_data_dir() {
            Ok(d) => d,
            Err(e) => {
                eprintln!("[layout] failed to resolve app_data_dir: {e}");
                return;
            }
        };

        // When auto_save is off: just mark dirty and do not touch the
        // persisted dashboard snapshot.
        let auto_save = self.dashboard_store.read().unwrap().auto_save;
        if !auto_save {
            self.dashboard_store.write().unwrap().dirty = true;
            return;
        }

        // Snapshot the current layout and fold it into the active dashboard,
        // then serialise the whole store for the async write task.
        let terminal_persist = {
            let layout = {
                let g = self.inner.read().unwrap();
                snapshot_for_persist(&g)
            };
            let mut ds = self.dashboard_store.write().unwrap();
            ds.snapshot_current(layout);
            ds.to_terminal_persist()
        };

        let terminal_id = self.terminal_id.to_string();
        let mut handle = self.save_handle.lock().unwrap();
        if let Some(h) = handle.take() {
            h.abort();
        }
        *handle = Some(tauri::async_runtime::spawn(async move {
            if debounce_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(debounce_ms)).await;
            }
            if let Err(e) =
                persist::save_terminal_dashboards(&terminal_id, &terminal_persist, &data_dir)
            {
                eprintln!("[layout] persist::save_terminal_dashboards failed: {e}");
            }
        }));
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn short_id() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

fn collect_leaf_labels(node: &LayoutNode, out: &mut Vec<String>) {
    match node {
        LayoutNode::Leaf { label, .. } => out.push(label.clone()),
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
            for c in children {
                collect_leaf_labels(c, out);
            }
        }
    }
}

fn collect_panel_urls(
    node: &LayoutNode,
    meta: &HashMap<String, LeafMeta>,
    out: &mut Vec<(String, String)>,
) {
    match node {
        LayoutNode::Leaf { label, .. } => {
            if let Some(m) = meta.get(label) {
                out.push((label.clone(), m.url.clone()));
            }
        }
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
            for c in children {
                collect_panel_urls(c, meta, out);
            }
        }
    }
}

/// First leaf label (depth-first) inside the Stack with the given `id`, if
/// that stack exists somewhere in `root`. Used to keep `active_panel` inside
/// the maximized stack after its current tab is closed.
fn first_leaf_in_stack_with_id(root: &LayoutNode, stack_id: &str) -> Option<String> {
    let mut path = Vec::new();
    if !find_stack_path_by_id(root, stack_id, &mut path) {
        return None;
    }
    let node = node_at(root, &path)?;
    first_leaf_label(node)
}

/// First leaf encountered in depth-first traversal, if any.
fn first_leaf_label(node: &LayoutNode) -> Option<String> {
    match node {
        LayoutNode::Leaf { label, .. } => Some(label.clone()),
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
            children.iter().find_map(first_leaf_label)
        }
    }
}

/// Walk the tree looking for a Stack with the given `id` and return its path
/// by pushing indices into `acc`. Returns `true` if the id was found; `acc`
/// is left in an unspecified partial state on `false`, so callers should
/// pass a fresh buffer or check the return.
fn find_stack_path_by_id(node: &LayoutNode, id: &str, acc: &mut Vec<usize>) -> bool {
    match node {
        LayoutNode::Leaf { .. } => false,
        LayoutNode::Stack {
            id: node_id,
            children,
            ..
        } => {
            if node_id == id {
                return true;
            }
            for (i, child) in children.iter().enumerate() {
                acc.push(i);
                if find_stack_path_by_id(child, id, acc) {
                    return true;
                }
                acc.pop();
            }
            false
        }
        LayoutNode::Splitter { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                acc.push(i);
                if find_stack_path_by_id(child, id, acc) {
                    return true;
                }
                acc.pop();
            }
            false
        }
    }
}

/// Resolve a reference to the node at `path` in the tree, if the path is valid.
fn node_at<'a>(root: &'a LayoutNode, path: &[usize]) -> Option<&'a LayoutNode> {
    let mut node = root;
    for &i in path {
        match node {
            LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
                node = children.get(i)?;
            }
            LayoutNode::Leaf { .. } => return None,
        }
    }
    Some(node)
}

/// Resolve the label of the leaf at the given path, if the path lands on one.
fn leaf_label_at(root: &LayoutNode, path: &[usize]) -> Option<String> {
    let mut node = root;
    for &i in path {
        match node {
            LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
                node = children.get(i)?;
            }
            LayoutNode::Leaf { .. } => return None,
        }
    }
    match node {
        LayoutNode::Leaf { label, .. } => Some(label.clone()),
        _ => None,
    }
}

/// Walk the tree and collect `PanelBounds` for each leaf that has a chrome
/// panel-header (i.e., not inside a Stack). Stack members are surfaced via
/// tab strips instead and intentionally skipped here.
#[allow(clippy::too_many_arguments)]
fn walk_for_snapshot(
    node: &LayoutNode,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    in_stack: bool,
    panels: &mut Vec<PanelBounds>,
    meta: &HashMap<String, LeafMeta>,
) {
    match node {
        LayoutNode::Leaf { label, .. } => {
            if in_stack {
                return;
            }
            let m = meta.get(label);
            panels.push(PanelBounds {
                id: label.clone(),
                x,
                y,
                width,
                height,
                title: m.map(|m| m.title.clone()).unwrap_or_default(),
                app_id: m.map(|m| m.app_id.clone()).unwrap_or_default(),
                url: m.map(|m| m.url.clone()).unwrap_or_default(),
                engine_binding: m.and_then(|m| m.engine_binding.clone()),
                display_name: m.and_then(|m| m.display_name.clone()),
                zoom_factor: m.map(|m| m.zoom_factor).unwrap_or(1.0),
            });
        }
        LayoutNode::Splitter {
            direction,
            children,
            ..
        } => {
            let n = children.len();
            if n == 0 {
                return;
            }
            let total: f64 = children.iter().map(|c| c.weight().max(0.0)).sum();
            if total <= 0.0 {
                return;
            }
            let axis_total = match direction {
                Direction::Horizontal => width,
                Direction::Vertical => height,
            };
            let gaps = SPLITTER_THICKNESS * n.saturating_sub(1) as f64;
            let content = (axis_total - gaps).max(0.0);

            let mut offset = 0.0;
            for (i, child) in children.iter().enumerate() {
                let frac = child.weight().max(0.0) / total;
                let axis_share = content * frac;
                let (cx, cy, cw, ch) = match direction {
                    Direction::Horizontal => (x + offset, y, axis_share, height),
                    Direction::Vertical => (x, y + offset, width, axis_share),
                };
                walk_for_snapshot(child, cx, cy, cw, ch, false, panels, meta);
                offset += axis_share;
                if i + 1 < n {
                    offset += SPLITTER_THICKNESS;
                }
            }
        }
        LayoutNode::Stack { children, .. } => {
            let content_y = y + TAB_STRIP_HEIGHT;
            let content_h = (height - TAB_STRIP_HEIGHT).max(0.0);
            for child in children {
                walk_for_snapshot(child, x, content_y, width, content_h, true, panels, meta);
            }
        }
    }
}

fn swap_leaves_inner(node: &mut LayoutNode, a: &str, b: &str) -> bool {
    let found_a = contains_label(node, a);
    let found_b = contains_label(node, b);
    if !found_a || !found_b {
        return false;
    }
    relabel(node, a, b);
    true
}

fn contains_label(node: &LayoutNode, label: &str) -> bool {
    match node {
        LayoutNode::Leaf { label: l, .. } => l == label,
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
            children.iter().any(|c| contains_label(c, label))
        }
    }
}

/// Swap every occurrence of `a` with `b` (and vice-versa) in Leaf labels.
fn relabel(node: &mut LayoutNode, a: &str, b: &str) {
    match node {
        LayoutNode::Leaf { label, .. } => {
            if label == a {
                *label = b.to_string();
            } else if label == b {
                *label = a.to_string();
            }
        }
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
            for c in children {
                relabel(c, a, b);
            }
        }
    }
}

/// Recursively walk the tree along `path`, tracking the rect for the
/// node under consideration exactly as `reflow_layout` / `compute_host_layout`
/// do. When `path` is exhausted we are at the target Splitter and mutate the
/// two child weights straddling `child_index`.
#[allow(clippy::too_many_arguments)]
fn apply_splitter_drag(
    node: &mut LayoutNode,
    path: &[usize],
    child_index: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    px: f64,
    py: f64,
) -> bool {
    if path.is_empty() {
        return apply_at_splitter(node, child_index, x, y, w, h, px, py);
    }

    let idx = path[0];
    let rest = &path[1..];
    match node {
        LayoutNode::Leaf { .. } => false,

        LayoutNode::Splitter {
            direction,
            children,
            ..
        } => {
            if idx >= children.len() {
                return false;
            }
            let n = children.len();
            let total: f64 = children.iter().map(|c| c.weight().max(0.0)).sum();
            if total <= 0.0 {
                return false;
            }
            let axis_total = match direction {
                Direction::Horizontal => w,
                Direction::Vertical => h,
            };
            let gaps = SPLITTER_THICKNESS * n.saturating_sub(1) as f64;
            let content = (axis_total - gaps).max(0.0);

            let prior_sum: f64 = children[..idx].iter().map(|c| c.weight().max(0.0)).sum();
            let offset = content * (prior_sum / total) + idx as f64 * SPLITTER_THICKNESS;
            let axis_share = content * (children[idx].weight().max(0.0) / total);

            let (cx, cy, cw, ch) = match direction {
                Direction::Horizontal => (x + offset, y, axis_share, h),
                Direction::Vertical => (x, y + offset, w, axis_share),
            };
            apply_splitter_drag(
                &mut children[idx],
                rest,
                child_index,
                cx,
                cy,
                cw,
                ch,
                px,
                py,
            )
        }

        LayoutNode::Stack { children, .. } => {
            if idx >= children.len() {
                return false;
            }
            let content_y = y + TAB_STRIP_HEIGHT;
            let content_h = (h - TAB_STRIP_HEIGHT).max(0.0);
            apply_splitter_drag(
                &mut children[idx],
                rest,
                child_index,
                x,
                content_y,
                w,
                content_h,
                px,
                py,
            )
        }
    }
}

/// Mutate the two straddling weights at a Splitter so their shared boundary
/// aligns with the cursor. Keeps the sum `w_i + w_{i+1}` invariant and clamps
/// each to `[5%, 95%]` of the combined budget so panels can't collapse.
#[allow(clippy::too_many_arguments)]
fn apply_at_splitter(
    node: &mut LayoutNode,
    child_index: usize,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    px: f64,
    py: f64,
) -> bool {
    let LayoutNode::Splitter {
        direction,
        children,
        ..
    } = node
    else {
        return false;
    };
    let n = children.len();
    if child_index + 1 >= n {
        return false;
    }
    let total: f64 = children.iter().map(|c| c.weight().max(0.0)).sum();
    if total <= 0.0 {
        return false;
    }

    let (axis_total, axis_start, cursor) = match direction {
        Direction::Horizontal => (w, x, px),
        Direction::Vertical => (h, y, py),
    };
    let gaps = SPLITTER_THICKNESS * n.saturating_sub(1) as f64;
    let content = (axis_total - gaps).max(1.0);

    let prior_sum: f64 = children[..child_index]
        .iter()
        .map(|c| c.weight().max(0.0))
        .sum();
    let wi = children[child_index].weight().max(0.0);
    let wi1 = children[child_index + 1].weight().max(0.0);
    let combined = wi + wi1;
    if combined <= 0.0 {
        return false;
    }

    let rel = (cursor - axis_start - child_index as f64 * SPLITTER_THICKNESS) / content * total;
    let target_left_sum = rel.clamp(0.0, total);

    let min_w = combined * 0.05;
    let max_w = combined * 0.95;
    let new_wi = (target_left_sum - prior_sum).clamp(min_w, max_w);
    let new_wi1 = combined - new_wi;

    set_weight(&mut children[child_index], new_wi);
    set_weight(&mut children[child_index + 1], new_wi1);
    true
}

fn set_weight(node: &mut LayoutNode, w: f64) {
    match node {
        LayoutNode::Leaf { weight, .. }
        | LayoutNode::Splitter { weight, .. }
        | LayoutNode::Stack { weight, .. } => *weight = w,
    }
}
