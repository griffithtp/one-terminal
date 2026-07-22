//! Layout-tree IPC commands. The frontend drives the layout by shipping
//! whole trees into `update_layout`; Rust installs them as the source of
//! truth and stitches every webview via `reflow_layout`.

use tauri::{AppHandle, Emitter, Manager, State, Window};

use crate::terminal::TerminalManager;

use super::dashboard::DashboardsSnapshot;
use super::docking::DropZone;
use super::node::LayoutNode;

/// The 8 standard FDC3 2.2 system channel ids. Mirrors `FDC3_CHANNELS` in
/// `Header.tsx` / `OverlayApp.tsx` — kept as a fixed allowlist since
/// `wm_set_panel_fdc3_channel` interpolates the id into a JS snippet
/// evaluated in the target panel's webview.
const SYSTEM_CHANNEL_IDS: [&str; 8] = [
    "fdc3.channel.1",
    "fdc3.channel.2",
    "fdc3.channel.3",
    "fdc3.channel.4",
    "fdc3.channel.5",
    "fdc3.channel.6",
    "fdc3.channel.7",
    "fdc3.channel.8",
];

// ── Helper ────────────────────────────────────────────────────────────────────

/// Resolve the calling window's terminal, returning a descriptive error if the
/// label is not registered. Used by all commands that mutate layout state.
macro_rules! get_terminal {
    ($manager:expr, $window:expr) => {
        match $manager.get($window.label()) {
            Some(t) => t,
            None => return Err(format!("terminal '{}' not found", $window.label())),
        }
    };
}

// ── Layout commands ───────────────────────────────────────────────────────────

/// Replace the layout tree wholesale and reflow every webview.
///
/// The argument is a FlexLayout-shaped tree: each node is `{ "type": "leaf"
/// | "splitter" | "stack", "weight": f64, ... }`. Leaves that reference a
/// webview label not currently alive are silently skipped by the reflow pass.
#[tauri::command]
pub fn update_layout(
    json_tree: LayoutNode,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    terminal.layout_tree.set_root(Some(json_tree));
    terminal.layout_tree.reflow(&app);
    terminal.layout_tree.emit_host(&app);
    Ok(())
}

/// Place the boundary between `children[child_index]` and
/// `children[child_index + 1]` of the Splitter at `path` under window-space
/// `(position_x, position_y)`. High-frequency — called from `pointermove`.
#[tauri::command]
pub fn wm_splitter_drag(
    path: Vec<usize>,
    child_index: usize,
    position_x: f64,
    position_y: f64,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if terminal
        .layout_tree
        .resize_splitter(&path, child_index, position_x, position_y)
    {
        terminal.layout_tree.reflow(&app);
        terminal.layout_tree.emit_host(&app);
    }
    Ok(())
}

/// Tab drag begin — park every panel offscreen so the chrome (which sits
/// beneath panels in z-order) is fully visible while the user drags. The
/// webviews keep their process/session intact; `wm_end_tab_drag` restores
/// them via `reflow`.
#[tauri::command]
pub fn wm_begin_tab_drag(
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    terminal.layout_tree.park_all(&app);
    Ok(())
}

/// Tab drag end — on a valid drop, reparent the Leaf by `source_label` into
/// `target_path` under `zone` semantics; then reflow and republish the host
/// layout. On a cancel (no target/zone) we only reflow, which restores any
/// panels parked by `wm_begin_tab_drag`.
///
/// Session preservation: the moved Leaf reuses its existing webview process,
/// so cookies, storage, sockets, and in-page state all survive the dock. No
/// new webview is created for a drop, so no session inheritance is needed.
/// Make `tab_index` the active (visible) tab on the Stack at `path`. Reflow
/// parks the previously-active child offscreen and restores the new one —
/// the webview process is preserved in both cases, so session state survives
/// tab switches by construction.
#[tauri::command]
pub fn wm_set_active_tab(
    path: Vec<usize>,
    tab_index: usize,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if terminal.layout_tree.set_active_tab(&path, tab_index) {
        terminal.layout_tree.reflow(&app);
        terminal.layout_tree.emit_host(&app);
    }
    Ok(())
}

/// Close the Leaf with `label`: drop it from the tree, destroy its webview,
/// then reflow. If removing the leaf empties a Stack, `simplify` collapses
/// it and any 1-child Splitters bubble up.
#[tauri::command]
pub fn wm_close_leaf(
    label: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if terminal.layout_tree.remove_leaf(&label) {
        if let Some(wv) = app.get_webview(&label) {
            let _ = wv.close();
        }
        terminal.layout_tree.reflow(&app);
        terminal.layout_tree.emit_host(&app);
    }
    Ok(())
}

/// Close a tab by its webview label.
///
/// 1. Look the webview up in Tauri's registry by `label`.
/// 2. Call `.close()` to kill the process and free resources.
/// 3. Remove the matching leaf from `LayoutTree`.
/// 4. Reflow so remaining panels redistribute into the freed space.
///
/// Order: tree-mutate first, then webview close, then reflow. That way
/// `simplify` collapses any orphan stack/splitter before reflow tries to
/// reposition surviving panels.
#[tauri::command]
pub fn close_tab(
    label: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if !terminal.layout_tree.remove_leaf(&label) {
        return Err(format!("no leaf with label '{label}' in layout tree"));
    }
    if let Some(wv) = app.get_webview(&label) {
        wv.close().map_err(|e| e.to_string())?;
    }
    terminal.layout_tree.reflow(&app);
    terminal.layout_tree.emit_host(&app);
    Ok(())
}

/// Close every tab in the Stack at `path` — the "Close group" action.
///
/// 1. Collect every leaf label under the Stack subtree (read-only walk).
/// 2. For each: remove from the tree (which also simplifies any now-empty
///    parents) and destroy the Tauri webview.
/// 3. Reflow + republish so surviving panels redistribute into the freed
///    space.
///
/// No-op if `path` doesn't land on a Stack (e.g., tree is empty, path is
/// stale, or the node is a Leaf/Splitter). This is intentionally tolerant
/// so the frontend can dispatch without pre-checks.
#[tauri::command]
pub fn wm_close_stack(
    path: Vec<usize>,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    let labels = terminal.layout_tree.labels_in_stack(&path);
    if labels.is_empty() {
        return Ok(());
    }
    for label in &labels {
        terminal.layout_tree.remove_panel(label);
        if let Some(wv) = app.get_webview(label) {
            let _ = wv.close();
        }
    }
    terminal.layout_tree.reflow(&app);
    terminal.layout_tree.emit_host(&app);
    Ok(())
}

/// Toggle maximize state for the Stack at `path`. Maximizing parks every
/// leaf outside the stack offscreen and gives the stack the full content
/// rect; the chrome publishes only that one strip (no splitters) so the
/// user can't interact with the hidden layout until they restore. A second
/// call on the same path restores; calls with a path that doesn't resolve
/// to a Stack are silently ignored.
#[tauri::command]
pub fn wm_toggle_maximize_stack(
    path: Vec<usize>,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if terminal.layout_tree.toggle_maximize_stack(&path) {
        terminal.layout_tree.reflow(&app);
        terminal.layout_tree.emit_host(&app);
    }
    Ok(())
}

/// Rename the tab for the panel with `label`. Empty `title` is rejected so
/// the strip always has something to render (the frontend trims + validates
/// before calling, but guard here too).
#[tauri::command]
pub fn wm_rename_tab(
    label: String,
    title: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("title must not be empty".into());
    }
    let terminal = get_terminal!(manager, window);
    if !terminal.layout_tree.rename_panel(&label, trimmed) {
        return Err(format!("no panel with label '{label}'"));
    }
    terminal.layout_tree.emit_host(&app);
    if let Some(snap) = terminal.layout_tree.snapshot() {
        let chrome = format!("{}-chrome", window.label());
        if let Some(wv) = app.get_webview(&chrome) {
            let _ = wv.emit("wm:layout", &snap);
        }
    }
    Ok(())
}

/// Set the user-visible display name for the panel identified by `label`.
///
/// `display_name: null` clears the override and reverts to the app-provided
/// title. Persists immediately (no debounce) and emits `wm:host-layout` +
/// `wm:layout` so the tab strip and any panel headers update in real time.
#[tauri::command]
pub async fn wm_rename_panel(
    label: String,
    display_name: Option<String>,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if !terminal.layout_tree.set_display_name(&label, display_name) {
        return Err(format!("no panel with label '{label}'"));
    }
    terminal.layout_tree.emit_host(&app);
    if let Some(snap) = terminal.layout_tree.snapshot() {
        let chrome = format!("{}-chrome", window.label());
        if let Some(wv) = app.get_webview(&chrome) {
            let _ = wv.emit("wm:layout", &snap);
        }
    }
    Ok(())
}

/// Apply a zoom multiplier to the webview for `label`.
///
/// `zoom_factor` is clamped to `[0.5, 2.0]` before being stored or applied.
/// Persists immediately (no debounce) and emits `wm:host-layout` + `wm:layout`.
#[tauri::command]
pub async fn wm_set_zoom(
    label: String,
    zoom_factor: f64,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    let Some(clamped) = terminal.layout_tree.set_zoom_factor(&label, zoom_factor) else {
        return Err(format!("no panel with label '{label}'"));
    };
    if let Some(wv) = app.get_webview(&label) {
        wv.set_zoom(clamped).map_err(|e| e.to_string())?;
    }
    terminal.layout_tree.emit_host(&app);
    if let Some(snap) = terminal.layout_tree.snapshot() {
        let chrome = format!("{}-chrome", window.label());
        if let Some(wv) = app.get_webview(&chrome) {
            let _ = wv.emit("wm:layout", &snap);
        }
    }
    Ok(())
}

/// Return the FDC3 channel currently joined by the panel identified by
/// `label`, or `None` if it's on no channel (or doesn't exist).
#[tauri::command]
pub fn wm_get_panel_fdc3_channel(
    label: String,
    window: Window,
    manager: State<'_, TerminalManager>,
) -> Option<String> {
    manager
        .get(window.label())
        .and_then(|t| t.layout_tree.fdc3_channel(&label))
}

/// Join (or leave) the FDC3 user channel for the panel identified by `label`.
///
/// Unlike the Terminal-wide channel pill this replaces, this actually joins
/// the panel's own `fdc3-plugin.js` connection to the broker: the channel
/// choice is persisted on the panel's `LeafMeta`, then pushed into the
/// panel's live webview via `wv.eval` (there is no native Tauri API for this,
/// unlike `wm_set_zoom`'s `wv.set_zoom` — only the panel's own JS knows how
/// to speak the FDC3 join protocol). `channel_id` must be one of the 8 fixed
/// system-channel ids; it is validated before being interpolated into the
/// evaluated script.
#[tauri::command]
pub fn wm_set_panel_fdc3_channel(
    label: String,
    channel_id: Option<String>,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    if let Some(id) = &channel_id {
        if !SYSTEM_CHANNEL_IDS.contains(&id.as_str()) {
            return Err(format!("unknown FDC3 channel id '{id}'"));
        }
    }

    let terminal = get_terminal!(manager, window);
    if !terminal
        .layout_tree
        .set_fdc3_channel(&label, channel_id.clone())
    {
        return Err(format!("no panel with label '{label}'"));
    }

    let script = match &channel_id {
        Some(id) => {
            let encoded = serde_json::to_string(id).map_err(|e| e.to_string())?;
            format!(
                "window.fdc3Ready && window.fdc3Ready.then(function(c) {{ c.joinUserChannel({encoded}); }});"
            )
        }
        None => {
            "window.fdc3Ready && window.fdc3Ready.then(function(c) { c.leaveCurrentChannel(); });"
                .to_string()
        }
    };
    if let Some(wv) = app.get_webview(&label) {
        wv.eval(&script).map_err(|e| e.to_string())?;
    }

    terminal.layout_tree.emit_host(&app);
    if let Some(snap) = terminal.layout_tree.snapshot() {
        let chrome = format!("{}-chrome", window.label());
        if let Some(wv) = app.get_webview(&chrome) {
            let _ = wv.emit("wm:layout", &snap);
        }
    }
    Ok(())
}

#[tauri::command]
pub fn wm_end_tab_drag(
    source_label: String,
    target_path: Option<Vec<usize>>,
    zone: Option<DropZone>,
    insert_index: Option<usize>,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if let (Some(path), Some(z)) = (target_path, zone) {
        terminal
            .layout_tree
            .move_leaf(&source_label, &path, z, insert_index);
    }
    terminal.layout_tree.reflow(&app);
    terminal.layout_tree.emit_host(&app);
    Ok(())
}

// ── Dashboard commands ────────────────────────────────────────────────────────

/// Return the current dashboard list state. Useful for an initial sync after
/// the chrome mounts; thereafter the frontend should listen to `wm:dashboards`.
#[tauri::command]
pub fn wm_list_dashboards(
    window: Window,
    manager: State<'_, TerminalManager>,
) -> DashboardsSnapshot {
    match manager.get(window.label()) {
        Some(terminal) => terminal.layout_tree.dashboards_snapshot(),
        None => {
            eprintln!(
                "[wm_list_dashboards] terminal '{}' not found",
                window.label()
            );
            DashboardsSnapshot {
                active: String::new(),
                auto_save: true,
                dirty: false,
                dashboards: vec![],
            }
        }
    }
}

/// Create a new empty dashboard with the given name. The new dashboard is
/// appended to the end of the list and is not automatically made active.
/// Returns `false` if a dashboard with that name already exists.
#[tauri::command]
pub fn wm_create_dashboard(
    name: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<bool, String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("dashboard name must not be empty".into());
    }
    let terminal = get_terminal!(manager, window);
    let created = terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.create(trimmed));
    if created {
        terminal.layout_tree.persist_dashboards();
        terminal.layout_tree.emit_dashboards(&app);
    }
    Ok(created)
}

/// Snapshot the live layout into the active dashboard and write to disk.
/// Clears the dirty flag. Call this when `auto_save` is off and the user
/// explicitly requests a save.
#[tauri::command]
pub fn wm_save_dashboard(window: Window, manager: State<'_, TerminalManager>, app: AppHandle) {
    let Some(terminal) = manager.get(window.label()) else {
        eprintln!(
            "[wm_save_dashboard] terminal '{}' not found",
            window.label()
        );
        return;
    };
    terminal.layout_tree.save_dashboard();
    terminal.layout_tree.emit_dashboards(&app);
}

/// Rename a dashboard. Returns `false` if `old_name` doesn't exist or
/// `new_name` is already taken.
#[tauri::command]
pub fn wm_rename_dashboard(
    old_name: String,
    new_name: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<bool, String> {
    let new_trimmed = new_name.trim().to_string();
    if new_trimmed.is_empty() {
        return Err("dashboard name must not be empty".into());
    }
    let terminal = get_terminal!(manager, window);
    let renamed = terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.rename(&old_name, new_trimmed));
    if renamed {
        terminal.layout_tree.persist_dashboards();
        terminal.layout_tree.emit_dashboards(&app);
    }
    Ok(renamed)
}

/// Delete a dashboard. Deleting the active dashboard switches to the next one,
/// or leaves the terminal empty if it was the last. Returns `false` if `name`
/// doesn't exist.
#[tauri::command]
pub async fn wm_delete_dashboard(
    name: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    cfg: State<'_, crate::config::TerminalConfig>,
    app: AppHandle,
) -> Result<bool, String> {
    let terminal = get_terminal!(manager, window);
    let deleted = terminal
        .layout_tree
        .delete_dashboard(&name, &window, &app, &cfg.panel_init_script())
        .map_err(|e| format!("{e:?}"))?;
    if deleted {
        terminal.layout_tree.persist_dashboards();
        terminal.layout_tree.emit_dashboards(&app);
    }
    Ok(deleted)
}

/// Reorder the dashboard list to match `order`. Names not present in the
/// current store are ignored; names present but missing from `order` are
/// dropped.
#[tauri::command]
pub fn wm_reorder_dashboards(
    order: Vec<String>,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) {
    let Some(terminal) = manager.get(window.label()) else {
        eprintln!(
            "[wm_reorder_dashboards] terminal '{}' not found",
            window.label()
        );
        return;
    };
    terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.reorder(&order));
    terminal.layout_tree.persist_dashboards();
    terminal.layout_tree.emit_dashboards(&app);
}

/// Enable or disable auto-save. When switching from off→on, the live layout
/// is immediately snapshotted and persisted.
#[tauri::command]
pub fn wm_set_auto_save(
    enabled: bool,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) {
    let Some(terminal) = manager.get(window.label()) else {
        eprintln!("[wm_set_auto_save] terminal '{}' not found", window.label());
        return;
    };
    terminal.layout_tree.set_auto_save(enabled);
    terminal.layout_tree.emit_dashboards(&app);
}
