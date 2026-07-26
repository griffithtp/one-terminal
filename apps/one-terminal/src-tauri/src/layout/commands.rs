//! Layout-tree IPC commands. The frontend drives the layout by shipping
//! whole trees into `update_layout`; Rust installs them as the source of
//! truth and stitches every webview via `reflow_layout`.

use tauri::{AppHandle, Emitter, Manager, State, Window};

use crate::terminal::TerminalManager;

use super::dashboard::{DashboardError, DashboardsSnapshot};
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

/// Return whether the panel identified by `label` is flagged to keep
/// running in the background across Dashboard switches.
#[tauri::command]
pub fn wm_get_panel_keep_alive(
    label: String,
    window: Window,
    manager: State<'_, TerminalManager>,
) -> bool {
    manager
        .get(window.label())
        .map(|t| t.layout_tree.keep_alive(&label))
        .unwrap_or(false)
}

/// Set (or clear) the "keep running in background" flag for the panel
/// identified by `label`. Unlike `wm_set_panel_fdc3_channel`, this is pure
/// metadata — no live webview action is needed here; the flag only changes
/// what happens the *next* time the panel's Dashboard is switched away from.
#[tauri::command]
pub fn wm_set_panel_keep_alive(
    label: String,
    keep_alive: bool,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if !terminal.layout_tree.set_keep_alive(&label, keep_alive) {
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

/// Set (or clear) the "show address bar" flag for the panel identified by
/// `label` (Generic Web Widget panels only — harmless no-op visually for any
/// other app). Unlike `wm_set_panel_keep_alive`, this changes how much
/// content-area height the panel's own webview reserves, so a `reflow` is
/// required in addition to the metadata update and snapshot re-emit.
#[tauri::command]
pub fn wm_set_panel_show_address_bar(
    label: String,
    show_address_bar: bool,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    if !terminal
        .layout_tree
        .set_show_address_bar(&label, show_address_bar)
    {
        return Err(format!("no panel with label '{label}'"));
    }

    terminal.layout_tree.reflow(&app);
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
                closed_dashboards: vec![],
                parked_count: 0,
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
        .with_dashboard_store_mut(|ds| ds.rename(&old_name, new_trimmed.clone()));
    if renamed {
        // Keep the `parked` registry's ownership in sync — otherwise panels
        // kept alive under the old name become unreachable by
        // `wm_delete_dashboard` (leaked webview) once the rename lands.
        terminal
            .layout_tree
            .rename_parked_owner(&old_name, &new_trimmed);
        terminal.layout_tree.persist_dashboards();
        terminal.layout_tree.emit_dashboards(&app);
    }
    Ok(renamed)
}

/// Permanently delete a dashboard — its layout is gone for good. Only
/// **closed** dashboards can be deleted (see `wm_close_dashboard`); an open
/// dashboard — including the active one, which is always open — must be
/// closed first. This is enforced here, not just hidden in the UI, since a
/// closed dashboard is guaranteed to be neither dirty nor have anything
/// parked in the background, which is what makes it safe to skip all the
/// webview/active-dashboard reconciliation `close_dashboard` already did.
///
/// This is the drawer's "Delete" action (irreversible, only reachable from
/// the Closed section). The dashboard tab's "Close dashboard" menu item
/// uses `wm_close_dashboard` instead, which keeps the dashboard's data
/// around for `wm_reopen_dashboard`.
///
/// Returns `Err(DashboardError::Other)` if `name` is open. Returns `false`
/// if `name` doesn't exist. Unless `force` is `true`, returns
/// `DashboardError::NeedsConfirm` unconditionally — deletion is
/// irreversible, so (unlike `wm_close_dashboard`) this always asks, not just
/// when there's live state at risk — the frontend should show a confirm
/// dialog and retry with `force: true`.
#[tauri::command]
pub async fn wm_delete_dashboard(
    name: String,
    force: bool,
    window: Window,
    manager: State<'_, TerminalManager>,
    cfg: State<'_, crate::config::TerminalConfig>,
    app: AppHandle,
) -> Result<bool, DashboardError> {
    let terminal = manager
        .get(window.label())
        .ok_or_else(|| DashboardError::Other {
            message: format!("terminal '{}' not found", window.label()),
        })?;

    let is_closed = terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.dashboards.get(&name).map(|d| d.closed));
    match is_closed {
        None => return Ok(false),
        Some(false) => {
            return Err(DashboardError::Other {
                message: format!("'{name}' is open — close it before deleting"),
            });
        }
        Some(true) => {}
    }

    if !force {
        return Err(DashboardError::NeedsConfirm);
    }

    let deleted =
        terminal
            .layout_tree
            .delete_dashboard(&name, &window, &app, &cfg.panel_init_script())?;
    if deleted {
        terminal.layout_tree.persist_dashboards();
        terminal.layout_tree.emit_dashboards(&app);
    }
    Ok(deleted)
}

/// Close a dashboard — hide it from the switcher and Manage drawer's main
/// list, and stop any of its widgets currently running in the background,
/// but keep its layout intact so `wm_reopen_dashboard` can bring it back
/// exactly as it was. Closing the active dashboard switches to the next
/// open one, or leaves the terminal empty if none remain open. Returns
/// `false` if `name` doesn't exist.
///
/// Unless `force` is `true`, returns `DashboardError::NeedsConfirm` under
/// the same conditions as `wm_delete_dashboard` (unsaved changes on the
/// active dashboard, or keep-alive panels currently parked) — closing
/// still discards *live, unsaved* state even though the last-saved layout
/// survives.
#[tauri::command]
pub async fn wm_close_dashboard(
    name: String,
    force: bool,
    window: Window,
    manager: State<'_, TerminalManager>,
    cfg: State<'_, crate::config::TerminalConfig>,
    app: AppHandle,
) -> Result<bool, DashboardError> {
    let terminal = manager
        .get(window.label())
        .ok_or_else(|| DashboardError::Other {
            message: format!("terminal '{}' not found", window.label()),
        })?;

    if !force && terminal.layout_tree.dashboard_needs_confirm_close(&name) {
        return Err(DashboardError::NeedsConfirm);
    }

    let closed =
        terminal
            .layout_tree
            .close_dashboard(&name, &window, &app, &cfg.panel_init_script())?;
    if closed {
        terminal.layout_tree.persist_dashboards();
        terminal.layout_tree.emit_dashboards(&app);
    }
    Ok(closed)
}

/// Reopen a dashboard previously hidden via `wm_close_dashboard` — it
/// reappears in the switcher and Manage drawer's main list, unchanged from
/// when it was closed. Does not switch to it. Returns `false` if `name`
/// doesn't exist.
#[tauri::command]
pub fn wm_reopen_dashboard(
    name: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<bool, String> {
    let terminal = get_terminal!(manager, window);
    let reopened = terminal.layout_tree.reopen_dashboard(&name);
    if reopened {
        terminal.layout_tree.emit_dashboards(&app);
    }
    Ok(reopened)
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

/// Set (or clear) the default FDC3 channel for `dashboard_name` and apply it
/// to every widget currently in that dashboard, whether or not it's the
/// active one. When active, this also joins every open panel's live webview
/// to the channel (mirroring `wm_set_panel_fdc3_channel`'s per-panel eval)
/// and re-emits the layout snapshot so tab channel dots update immediately.
/// Future widgets added to this dashboard inherit the channel automatically
/// (see `LayoutTree::insert_panel`'s `default_channel` lookup).
#[tauri::command]
pub fn wm_set_dashboard_default_channel(
    dashboard_name: String,
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
    let is_active = terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.active == dashboard_name);
    let found = terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.set_default_channel(&dashboard_name, channel_id.clone()));
    if !found {
        return Err(format!("dashboard '{dashboard_name}' not found"));
    }
    terminal.layout_tree.persist_dashboards();

    if is_active {
        let labels = terminal
            .layout_tree
            .set_fdc3_channel_for_all(channel_id.clone());
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
        for label in &labels {
            if let Some(wv) = app.get_webview(label) {
                wv.eval(&script).map_err(|e| e.to_string())?;
            }
        }
        terminal.layout_tree.emit_host(&app);
        if let Some(snap) = terminal.layout_tree.snapshot() {
            let chrome = format!("{}-chrome", window.label());
            if let Some(wv) = app.get_webview(&chrome) {
                let _ = wv.emit("wm:layout", &snap);
            }
        }
    }

    Ok(())
}

/// Bulk-set the "keep running in background" flag for every widget currently
/// in `dashboard_name`, whether or not it's the active dashboard. Unlike
/// `wm_set_dashboard_default_channel`, this is a one-shot bulk action, not a
/// sticky default — widgets added to this dashboard later still default to
/// `keep_alive: false` regardless of this call.
#[tauri::command]
pub fn wm_set_dashboard_keep_alive_all(
    dashboard_name: String,
    keep_alive: bool,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    let is_active = terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.active == dashboard_name);
    let found = terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.set_all_keep_alive(&dashboard_name, keep_alive));
    if !found {
        return Err(format!("dashboard '{dashboard_name}' not found"));
    }
    terminal.layout_tree.persist_dashboards();

    if is_active {
        terminal.layout_tree.set_keep_alive_for_all(keep_alive);
        terminal.layout_tree.emit_host(&app);
        if let Some(snap) = terminal.layout_tree.snapshot() {
            let chrome = format!("{}-chrome", window.label());
            if let Some(wv) = app.get_webview(&chrome) {
                let _ = wv.emit("wm:layout", &snap);
            }
        }
    }

    Ok(())
}

/// Duplicate a dashboard within the same Terminal window. `name` may or may
/// not be the active dashboard — duplicating a background dashboard reads
/// its persisted snapshot directly and never touches the live layout tree.
/// The new name is derived from `name` by appending " (copy)", " (copy 2)",
/// etc. until an unused name is found. Returns the final chosen name.
#[tauri::command]
pub fn wm_duplicate_dashboard(
    name: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<String, String> {
    let terminal = get_terminal!(manager, window);

    let snapshot = terminal
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.dashboards.get(&name).cloned());
    let Some(mut dashboard) = snapshot else {
        return Err(format!("dashboard '{name}' not found"));
    };
    // The duplicate is always open, regardless of whether the source is
    // closed — duplicating a closed dashboard should produce a usable copy,
    // not another hidden one.
    dashboard.closed = false;

    let new_name = terminal.layout_tree.with_dashboard_store_mut(|ds| {
        let mut candidate = format!("{name} (copy)");
        let mut n = 2;
        while ds.dashboards.contains_key(&candidate) {
            candidate = format!("{name} (copy {n})");
            n += 1;
        }
        ds.create_from(candidate.clone(), dashboard);
        candidate
    });

    terminal.layout_tree.persist_dashboards();
    terminal.layout_tree.emit_dashboards(&app);
    Ok(new_name)
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
