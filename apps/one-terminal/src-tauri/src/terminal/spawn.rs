//! Terminal window spawning and startup restore.
//!
//! `spawn_terminal` creates a complete Terminal: OS window, chrome/overlay/pool
//! child webviews (all with namespaced labels), a fresh `TerminalState`, and
//! registration in `TerminalManager`.
//!
//! `load_persisted_terminals` scans `<data_dir>/terminals/` at startup and
//! calls `spawn_terminal` for each persisted non-main terminal directory.

use std::sync::{Arc, Mutex, RwLock};

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl};

use crate::layout::persist::TerminalPersist;
use crate::layout::store::LayoutTree;
use crate::webview_pool::{pool_label, WebviewPool};

use super::manager::TerminalManager;
use super::state::{OverlayInner, OverlayState, TerminalInfo, TerminalState};

// ── Window geometry defaults ──────────────────────────────────────────────────

const DEFAULT_W: f64 = 1600.0;
const DEFAULT_H: f64 = 900.0;
const MIN_W: f64 = 640.0;
const MIN_H: f64 = 400.0;

// ── spawn_terminal ────────────────────────────────────────────────────────────

/// Create a new Terminal OS window and register it in `manager`.
///
/// Steps:
/// 1. Build the OS window with `WindowBuilder`.
/// 2. Add chrome, overlay, and pool child webviews with namespaced labels.
/// 3. Construct `TerminalState` (empty layout — persistence restore is wired in
///    PR 7 once `LayoutTree::init_from_persist` exists).
/// 4. Register the state in `manager`.
///
/// `label` must be globally unique across all open windows. Use
/// `manager.next_label()` for dynamically-spawned Terminals; pass
/// `"terminal-main"` explicitly for the primary window.
///
/// `_persist` is accepted but not yet applied — it is plumbed here so the
/// call sites in `load_persisted_terminals` are correct for PR 7.
pub fn spawn_terminal(
    label: &str,
    name: Option<String>,
    manager: &TerminalManager,
    app: &AppHandle,
    pool_size: usize,
    _persist: Option<TerminalPersist>,
) -> Result<TerminalInfo, String> {
    let display_name = name.unwrap_or_else(|| label.to_string());

    // ── OS window ─────────────────────────────────────────────────────────────
    let win = tauri::WindowBuilder::new(app, label)
        .title(&format!("OneTerminal — {display_name}"))
        .inner_size(DEFAULT_W, DEFAULT_H)
        .min_inner_size(MIN_W, MIN_H)
        .resizable(true)
        .decorations(false)
        .build()
        .map_err(|e| e.to_string())?;

    let (init_w, init_h) = {
        let sz = win.inner_size().unwrap_or(tauri::PhysicalSize {
            width: DEFAULT_W as u32,
            height: DEFAULT_H as u32,
        });
        let sf = win.scale_factor().unwrap_or(1.0);
        (sz.width as f64 / sf, sz.height as f64 / sf)
    };

    // ── Layout tree ───────────────────────────────────────────────────────────
    let tree = LayoutTree::new(label, init_w, init_h);
    // Register the AppHandle so schedule_save works without loading the
    // hardcoded "main" path that LayoutTree::init uses.
    tree.register_app_handle(app);

    // ── Overlay state ─────────────────────────────────────────────────────────
    let overlay: OverlayState = Arc::new(Mutex::new(OverlayInner::default()));

    // ── Webview pool ──────────────────────────────────────────────────────────
    let pool = WebviewPool::new(pool_size);

    // ── Chrome webview — lowest z-order ───────────────────────────────────────
    win.add_child(
        WebviewBuilder::new(
            &format!("{label}-chrome"),
            WebviewUrl::App("index.html".into()),
        ),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(init_w, init_h),
    )
    .map_err(|e| e.to_string())?;

    // ── Overlay webview — above chrome, below future panel webviews ───────────
    win.add_child(
        WebviewBuilder::new(
            &format!("{label}-overlay"),
            WebviewUrl::App("index.html#overlay".into()),
        )
        .transparent(true),
        LogicalPosition::new(-20000.0, -20000.0),
        LogicalSize::new(init_w, init_h),
    )
    .map_err(|e| e.to_string())?;

    // ── Pool webviews — pre-warmed blank webviews ─────────────────────────────
    for _ in 0..pool_size {
        let lbl = pool_label(label);
        match win.add_child(
            WebviewBuilder::new(
                &lbl,
                WebviewUrl::External("about:blank".parse().expect("about:blank is valid")),
            ),
            LogicalPosition::new(-20000.0, -20000.0),
            LogicalSize::new(init_w, init_h),
        ) {
            Ok(_) => pool.push(lbl),
            Err(e) => eprintln!("[spawn_terminal] pool pre-warm failed for {lbl}: {e}"),
        }
    }

    // ── Register ──────────────────────────────────────────────────────────────
    let state = Arc::new(TerminalState {
        id: label.to_string(),
        name: display_name.clone(),
        layout_tree: tree,
        overlay,
        pool,
        fdc3_channel: Arc::new(RwLock::new(None)),
    });
    manager.register(state);

    Ok(TerminalInfo {
        id: label.to_string(),
        name: display_name,
    })
}

// ── load_persisted_terminals ──────────────────────────────────────────────────

/// Scan `<data_dir>/terminals/` and call `spawn_terminal` for every persisted
/// Terminal whose state is not yet registered in `manager`.
///
/// The `"main"` subdirectory is skipped — the primary Terminal window is
/// created by the legacy `lib.rs` setup path until the migration in PR 6.
///
/// Errors from individual terminals are logged and skipped rather than
/// aborting the whole startup.
pub fn load_persisted_terminals(
    app: &AppHandle,
    manager: &TerminalManager,
    pool_size: usize,
) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let terminals_dir = data_dir.join("terminals");

    let entries = match std::fs::read_dir(&terminals_dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // no terminals directory yet — fresh install
    };

    for entry in entries.flatten() {
        let dir_name = entry.file_name();
        let Some(name_str) = dir_name.to_str() else {
            continue;
        };

        // "main" is handled by the existing lib.rs setup path.
        if name_str == "main" {
            continue;
        }

        let label = format!("terminal-{name_str}");

        if manager.get(&label).is_some() {
            continue; // already registered (called twice during startup?)
        }

        let persist: Option<TerminalPersist> =
            std::fs::read(entry.path().join("dashboards.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());

        if let Err(e) = spawn_terminal(&label, None, manager, app, pool_size, persist) {
            eprintln!("[load_persisted_terminals] failed to spawn {label}: {e}");
        }
    }

    Ok(())
}
