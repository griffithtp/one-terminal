//! Layout-tree IPC commands. The frontend drives the layout by shipping
//! whole trees into `update_layout`; Rust installs them as the source of
//! truth and stitches every webview via `reflow_layout`.

use tauri::{AppHandle, Emitter, Manager, State};

use super::docking::DropZone;
use super::node::LayoutNode;
use super::store::LayoutTree;

/// Replace the layout tree wholesale and reflow every webview.
///
/// The argument is a FlexLayout-shaped tree: each node is `{ "type": "leaf"
/// | "splitter" | "stack", "weight": f64, ... }`. Leaves that reference a
/// webview label not currently alive are silently skipped by the reflow pass.
#[tauri::command]
pub fn update_layout(
    json_tree: LayoutNode,
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    store.set_root(Some(json_tree));
    store.reflow(&app);
    store.emit_host(&app);
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
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    if store.resize_splitter(&path, child_index, position_x, position_y) {
        store.reflow(&app);
        store.emit_host(&app);
    }
    Ok(())
}

/// Tab drag begin — park every panel offscreen so the chrome (which sits
/// beneath panels in z-order) is fully visible while the user drags. The
/// webviews keep their process/session intact; `wm_end_tab_drag` restores
/// them via `reflow`.
#[tauri::command]
pub fn wm_begin_tab_drag(store: State<'_, LayoutTree>, app: AppHandle) -> Result<(), String> {
    store.park_all(&app);
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
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    if store.set_active_tab(&path, tab_index) {
        store.reflow(&app);
        store.emit_host(&app);
    }
    Ok(())
}

/// Close the Leaf with `label`: drop it from the tree, destroy its webview,
/// then reflow. If removing the leaf empties a Stack, `simplify` collapses
/// it and any 1-child Splitters bubble up.
#[tauri::command]
pub fn wm_close_leaf(
    label: String,
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    if store.remove_leaf(&label) {
        if let Some(wv) = app.get_webview(&label) {
            let _ = wv.close();
        }
        store.reflow(&app);
        store.emit_host(&app);
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
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    if !store.remove_leaf(&label) {
        return Err(format!("no leaf with label '{label}' in layout tree"));
    }
    if let Some(wv) = app.get_webview(&label) {
        wv.close().map_err(|e| e.to_string())?;
    }
    store.reflow(&app);
    store.emit_host(&app);
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
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    let labels = store.labels_in_stack(&path);
    if labels.is_empty() {
        return Ok(());
    }
    for label in &labels {
        store.remove_panel(label);
        if let Some(wv) = app.get_webview(label) {
            let _ = wv.close();
        }
    }
    store.reflow(&app);
    store.emit_host(&app);
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
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    if store.toggle_maximize_stack(&path) {
        store.reflow(&app);
        store.emit_host(&app);
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
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("title must not be empty".into());
    }
    if !store.rename_panel(&label, trimmed) {
        return Err(format!("no panel with label '{label}'"));
    }
    store.emit_host(&app);
    if let Some(snap) = store.snapshot() {
        let _ = app.emit("wm:layout", &snap);
    }
    Ok(())
}

#[tauri::command]
pub fn wm_end_tab_drag(
    source_label: String,
    target_path: Option<Vec<usize>>,
    zone: Option<DropZone>,
    insert_index: Option<usize>,
    store: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    if let (Some(path), Some(z)) = (target_path, zone) {
        store.move_leaf(&source_label, &path, z, insert_index);
    }
    store.reflow(&app);
    store.emit_host(&app);
    Ok(())
}
