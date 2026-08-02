//! `LayoutTree` — thread-safe store for the N-ary layout tree.
//!
//! One `LayoutTree` lives inside each `TerminalState`. Commands reach it via
//! `manager.get(window.label())?.layout_tree`.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, Window,
};
use uuid::Uuid;

use super::dashboard::{DashboardError, DashboardRegistry, DashboardSession, DashboardsSnapshot};
use super::docking::{add_leaf_as_sibling, append_to_stack_at, is_stack_at, move_leaf, DropZone};
use super::host::{compute_host_layout, HostLayout};
use super::node::{Direction, LayoutNode};
use super::persist::{self, PersistedLayout, PersistedLeafMeta};
use super::reflow::{
    park_label_offscreen, park_offscreen, reflow_layout, SPLITTER_THICKNESS, TAB_STRIP_HEIGHT,
};
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
    /// FDC3 user channel this panel is joined to. `None` = no channel.
    fdc3_channel: Option<String>,
    /// When `true`, switching away from this panel's Dashboard parks its
    /// webview off-screen instead of closing it, so it keeps running and
    /// reappears instantly when the Dashboard becomes active again.
    keep_alive: bool,
    /// Whether the read-only address-bar row (Generic Web Widget panels
    /// only) is shown below the title header. Defaults to `false` — hidden
    /// until the user opts in via the tab context menu.
    show_address_bar: bool,
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
    /// Shared, process-wide dashboard registry (Issue 15-A) — one instance,
    /// passed in via `TerminalManager::dashboards()` and referenced by every
    /// Terminal window's `LayoutTree`. Populated from disk in `init`.
    ///
    /// Lock ordering: whenever a method needs both `dashboards` and
    /// `session`, always acquire `dashboards` first to avoid deadlocking
    /// against another window's command running concurrently.
    dashboards: Arc<RwLock<DashboardRegistry>>,
    /// This window's own session state (active dashboard id / auto_save /
    /// dirty) — per-Terminal, NOT shared, unlike `dashboards` above.
    session: Arc<RwLock<DashboardSession>>,
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
    /// Webview labels that are alive but not part of the active Dashboard's
    /// tree — parked off-screen (see `reflow::park_label_offscreen`) rather
    /// than closed when their `keep_alive`-flagged owning panel's Dashboard
    /// was switched away from. Keyed by label, value is the name of the
    /// Dashboard that owns the panel. Switching back to that Dashboard
    /// reuses the still-running webview instead of recreating it; deleting
    /// that Dashboard closes it.
    parked: Arc<RwLock<HashMap<String, String>>>,
}

impl LayoutTree {
    pub fn new(
        terminal_id: &str,
        width: f64,
        height: f64,
        dashboards: Arc<RwLock<DashboardRegistry>>,
    ) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                root: None,
                meta: HashMap::new(),
                active_panel: None,
                maximized_stack_id: None,
                width,
                height,
            })),
            dashboards,
            session: Arc::new(RwLock::new(DashboardSession::with_empty())),
            app: Arc::new(OnceLock::new()),
            save_handle: Arc::new(Mutex::new(None)),
            terminal_id: Arc::from(terminal_id),
            parked: Arc::new(RwLock::new(HashMap::new())),
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
            let persist = persist::load_terminal(&data_dir);
            // The legacy layout.json migration (inside `load_terminal`, above)
            // may have just written a brand-new dashboard directly to the
            // shared registry *file* — the in-memory registry was already
            // loaded once at process startup (`TerminalManager::load_dashboards_registry`,
            // before this ran) and wouldn't otherwise see it until next
            // launch. Re-merge from disk so this session isn't stuck with an
            // empty layout on the very launch that migrated it.
            if let Some(fresh) = persist::load_registry(&data_dir) {
                let mut registry = self.dashboards.write().unwrap();
                for d in fresh.dashboards {
                    registry.dashboards.entry(d.name.clone()).or_insert(d);
                }
            }
            persist
        } else {
            persist::load_terminal_for(&self.terminal_id, &data_dir)
        };
        if let Some(terminal_persist) = terminal_persist_opt {
            // The shared registry (Issue 15-B) is loaded once, process-wide,
            // by `TerminalManager::load_dashboards_registry` before any
            // terminal's `init` runs — resolve this session's active
            // dashboard against it here rather than merging anything in.
            let session = {
                let registry = self.dashboards.read().unwrap();
                DashboardSession::from_persist(&terminal_persist, &registry)
            };

            // Load the active dashboard's layout into Inner.
            let active_layout = {
                let registry = self.dashboards.read().unwrap();
                registry.get_by_id(&session.active).map(|d| d.as_layout())
            };
            if let Some(layout) = active_layout {
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
                                fdc3_channel: pm.fdc3_channel,
                                keep_alive: pm.keep_alive,
                                show_address_bar: pm.show_address_bar,
                            },
                        )
                    })
                    .collect();
                g.active_panel = layout.active_panel;
                g.maximized_stack_id = layout.maximized_stack_id;
            }

            *self.session.write().unwrap() = session;
        }

        Ok(())
    }

    /// Return every (label, url, fdc3_channel) triple for leaves currently in
    /// the tree so the startup path can recreate webviews for a restored
    /// layout, re-joining each panel to its previously-selected channel.
    pub fn panels_for_restore(&self) -> Vec<(String, String, Option<String>)> {
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
        let address_bar_extra: HashMap<String, f64> = g
            .meta
            .iter()
            .map(|(k, v)| {
                let extra = if v.app_id == super::GENERIC_WEB_WIDGET_APP_ID && v.show_address_bar {
                    super::ADDRESS_BAR_HEIGHT
                } else {
                    0.0
                };
                (k.clone(), extra)
            })
            .collect();

        if let Some(stack_id) = g.maximized_stack_id.as_deref() {
            let mut path = Vec::new();
            if find_stack_path_by_id(root, stack_id, &mut path) {
                if let Some(stack_node) = node_at(root, &path) {
                    park_offscreen(root, app);
                    reflow_layout(
                        stack_node,
                        app,
                        0.0,
                        HEADER_HEIGHT,
                        g.width,
                        h,
                        &address_bar_extra,
                    );
                    return;
                }
            }
        }

        reflow_layout(
            root,
            app,
            0.0,
            HEADER_HEIGHT,
            g.width,
            h,
            &address_bar_extra,
        );
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
        let urls: HashMap<String, String> = g
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.url.clone()))
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
        let fdc3_channels: HashMap<String, Option<String>> = g
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.fdc3_channel.clone()))
            .collect();
        let keep_alives: HashMap<String, bool> = g
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.keep_alive))
            .collect();
        let show_address_bars: HashMap<String, bool> = g
            .meta
            .iter()
            .map(|(k, v)| (k.clone(), v.show_address_bar))
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
            &urls,
            &app_ids,
            &display_names,
            &zoom_factors,
            &fdc3_channels,
            &keep_alives,
            &show_address_bars,
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

    /// Return the FDC3 channel currently joined by `label`, if any. Returns
    /// `None` both when the panel doesn't exist and when it exists but has
    /// no channel — callers that need to distinguish should check panel
    /// existence separately (e.g. via `labels_in_stack` or a tree walk).
    pub fn fdc3_channel(&self, label: &str) -> Option<String> {
        self.inner
            .read()
            .unwrap()
            .meta
            .get(label)
            .and_then(|m| m.fdc3_channel.clone())
    }

    /// Whether closing `name` would silently discard *live* state — either
    /// it's the active dashboard with unsaved changes (auto-save off), or it
    /// owns panels currently parked (kept alive) in the background. Used by
    /// `wm_close_dashboard` only — `wm_delete_dashboard` only ever operates
    /// on already-closed dashboards, which can be neither dirty nor own
    /// parked panels (both conditions require being open/active), so it
    /// always confirms unconditionally instead of consulting this check.
    /// Closing the last remaining *open* dashboard is unaffected by this
    /// check — that's intentional, existing behavior, not a state-loss risk.
    pub fn dashboard_needs_confirm_close(&self, name: &str) -> bool {
        let dirty = {
            let registry = self.dashboards.read().unwrap();
            let session = self.session.read().unwrap();
            registry.id_of(name).as_deref() == Some(session.active.as_str())
                && session.dirty
                && !session.auto_save
        };
        if dirty {
            return true;
        }
        self.parked
            .read()
            .unwrap()
            .values()
            .any(|owner| owner == name)
    }

    /// Set (or clear) the FDC3 channel for the panel identified by `label`.
    /// Returns `false` if no panel with that label exists.
    pub fn set_fdc3_channel(&self, label: &str, channel_id: Option<String>) -> bool {
        let found = {
            let mut g = self.inner.write().unwrap();
            let Some(meta) = g.meta.get_mut(label) else {
                return false;
            };
            meta.fdc3_channel = channel_id;
            true
        };
        if found {
            // Infrequent, user-driven — write immediately, no debounce.
            self.schedule_save(0);
        }
        found
    }

    /// Set the FDC3 channel for every panel currently in the live tree (i.e.
    /// every panel belonging to the active Dashboard). Returns the affected
    /// labels so the caller can push the join/leave script into each
    /// webview. Does not touch the persisted `DashboardRegistry` entry — the
    /// caller (`wm_set_dashboard_default_channel`) is responsible for that.
    pub fn set_fdc3_channel_for_all(&self, channel_id: Option<String>) -> Vec<String> {
        let labels: Vec<String> = {
            let mut g = self.inner.write().unwrap();
            let labels: Vec<String> = g.meta.keys().cloned().collect();
            for meta in g.meta.values_mut() {
                meta.fdc3_channel = channel_id.clone();
            }
            labels
        };
        if !labels.is_empty() {
            self.schedule_save(0);
        }
        labels
    }

    /// Set the keep-alive flag for every panel currently in the live tree
    /// (i.e. every panel belonging to the active Dashboard). Returns the
    /// affected labels.
    pub fn set_keep_alive_for_all(&self, keep_alive: bool) -> Vec<String> {
        let labels: Vec<String> = {
            let mut g = self.inner.write().unwrap();
            let labels: Vec<String> = g.meta.keys().cloned().collect();
            for meta in g.meta.values_mut() {
                meta.keep_alive = keep_alive;
            }
            labels
        };
        if !labels.is_empty() {
            self.schedule_save(0);
        }
        labels
    }

    /// Return whether the panel identified by `label` is flagged to keep
    /// running in the background across Dashboard switches. `false` both
    /// when the panel doesn't exist and when the flag is unset.
    pub fn keep_alive(&self, label: &str) -> bool {
        self.inner
            .read()
            .unwrap()
            .meta
            .get(label)
            .map(|m| m.keep_alive)
            .unwrap_or(false)
    }

    /// Set (or clear) the keep-alive flag for the panel identified by
    /// `label`. Returns `false` if no panel with that label exists.
    pub fn set_keep_alive(&self, label: &str, keep_alive: bool) -> bool {
        let found = {
            let mut g = self.inner.write().unwrap();
            let Some(meta) = g.meta.get_mut(label) else {
                return false;
            };
            meta.keep_alive = keep_alive;
            true
        };
        if found {
            // Infrequent, user-driven — write immediately, no debounce.
            self.schedule_save(0);
        }
        found
    }

    /// Update `LeafMeta.url` to reflect live navigation inside the panel's
    /// own webview (link clicks, redirects, SPA route changes), so the
    /// Generic Web Widget address-bar row tracks where the user actually is
    /// rather than staying frozen at the launch URL. Scoped to Generic Web
    /// Widget panels only — other app types may navigate/redirect
    /// internally (auth flows, etc.) in ways that shouldn't silently
    /// overwrite their `LeafMeta.url`. Returns `true` iff the field changed,
    /// so the caller knows whether a snapshot re-emit is warranted.
    pub fn track_navigated_url(&self, label: &str, url: &str) -> bool {
        let changed = {
            let mut g = self.inner.write().unwrap();
            let Some(meta) = g.meta.get_mut(label) else {
                return false;
            };
            if meta.app_id != super::GENERIC_WEB_WIDGET_APP_ID || meta.url == url {
                return false;
            }
            meta.url = url.to_string();
            true
        };
        if changed {
            // Debounced — SPA route changes can fire in quick bursts.
            self.schedule_save(500);
        }
        changed
    }

    /// Set (or clear) the address-bar visibility flag for the panel
    /// identified by `label`. Returns `false` if no panel with that label
    /// exists. Triggers a reflow-affecting change — callers must follow up
    /// with `reflow()` to resize the webview into the freed/reserved space.
    pub fn set_show_address_bar(&self, label: &str, show_address_bar: bool) -> bool {
        let found = {
            let mut g = self.inner.write().unwrap();
            let Some(meta) = g.meta.get_mut(label) else {
                return false;
            };
            meta.show_address_bar = show_address_bar;
            true
        };
        if found {
            // Infrequent, user-driven — write immediately, no debounce.
            self.schedule_save(0);
        }
        found
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
        // New panels join the active dashboard's default FDC3 channel, if
        // one is set (see `DashboardRegistry::set_default_channel`).
        let default_channel = {
            let registry = self.dashboards.read().unwrap();
            let session = self.session.read().unwrap();
            registry
                .get_by_id(&session.active)
                .and_then(|d| d.default_fdc3_channel.clone())
        };
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
                    fdc3_channel: default_channel,
                    keep_alive: false,
                    show_address_bar: false,
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

    /// Return the current dashboard list state without emitting an event.
    /// This is the *raw* snapshot — no lock-ownership info (`lockedBy`),
    /// since locks live in `TerminalManager`, not here. Emitting to the
    /// frontend should always go through `TerminalManager::emit_dashboards_for`
    /// / `emit_dashboards_all`, which enrich this before sending.
    pub fn dashboards_snapshot(&self) -> DashboardsSnapshot {
        let parked_count = self.parked.read().unwrap().len();
        let registry = self.dashboards.read().unwrap();
        let session = self.session.read().unwrap();
        registry.snapshot(&session, parked_count)
    }

    /// Write the shared registry + this session to disk immediately without
    /// touching `Inner`. Used by metadata-only mutations (create, rename,
    /// delete, reorder).
    pub fn persist_dashboards(&self) {
        let Some(app) = self.app.get() else { return };
        let (terminal_persist, registry_persist) = {
            let registry = self.dashboards.read().unwrap();
            let session = self.session.read().unwrap();
            (registry.to_terminal_persist(&session), registry.to_persisted())
        };
        if let Ok(data_dir) = app.path().app_data_dir() {
            let tid = self.terminal_id.to_string();
            if let Err(e) = persist::save_terminal_dashboards(&tid, &terminal_persist, &data_dir) {
                eprintln!("[layout] persist_dashboards: {e}");
            }
            if let Err(e) = persist::save_registry(&data_dir, &registry_persist) {
                eprintln!("[layout] persist_dashboards (registry): {e}");
            }
        }
    }

    /// Expose a write-locked reference to the shared dashboard registry for
    /// commands that mutate dashboard *content* (create / rename / delete /
    /// reorder / set-default-channel / set-keep-alive-all). Visible to every
    /// Terminal window sharing this registry (Issue 15-A).
    pub fn with_registry_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut DashboardRegistry) -> R,
    {
        f(&mut self.dashboards.write().unwrap())
    }

    /// Read-only access to the shared dashboard registry.
    pub fn with_registry<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&DashboardRegistry) -> R,
    {
        f(&self.dashboards.read().unwrap())
    }

    /// `true` iff `name` is this window's own active dashboard.
    pub fn is_active_dashboard(&self, name: &str) -> bool {
        let registry = self.dashboards.read().unwrap();
        let session = self.session.read().unwrap();
        registry.id_of(name).as_deref() == Some(session.active.as_str())
    }

    /// This window's active dashboard id (empty string = none).
    pub fn active_dashboard_id(&self) -> String {
        self.session.read().unwrap().active.clone()
    }

    /// `true` iff this session's live layout has diverged from its
    /// persisted active dashboard. Only ever set when `auto_save` is off —
    /// used by "Move here" (Issue 15-G) to decide whether evicting this
    /// terminal from its active dashboard needs a save-then-move confirm.
    pub fn session_dirty(&self) -> bool {
        self.session.read().unwrap().dirty
    }

    /// Force this session to "no active dashboard" and clear whatever `init`
    /// optimistically loaded into the live tree (Issue 15-D). Used only for
    /// the narrow startup race its design notes call out: two terminals
    /// restored in the same launch both resolve to what's now the same
    /// shared dashboard id (should be rare-to-impossible after Issue 15-B's
    /// migration renames name collisions, but defended against rather than
    /// left to crash or silently double-lock).
    pub fn clear_active_for_lock_conflict(&self) {
        {
            let mut g = self.inner.write().unwrap();
            g.root = None;
            g.meta.clear();
            g.active_panel = None;
            g.maximized_stack_id = None;
        }
        self.session.write().unwrap().active = String::new();
    }

    /// Set auto-save mode. When switching from off→on, the live layout is
    /// immediately snapshotted and persisted, clearing the dirty flag.
    pub fn set_auto_save(&self, enabled: bool) {
        let should_flush = {
            let mut session = self.session.write().unwrap();
            let was_off = !session.auto_save;
            session.auto_save = enabled;
            was_off && enabled && session.dirty
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
        let (terminal_persist, registry_persist) = {
            let mut registry = self.dashboards.write().unwrap();
            let mut session = self.session.write().unwrap();
            registry.snapshot_current(&session.active, layout);
            session.dirty = false;
            (registry.to_terminal_persist(&session), registry.to_persisted())
        };
        let Some(app) = self.app.get() else { return };
        if let Ok(data_dir) = app.path().app_data_dir() {
            let tid = self.terminal_id.to_string();
            if let Err(e) = persist::save_terminal_dashboards(&tid, &terminal_persist, &data_dir) {
                eprintln!("[layout] save_dashboard: {e}");
            }
            if let Err(e) = persist::save_registry(&data_dir, &registry_persist) {
                eprintln!("[layout] save_dashboard (registry): {e}");
            }
        }
    }

    /// Reload the saved active dashboard snapshot into `Inner`, discarding all
    /// unsaved live changes, and reconcile panel webviews on the main thread.
    /// Clears the dirty flag. `win` is required to create any missing
    /// webviews; `panel_init_script` is injected into each recreated panel
    /// (base FDC3 agent config + a per-panel channel rejoin, see
    /// [`TerminalConfig::panel_init_script`](crate::config::TerminalConfig::panel_init_script)).
    pub fn discard_dashboard(
        &self,
        win: &Window,
        app: &AppHandle,
        panel_init_script: &str,
    ) -> Result<(), DashboardError> {
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
            let registry = self.dashboards.read().unwrap();
            let mut session = self.session.write().unwrap();
            session.dirty = false;
            registry.get_by_id(&session.active).map(|d| d.as_layout())
        };
        let panels_to_create = self.apply_layout_to_inner(clean_layout);

        // Reconcile webviews on the main thread. No cross-dashboard parking
        // here — discard stays within the active Dashboard.
        self.reconcile_panel_webviews(
            current_labels,
            Vec::new(),
            panels_to_create,
            win,
            app,
            panel_init_script,
        )?;

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
        panel_init_script: &str,
    ) -> Result<(), DashboardError> {
        // Validate and check dirty state without holding any write lock.
        let target_id = {
            let registry = self.dashboards.read().unwrap();
            let session = self.session.read().unwrap();
            let Some(target_id) = registry.id_of(name) else {
                return Err(DashboardError::NotFound);
            };
            if session.active == target_id {
                return Ok(());
            }
            if !session.auto_save && session.dirty {
                return Err(DashboardError::NeedsConfirm);
            }
            target_id
        };

        // Capture the outgoing dashboard's name — it owns any panels we park.
        let outgoing_name = {
            let registry = self.dashboards.read().unwrap();
            let session = self.session.read().unwrap();
            registry.name_of(&session.active).unwrap_or_default()
        };

        // Collect current panel labels, split into those to destroy and
        // those flagged `keep_alive` to park off-screen instead.
        let (to_destroy, to_park): (Vec<String>, Vec<String>) = {
            let g = self.inner.read().unwrap();
            let mut labels = Vec::new();
            if let Some(root) = &g.root {
                collect_leaf_labels(root, &mut labels);
            }
            labels
                .into_iter()
                .partition(|label| !g.meta.get(label).map(|m| m.keep_alive).unwrap_or(false))
        };
        if !to_park.is_empty() {
            let mut parked = self.parked.write().unwrap();
            for label in &to_park {
                parked.insert(label.clone(), outgoing_name.clone());
            }
        }

        // Snapshot outgoing dashboard if auto_save is on, then switch active.
        let new_layout = {
            let mut registry = self.dashboards.write().unwrap();
            let mut session = self.session.write().unwrap();
            if session.auto_save {
                let layout = {
                    let g = self.inner.read().unwrap();
                    snapshot_for_persist(&g)
                };
                registry.snapshot_current(&session.active, layout);
            }
            session.active = target_id.clone();
            session.dirty = false;
            registry.get_by_id(&target_id).map(|d| d.as_layout())
        };

        // Load new dashboard into Inner and collect panels to create. Any
        // label already parked (typically because we're returning to a
        // Dashboard whose kept-alive panels we parked earlier) is reused —
        // its webview never gets destroyed, so skip recreating it.
        let panels_to_create = self.apply_layout_to_inner(new_layout);
        let panels_to_create: Vec<(String, String, Option<String>)> = {
            let mut parked = self.parked.write().unwrap();
            panels_to_create
                .into_iter()
                .filter(|(label, _, _)| parked.remove(label).is_none())
                .collect()
        };

        // Destroy/park old webviews + create new ones on the main thread.
        self.reconcile_panel_webviews(
            to_destroy,
            to_park,
            panels_to_create,
            win,
            app,
            panel_init_script,
        )?;

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

    /// Switch this session away from its current active dashboard to a
    /// fallback (the first open dashboard not in `locked_elsewhere`, or
    /// none), without touching the registry entry itself — used by "Move
    /// here" (Issue 15-G) when another window takes over this terminal's
    /// active dashboard out from under it. No-op if this terminal has no
    /// active dashboard.
    ///
    /// Mirrors `switch_dashboard`'s outgoing-dashboard handling (snapshot if
    /// `auto_save`, park `keep_alive` panels, destroy the rest) but picks
    /// the incoming dashboard automatically rather than by name, and never
    /// discards unsaved edits itself — callers must already have saved (or
    /// obtained confirmation to discard) before calling this, since by the
    /// time this runs there's no user in this window to ask.
    pub fn force_switch_away(
        &self,
        locked_elsewhere: &HashSet<String>,
        win: &Window,
        app: &AppHandle,
        panel_init_script: &str,
    ) -> Result<(), DashboardError> {
        let outgoing_id = self.session.read().unwrap().active.clone();
        if outgoing_id.is_empty() {
            return Ok(());
        }
        let outgoing_name = self
            .dashboards
            .read()
            .unwrap()
            .name_of(&outgoing_id)
            .unwrap_or_default();

        let (to_destroy, to_park): (Vec<String>, Vec<String>) = {
            let g = self.inner.read().unwrap();
            let mut labels = Vec::new();
            if let Some(root) = &g.root {
                collect_leaf_labels(root, &mut labels);
            }
            labels
                .into_iter()
                .partition(|label| !g.meta.get(label).map(|m| m.keep_alive).unwrap_or(false))
        };
        if !to_park.is_empty() {
            let mut parked = self.parked.write().unwrap();
            for label in &to_park {
                parked.insert(label.clone(), outgoing_name.clone());
            }
        }

        let new_layout = {
            let mut registry = self.dashboards.write().unwrap();
            let mut session = self.session.write().unwrap();
            if session.auto_save {
                let layout = {
                    let g = self.inner.read().unwrap();
                    snapshot_for_persist(&g)
                };
                registry.snapshot_current(&outgoing_id, layout);
            }
            let new_active_id = registry
                .first_open_name_excluding(locked_elsewhere)
                .and_then(|n| registry.id_of(&n))
                .unwrap_or_default();
            session.active = new_active_id.clone();
            session.dirty = false;
            registry.get_by_id(&new_active_id).map(|d| d.as_layout())
        };

        let panels_to_create = self.apply_layout_to_inner(new_layout);
        let panels_to_create: Vec<(String, String, Option<String>)> = {
            let mut parked = self.parked.write().unwrap();
            panels_to_create
                .into_iter()
                .filter(|(label, _, _)| parked.remove(label).is_none())
                .collect()
        };

        self.reconcile_panel_webviews(
            to_destroy,
            to_park,
            panels_to_create,
            win,
            app,
            panel_init_script,
        )?;

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

        Ok(())
    }

    /// Delete a dashboard by name, reconciling panel webviews when the active
    /// dashboard is the one being removed. Returns `false` if `name` doesn't
    /// exist. Allowed to delete the last dashboard, leaving the terminal empty.
    ///
    /// `locked_elsewhere` (Issue 15-D) — dashboard ids currently active in
    /// some *other* Terminal window — is consulted only when picking this
    /// session's fallback active dashboard (if `name` was active): the
    /// fallback must never auto-pick something already active elsewhere,
    /// or this window would silently end up displaying a dashboard another
    /// window has locked. Callers should pass
    /// `manager.locked_dashboard_ids_excluding(this_terminal_id)`.
    pub fn delete_dashboard(
        &self,
        name: &str,
        locked_elsewhere: &HashSet<String>,
        win: &Window,
        app: &AppHandle,
        panel_init_script: &str,
    ) -> Result<bool, DashboardError> {
        // Check existence and whether this is the active dashboard.
        let (is_active, current_labels) = {
            let registry = self.dashboards.read().unwrap();
            let session = self.session.read().unwrap();
            if !registry.dashboards.contains_key(name) {
                return Ok(false);
            }
            let is_active = registry.id_of(name).as_deref() == Some(session.active.as_str());
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

        self.with_registry_mut(|r| r.delete(name));

        // The deleted Dashboard may own panels parked off-screen (kept alive
        // while some other Dashboard was active) — there's no home for them
        // to return to, so close them now instead of leaking the webviews.
        self.close_parked_for_dashboard(name, app)?;

        if !is_active {
            return Ok(true);
        }

        // Deleted the active dashboard — reassign this session's active to
        // whatever's now the first open dashboard in the registry (or none).
        let new_layout = {
            let registry = self.dashboards.read().unwrap();
            let mut session = self.session.write().unwrap();
            let new_active_id = registry
                .first_open_name_excluding(locked_elsewhere)
                .and_then(|n| registry.id_of(&n))
                .unwrap_or_default();
            session.active = new_active_id.clone();
            registry.get_by_id(&new_active_id).map(|d| d.as_layout())
        };
        let panels_to_create = self.apply_layout_to_inner(new_layout);
        self.reconcile_panel_webviews(
            current_labels,
            Vec::new(),
            panels_to_create,
            win,
            app,
            panel_init_script,
        )?;

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

    /// Hide a dashboard from the switcher without deleting it — its layout,
    /// per-leaf metadata, and default channel all stay intact in the
    /// `DashboardRegistry` so `reopen_dashboard` can bring it back exactly as
    /// it was. Reconciles panel webviews the same way `delete_dashboard`
    /// does when `name` is the active dashboard (switches to the next open
    /// one, or empties the terminal if none remain open), and closes any of
    /// `name`'s panels currently parked in the background — a closed
    /// dashboard doesn't keep widgets running, same as a deleted one.
    /// Returns `false` if `name` doesn't exist.
    ///
    /// `locked_elsewhere` (Issue 15-D) — see `delete_dashboard`'s doc
    /// comment; same contract, used the same way for the fallback pick.
    pub fn close_dashboard(
        &self,
        name: &str,
        locked_elsewhere: &HashSet<String>,
        win: &Window,
        app: &AppHandle,
        panel_init_script: &str,
    ) -> Result<bool, DashboardError> {
        // Check existence and whether this is the active dashboard.
        let (is_active, current_labels) = {
            let registry = self.dashboards.read().unwrap();
            let session = self.session.read().unwrap();
            if !registry.dashboards.contains_key(name) {
                return Ok(false);
            }
            let is_active = registry.id_of(name).as_deref() == Some(session.active.as_str());
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

        self.with_registry_mut(|r| r.close(name));

        // The closed Dashboard may own panels parked off-screen (kept alive
        // while some other Dashboard was active) — it's no longer reachable
        // until reopened, so close them now instead of leaking the webviews.
        self.close_parked_for_dashboard(name, app)?;

        if !is_active {
            return Ok(true);
        }

        // Closed the active dashboard — reassign this session's active to
        // whatever's now the first open dashboard in the registry (or none).
        let new_layout = {
            let registry = self.dashboards.read().unwrap();
            let mut session = self.session.write().unwrap();
            let new_active_id = registry
                .first_open_name_excluding(locked_elsewhere)
                .and_then(|n| registry.id_of(&n))
                .unwrap_or_default();
            session.active = new_active_id.clone();
            registry.get_by_id(&new_active_id).map(|d| d.as_layout())
        };
        let panels_to_create = self.apply_layout_to_inner(new_layout);
        self.reconcile_panel_webviews(
            current_labels,
            Vec::new(),
            panels_to_create,
            win,
            app,
            panel_init_script,
        )?;

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

    /// Make a closed dashboard selectable in the switcher again, without
    /// changing which dashboard is currently active — the user picks it
    /// from the switcher (or Manage drawer) like any other pill. Returns
    /// `false` if `name` doesn't exist.
    pub fn reopen_dashboard(&self, name: &str) -> bool {
        let reopened = self.with_registry_mut(|r| r.reopen(name));
        if reopened {
            self.schedule_save(0);
        }
        reopened
    }

    /// Load a `PersistedLayout` (or empty layout when `None`) into `Inner`.
    /// Returns the `(label, url, fdc3_channel)` triples of all panels in the
    /// new layout, ordered depth-first.
    fn apply_layout_to_inner(
        &self,
        layout: Option<PersistedLayout>,
    ) -> Vec<(String, String, Option<String>)> {
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
                                fdc3_channel: pm.fdc3_channel,
                                keep_alive: pm.keep_alive,
                                show_address_bar: pm.show_address_bar,
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
    /// webviews for each `(label, url, fdc3_channel)` triple in `to_create`.
    /// Dispatched on the main thread because `add_child` requires it on
    /// Windows/macOS. `panel_init_script` is the base script (FDC3 agent URL,
    /// etc.); when a panel has a saved channel, `OT_FDC3_INITIAL_CHANNEL` is
    /// appended so `fdc3-plugin.js` rejoins it once the recreated webview's
    /// handshake completes.
    fn reconcile_panel_webviews(
        &self,
        to_destroy: Vec<String>,
        to_park: Vec<String>,
        to_create: Vec<(String, String, Option<String>)>,
        win: &Window,
        app: &AppHandle,
        panel_init_script: &str,
    ) -> Result<(), DashboardError> {
        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_for_main = app.clone();
        let win_for_main = win.clone();
        let base_script = panel_init_script.to_string();
        let tree_for_main = self.clone();
        let terminal_id_for_main = self.terminal_id.to_string();

        app.run_on_main_thread(move || {
            let result = (|| -> Result<(), String> {
                for label in &to_destroy {
                    if let Some(wv) = app_for_main.get_webview(label) {
                        wv.close().map_err(|e| e.to_string())?;
                    }
                }
                // Kept-alive panels: don't close, just move off-screen so the
                // webview (and its JS state / running requests) survives
                // until its owning Dashboard becomes active again.
                for label in &to_park {
                    park_label_offscreen(&app_for_main, label);
                }
                for (label, url, channel) in &to_create {
                    if let Ok(parsed) = url.parse::<tauri::Url>() {
                        let script =
                            crate::config::append_initial_channel(&base_script, channel.as_deref());
                        win_for_main
                            .add_child(
                                WebviewBuilder::new(label, WebviewUrl::External(parsed))
                                    .initialization_script(&script)
                                    .on_navigation(crate::address_bar_navigation_handler(
                                        app_for_main.clone(),
                                        terminal_id_for_main.clone(),
                                        label.clone(),
                                        tree_for_main.clone(),
                                    )),
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

    /// Repoint every `parked` entry owned by `old_name` to `new_name`. Must
    /// be called whenever a Dashboard is renamed — the `parked` registry
    /// tracks ownership by name, so without this a rename would silently
    /// orphan any panels parked while that Dashboard was in the background:
    /// `close_parked_for_dashboard` would never find them by the old name on
    /// delete (leaking the webview), and switching back under the new name
    /// still works (matched by label, not owner) but the stale owner would
    /// permanently inflate `parked_count`.
    pub fn rename_parked_owner(&self, old_name: &str, new_name: &str) {
        let mut parked = self.parked.write().unwrap();
        for owner in parked.values_mut() {
            if owner == old_name {
                *owner = new_name.to_string();
            }
        }
    }

    /// Close every parked (kept-alive) webview owned by `dashboard_name` and
    /// remove them from the `parked` registry. Called when that Dashboard is
    /// deleted — there is no home left for the panel to be reused by, so it
    /// must be torn down instead of leaking a hidden webview forever.
    fn close_parked_for_dashboard(
        &self,
        dashboard_name: &str,
        app: &AppHandle,
    ) -> Result<(), DashboardError> {
        let labels: Vec<String> = {
            let mut parked = self.parked.write().unwrap();
            let labels: Vec<String> = parked
                .iter()
                .filter(|(_, owner)| owner.as_str() == dashboard_name)
                .map(|(label, _)| label.clone())
                .collect();
            for label in &labels {
                parked.remove(label);
            }
            labels
        };
        if labels.is_empty() {
            return Ok(());
        }

        let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_for_main = app.clone();
        app.run_on_main_thread(move || {
            let result = (|| -> Result<(), String> {
                for label in &labels {
                    if let Some(wv) = app_for_main.get_webview(label) {
                        wv.close().map_err(|e| e.to_string())?;
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
                    fdc3_channel: m.fdc3_channel.clone(),
                    keep_alive: m.keep_alive,
                    show_address_bar: m.show_address_bar,
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
        let auto_save = self.session.read().unwrap().auto_save;
        if !auto_save {
            self.session.write().unwrap().dirty = true;
            return;
        }

        // Snapshot the current layout and fold it into the active dashboard,
        // then serialise both the session and the shared registry for the
        // async write task.
        let (terminal_persist, registry_persist) = {
            let layout = {
                let g = self.inner.read().unwrap();
                snapshot_for_persist(&g)
            };
            let mut registry = self.dashboards.write().unwrap();
            let session = self.session.read().unwrap();
            registry.snapshot_current(&session.active, layout);
            (registry.to_terminal_persist(&session), registry.to_persisted())
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
            if let Err(e) = persist::save_registry(&data_dir, &registry_persist) {
                eprintln!("[layout] persist::save_registry failed: {e}");
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
    out: &mut Vec<(String, String, Option<String>)>,
) {
    match node {
        LayoutNode::Leaf { label, .. } => {
            if let Some(m) = meta.get(label) {
                out.push((label.clone(), m.url.clone(), m.fdc3_channel.clone()));
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
                fdc3_channel: m.and_then(|m| m.fdc3_channel.clone()),
                keep_alive: m.map(|m| m.keep_alive).unwrap_or(false),
                show_address_bar: m.map(|m| m.show_address_bar).unwrap_or(false),
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
