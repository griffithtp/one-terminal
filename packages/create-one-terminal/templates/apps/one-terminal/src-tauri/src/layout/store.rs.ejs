//! `LayoutTree` — thread-safe, Tauri-managed store for the N-ary layout tree.
//!
//! Registered once at startup via `app.manage(LayoutTree::new(w, h))` and
//! accessed from commands via `State<'_, LayoutTree>`.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use super::docking::{add_leaf_as_sibling, append_to_stack_at, is_stack_at, move_leaf, DropZone};
use super::host::compute_host_layout;
use super::node::{Direction, LayoutNode};
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

#[derive(Clone, Default)]
pub struct LayoutTree(Arc<RwLock<Inner>>);

impl LayoutTree {
    pub fn new(width: f64, height: f64) -> Self {
        Self(Arc::new(RwLock::new(Inner {
            root: None,
            meta: HashMap::new(),
            active_panel: None,
            maximized_stack_id: None,
            width,
            height,
        })))
    }

    pub fn set_size(&self, width: f64, height: f64) {
        let mut g = self.0.write().unwrap();
        g.width = width;
        g.height = height;
    }

    pub fn size(&self) -> (f64, f64) {
        let g = self.0.read().unwrap();
        (g.width, g.height)
    }

    /// Read-only access to the root. Returns `None` if the tree is empty.
    pub fn with_root<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&LayoutNode) -> R,
    {
        let g = self.0.read().unwrap();
        g.root.as_ref().map(f)
    }

    /// Replace the root wholesale. Clears meta/active tracking since labels
    /// in the new tree may not match prior state — callers are responsible
    /// for repopulating metadata if needed.
    pub fn set_root(&self, root: Option<LayoutNode>) {
        let mut g = self.0.write().unwrap();
        g.root = root;
        g.meta.clear();
        g.active_panel = None;
        g.maximized_stack_id = None;
    }

    /// Toggle the maximize state for the Stack at `path`. If this stack is
    /// already maximized, restores the normal layout. Otherwise captures the
    /// stack's stable id so the maximize state survives tab adds/removes
    /// that shift sibling indices. Rejects paths that don't resolve to a
    /// Stack so callers can dispatch without pre-checks. Returns `true` iff
    /// state changed.
    pub fn toggle_maximize_stack(&self, path: &[usize]) -> bool {
        let mut g = self.0.write().unwrap();
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
            return true;
        }
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

    /// Currently-active panel label, if any.
    #[allow(dead_code)]
    pub fn active_panel(&self) -> Option<String> {
        self.0.read().unwrap().active_panel.clone()
    }

    /// Mark `label` as the active panel. Caller is responsible for ensuring
    /// `label` exists in the tree.
    #[allow(dead_code)]
    pub fn set_active_panel(&self, label: &str) {
        self.0.write().unwrap().active_panel = Some(label.to_string());
    }

    /// Reflow every webview from the current tree and window dimensions.
    /// Content area starts below the global chrome header (HEADER_HEIGHT).
    /// When a stack is maximized, every leaf outside it is parked offscreen
    /// and the maximized stack is reflowed at the full content rect.
    pub fn reflow(&self, app: &AppHandle) {
        let g = self.0.read().unwrap();
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

    /// Emit the host-shell projection (tab strips + splitter handles) so the
    /// chrome webview can render its overlays on top of the panel "holes".
    /// Matches reflow's HEADER_HEIGHT offset so overlays align with webviews.
    pub fn emit_host(&self, app: &AppHandle) {
        let g = self.0.read().unwrap();
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
        let payload = compute_host_layout(
            g.root.as_ref(),
            max_path,
            0.0,
            HEADER_HEIGHT,
            g.width,
            h,
            &titles,
            &app_ids,
        );
        let _ = app.emit("wm:host-layout", &payload);
    }

    /// Update the display title for the panel identified by `label`.
    /// Returns `true` if the panel exists (and was updated); `false` otherwise.
    pub fn rename_panel(&self, label: &str, title: &str) -> bool {
        let mut g = self.0.write().unwrap();
        let Some(meta) = g.meta.get_mut(label) else {
            return false;
        };
        meta.title = title.to_string();
        true
    }

    /// Synthesize a `LayoutSnapshot` for the legacy `wm:layout` event. Only
    /// leaves *not* inside a Stack are surfaced as panels — Stack members get
    /// their headers from the tab strip (`wm:host-layout`) instead.
    pub fn snapshot(&self) -> Option<LayoutSnapshot> {
        let g = self.0.read().unwrap();
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
        let g = self.0.read().unwrap();
        if let Some(root) = g.root.as_ref() {
            park_offscreen(root, app);
        }
    }

    /// Add a new panel to the tree.
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
        app_id: &str,
        url: &str,
        title: &str,
        target: Option<&str>,
        dir: Option<SplitDir>,
        engine_binding: Option<ot_core::engine::EngineBinding>,
    ) -> String {
        let label = format!("panel-{}", short_id());

        let mut g = self.0.write().unwrap();
        g.meta.insert(
            label.clone(),
            LeafMeta {
                app_id: app_id.into(),
                url: url.into(),
                title: title.into(),
                engine_binding,
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
            return label;
        }

        // Resolve the target label: explicit arg > active_panel > first leaf.
        let target_label = target
            .map(|s| s.to_string())
            .or_else(|| g.active_panel.clone())
            .or_else(|| first_leaf_label(g.root.as_ref().unwrap()));

        let Some(target_label) = target_label else {
            // Unreachable given root is Some, but guard anyway.
            g.root = Some(leaf);
            g.active_panel = Some(label.clone());
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
                // Explicit split direction.
                let zone = match d {
                    SplitDir::Horizontal => DropZone::Right,
                    SplitDir::Vertical => DropZone::Bottom,
                };
                add_leaf_as_sibling(&mut g.root, &target_path, zone, leaf);
            }
            None => {
                // Tab-insert. If target's parent is a Stack, append there;
                // otherwise wrap target + new leaf into a Stack (Center drop).
                let parent_is_stack = if target_path.is_empty() {
                    // Target is root itself; Stack check at empty path.
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
        label
    }

    /// Remove a panel: drop its leaf from the tree, forget its metadata, and
    /// pick a replacement active panel if we removed the current one.
    /// Returns `true` if the leaf was found and removed.
    pub fn remove_panel(&self, label: &str) -> bool {
        let mut g = self.0.write().unwrap();
        let removed = super::docking::remove_leaf(&mut g.root, label);
        if !removed {
            return false;
        }
        g.meta.remove(label);
        if g.active_panel.as_deref() == Some(label) {
            // While maximized, keep the active panel inside the maximized
            // stack so subsequent `wm_open` calls (which default target to
            // active_panel) tab-insert into the maximized group.
            let fallback = g.maximized_stack_id.as_deref().and_then(|id| {
                g.root
                    .as_ref()
                    .and_then(|r| first_leaf_in_stack_with_id(r, id))
            });
            g.active_panel = fallback.or_else(|| g.root.as_ref().and_then(first_leaf_label));
        }
        true
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
        let mut g = self.0.write().unwrap();
        let changed = move_leaf(&mut g.root, source_label, target_path, zone, insert_index);
        if changed {
            g.active_panel = Some(source_label.to_string());
        }
        changed
    }

    /// Swap the positions of two leaves identified by label. Used by panel-
    /// header drag (chrome-drawn header on non-stack leaves).
    pub fn swap_leaves(&self, a: &str, b: &str) -> bool {
        if a == b {
            return false;
        }
        let mut g = self.0.write().unwrap();
        let Some(root) = g.root.as_mut() else {
            return false;
        };
        swap_leaves_inner(root, a, b)
    }

    /// Set the active tab on the Stack at `path`. Also marks the chosen leaf
    /// as the active panel so subsequent `wm_open` calls target its group.
    pub fn set_active_tab(&self, path: &[usize], index: usize) -> bool {
        let mut g = self.0.write().unwrap();
        let changed = super::docking::set_active_tab(&mut g.root, path, index);
        if changed {
            // Look up the newly-active child's label.
            if let Some(root) = g.root.as_ref() {
                let mut active_path = path.to_vec();
                active_path.push(index);
                if let Some(label) = leaf_label_at(root, &active_path) {
                    g.active_panel = Some(label);
                }
            }
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
        let g = self.0.read().unwrap();
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
        let mut g = self.0.write().unwrap();
        let (w, h) = (g.width, (g.height - HEADER_HEIGHT).max(0.0));
        let Some(root) = g.root.as_mut() else {
            return false;
        };
        apply_splitter_drag(root, path, child_index, 0.0, HEADER_HEIGHT, w, h, px, py)
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
            // Stack members have their headers from the tab strip — don't emit
            // panel bounds for them. Recurse into the content area anyway so
            // nested Splitters inside a Stack still get their non-stack leaves
            // published.
            let content_y = y + TAB_STRIP_HEIGHT;
            let content_h = (height - TAB_STRIP_HEIGHT).max(0.0);
            for child in children {
                walk_for_snapshot(child, x, content_y, width, content_h, true, panels, meta);
            }
        }
    }
}

fn swap_leaves_inner(node: &mut LayoutNode, a: &str, b: &str) -> bool {
    // Two-pass via raw pointers would be unsafe; instead, snapshot labels and
    // rewrite both occurrences by label substitution. Panels are keyed only
    // by label, so a full-tree label swap is equivalent to a positional swap.
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

    // Boundary between child_index and child_index+1 sits at
    //   axis_start + content * (left_sum / total) + child_index * SPLITTER_THICKNESS
    // Solve for left_sum:
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
