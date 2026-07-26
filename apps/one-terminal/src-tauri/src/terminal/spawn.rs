//! Terminal window spawning and startup restore.
//!
//! `spawn_terminal` creates a complete Terminal: OS window, chrome/overlay/pool
//! child webviews (all with namespaced labels), a fresh `TerminalState`, and
//! registration in `TerminalManager`.
//!
//! `load_persisted_terminals` scans `<data_dir>/terminals/` at startup and
//! calls `spawn_terminal` for each persisted non-main terminal directory.

use std::sync::{Arc, Mutex, RwLock};

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl, Window,
};

use crate::layout::persist::{self, PersistedWindowConfig, TerminalPersist};
use crate::layout::store::LayoutTree;
use crate::webview_pool::{pool_label, WebviewPool};

use super::manager::TerminalManager;
use super::state::{OverlayInner, OverlayState, TerminalInfo, TerminalState};

// ── Window geometry defaults ──────────────────────────────────────────────────

pub const DEFAULT_W: f64 = 1600.0;
pub const DEFAULT_H: f64 = 900.0;
pub const MIN_W: f64 = 640.0;
pub const MIN_H: f64 = 400.0;

// ── Monitor helpers ───────────────────────────────────────────────────────────

/// Return `true` if the given logical rect overlaps at least one available
/// monitor by more than a threshold (we use 50×50 px), so the user still has a
/// grab-handle to drag the window to a reachable position.
pub fn is_rect_on_any_monitor(app: &AppHandle, x: f64, y: f64, w: f64, h: f64) -> bool {
    let monitors = match app.available_monitors() {
        Ok(m) => m,
        Err(_) => return false,
    };
    for monitor in monitors {
        let mp = monitor.position();
        let ms = monitor.size();
        let sf = monitor.scale_factor();
        let mx = mp.x as f64 / sf;
        let my = mp.y as f64 / sf;
        let mw = ms.width as f64 / sf;
        let mh = ms.height as f64 / sf;
        let overlap_x = (x + w).min(mx + mw) - x.max(mx);
        let overlap_y = (y + h).min(my + mh) - y.max(my);
        if overlap_x >= 50.0 && overlap_y >= 50.0 {
            return true;
        }
    }
    false
}

// ── Window event listener ─────────────────────────────────────────────────────

/// Install resize and move listeners on `win` that:
/// 1. Reflow the layout tree and resize the chrome webview on `Resized`.
/// 2. Debounce-save the window position to disk on `Resized` and `Moved`.
pub fn install_window_listeners(
    terminal_id: &str,
    win: &Window,
    tree: &LayoutTree,
    app: &AppHandle,
    window_config: Arc<RwLock<PersistedWindowConfig>>,
) {
    let app_h = app.clone();
    let tree_c = tree.clone();
    let chrome_label = format!("{terminal_id}-chrome");
    let terminal_id_s = terminal_id.to_string();
    let pos_handle: Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> =
        Arc::new(Mutex::new(None));

    win.on_window_event(move |evt| {
        match evt {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                // Intercept OS-level close so the frontend can show a
                // confirmation dialog before wm_close_terminal is called.
                api.prevent_close();
                let chrome = format!("{terminal_id_s}-chrome");
                if let Some(wv) = app_h.get_webview(&chrome) {
                    let _ = wv.emit("wm:confirm-close", serde_json::json!(null));
                }
            }
            tauri::WindowEvent::Resized(phys) => {
                let sf = app_h
                    .get_window(&terminal_id_s)
                    .and_then(|w| w.scale_factor().ok())
                    .unwrap_or(1.0);
                let lw = phys.width as f64 / sf;
                let lh = phys.height as f64 / sf;

                tree_c.set_size(lw, lh);

                if let Some(chrome) = app_h.get_webview(&chrome_label) {
                    let _ = chrome.set_bounds(tauri::Rect {
                        position: tauri::Position::Logical(LogicalPosition::new(0.0, 0.0)),
                        size: tauri::Size::Logical(LogicalSize::new(lw, lh)),
                    });
                }
                tree_c.reflow(&app_h);
                tree_c.emit_host(&app_h);
                if let Some(snap) = tree_c.snapshot() {
                    if let Some(wv) = app_h.get_webview(&chrome_label) {
                        let _ = wv.emit("wm:layout", &snap);
                    }
                }

                {
                    let mut cfg = window_config.write().unwrap();
                    cfg.width = lw;
                    cfg.height = lh;
                }
                schedule_position_save(&pos_handle, &window_config, &terminal_id_s, &app_h);
            }
            tauri::WindowEvent::Moved(pos) => {
                let sf = app_h
                    .get_window(&terminal_id_s)
                    .and_then(|w| w.scale_factor().ok())
                    .unwrap_or(1.0);
                let lx = pos.x as f64 / sf;
                let ly = pos.y as f64 / sf;
                {
                    let mut cfg = window_config.write().unwrap();
                    cfg.x = lx;
                    cfg.y = ly;
                }
                schedule_position_save(&pos_handle, &window_config, &terminal_id_s, &app_h);
            }
            _ => {}
        }
    });
}

fn schedule_position_save(
    handle: &Arc<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>>,
    window_config: &Arc<RwLock<PersistedWindowConfig>>,
    terminal_id: &str,
    app: &AppHandle,
) {
    let cfg = window_config.read().unwrap().clone();
    let tid = terminal_id.to_string();
    let app = app.clone();
    let mut h = handle.lock().unwrap();
    if let Some(prev) = h.take() {
        prev.abort();
    }
    *h = Some(tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if let Ok(data_dir) = app.path().app_data_dir() {
            if let Err(e) = persist::update_window_config(&tid, &cfg, &data_dir) {
                eprintln!("[wm] save window position for {tid}: {e}");
            }
        }
    }));
}

// ── spawn_terminal / restore_terminal ────────────────────────────────────────

/// Create a new Terminal OS window and register it in `manager`.
///
/// Starts with an empty dashboard list. `persist` supplies only the initial
/// window geometry and FDC3 channel; any existing `terminals/<label>/` persist
/// directory is deleted so the new terminal is guaranteed to start fresh.
pub fn spawn_terminal(
    label: &str,
    name: Option<String>,
    manager: &TerminalManager,
    app: &AppHandle,
    pool_size: usize,
    persist: Option<TerminalPersist>,
) -> Result<TerminalInfo, String> {
    spawn_or_restore(label, name, manager, app, pool_size, persist, false)
}

/// Re-open a Terminal that was previously saved to disk.
///
/// Loads `terminals/<label>/dashboards.json` for the window geometry, FDC3
/// channel, and — via `LayoutTree::init` — all dashboards and their panels.
pub fn restore_terminal(
    label: &str,
    manager: &TerminalManager,
    app: &AppHandle,
    pool_size: usize,
) -> Result<TerminalInfo, String> {
    let persist = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| persist::load_terminal_for(label, &d));
    spawn_or_restore(label, None, manager, app, pool_size, persist, true)
}

/// Scan `<data_dir>/terminals/` for saved non-main terminal directories and
/// restore each one. Called once from `one-terminal`'s setup so that all
/// terminals open in the previous session reappear automatically.
pub fn load_persisted_terminals(manager: &TerminalManager, app: &AppHandle, pool_size: usize) {
    let Ok(data_dir) = app.path().app_data_dir() else {
        return;
    };
    for id in persist::list_saved_terminal_ids(&data_dir) {
        match restore_terminal(&id, manager, app, pool_size) {
            Ok(info) => eprintln!("[wm] restored terminal: {} ({})", info.id, info.name),
            Err(e) => eprintln!("[wm] failed to restore terminal {id}: {e}"),
        }
    }
}

// ── Shared implementation ─────────────────────────────────────────────────────

fn spawn_or_restore(
    label: &str,
    name: Option<String>,
    manager: &TerminalManager,
    app: &AppHandle,
    pool_size: usize,
    persist: Option<TerminalPersist>,
    restore: bool,
) -> Result<TerminalInfo, String> {
    // Determine display name: caller arg > saved name > label.
    let display_name = name.unwrap_or_else(|| {
        persist
            .as_ref()
            .filter(|p| !p.name.is_empty())
            .map(|p| p.name.clone())
            .unwrap_or_else(|| label.to_string())
    });

    let saved_window = persist
        .as_ref()
        .map(|p| p.window.clone())
        .unwrap_or_default();

    // ── OS window ─────────────────────────────────────────────────────────────
    let win = tauri::WindowBuilder::new(app, label)
        .title(format!("OneTerminal — {display_name}"))
        .inner_size(DEFAULT_W, DEFAULT_H)
        .min_inner_size(MIN_W, MIN_H)
        .resizable(true)
        .decorations(false)
        .build()
        .map_err(|e| e.to_string())?;

    // ── Apply saved window position (if valid) ─────────────────────────────
    if saved_window.width > 0.0
        && saved_window.height > 0.0
        && is_rect_on_any_monitor(
            app,
            saved_window.x,
            saved_window.y,
            saved_window.width,
            saved_window.height,
        )
    {
        let _ = win.set_size(tauri::Size::Logical(LogicalSize::new(
            saved_window.width,
            saved_window.height,
        )));
        let _ = win.set_position(tauri::Position::Logical(LogicalPosition::new(
            saved_window.x,
            saved_window.y,
        )));
    }

    let (init_w, init_h) = {
        let sz = win.inner_size().unwrap_or(tauri::PhysicalSize {
            width: DEFAULT_W as u32,
            height: DEFAULT_H as u32,
        });
        let sf = win.scale_factor().unwrap_or(1.0);
        (sz.width as f64 / sf, sz.height as f64 / sf)
    };

    // ── Layout tree ───────────────────────────────────────────────────────────
    let tree = if restore {
        // Load dashboards and panels from disk for this terminal ID.
        let t = LayoutTree::new(label, init_w, init_h);
        if let Err(e) = t.init(app) {
            eprintln!("[restore_terminal] init failed for {label}: {e}");
        }
        t
    } else {
        // New terminal — delete any stale persisted directory so it can never
        // be loaded on a future restore pass, then start with a clean slate.
        if let Ok(data_dir) = app.path().app_data_dir() {
            if let Err(e) = crate::layout::persist::delete_terminal_for(label, &data_dir) {
                eprintln!("[spawn_terminal] stale persist cleanup for {label}: {e}");
            }
        }
        let t = LayoutTree::new(label, init_w, init_h);
        t.register_app_handle(app);
        t
    };

    // ── Overlay state ─────────────────────────────────────────────────────────
    let overlay: OverlayState = Arc::new(Mutex::new(OverlayInner::default()));

    // ── Webview pool ──────────────────────────────────────────────────────────
    let pool = WebviewPool::new(pool_size);

    // ── Chrome webview — lowest z-order ───────────────────────────────────────
    win.add_child(
        WebviewBuilder::new(
            format!("{label}-chrome"),
            WebviewUrl::App("index.html".into()),
        ),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(init_w, init_h),
    )
    .map_err(|e| e.to_string())?;

    // ── Overlay webview — above chrome, below future panel webviews ───────────
    win.add_child(
        WebviewBuilder::new(
            format!("{label}-overlay"),
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
            )
            .on_navigation(crate::address_bar_navigation_handler(
                app.clone(),
                label.to_string(),
                lbl.clone(),
                tree.clone(),
            )),
            LogicalPosition::new(-20000.0, -20000.0),
            LogicalSize::new(init_w, init_h),
        ) {
            Ok(_) => pool.push(lbl),
            Err(e) => eprintln!("[spawn_terminal] pool pre-warm failed for {lbl}: {e}"),
        }
    }

    // ── Restore persisted panel webviews ─────────────────────────────────────
    let panels_to_restore = tree.panels_for_restore();
    if !panels_to_restore.is_empty() {
        let base_init_script = app
            .state::<crate::config::TerminalConfig>()
            .panel_init_script();
        for (panel_label, url, channel) in &panels_to_restore {
            if let Ok(parsed_url) = url.parse::<tauri::Url>() {
                let script =
                    crate::config::append_initial_channel(&base_init_script, channel.as_deref());
                if let Err(e) = win.add_child(
                    WebviewBuilder::new(panel_label, WebviewUrl::External(parsed_url))
                        .initialization_script(&script)
                        .on_navigation(crate::address_bar_navigation_handler(
                            app.clone(),
                            label.to_string(),
                            panel_label.clone(),
                            tree.clone(),
                        )),
                    LogicalPosition::new(0.0, 0.0),
                    LogicalSize::new(1.0, 1.0),
                ) {
                    eprintln!("[spawn_terminal] restore panel '{panel_label}': {e}");
                }
            }
        }
        {
            let mut inner = overlay.lock().unwrap();
            inner.stale = true;
            inner.is_ready = false;
        }
        tree.reflow(app);
        tree.emit_host(app);
        if let Some(snap) = tree.snapshot() {
            let chrome = format!("{label}-chrome");
            if let Some(wv) = app.get_webview(&chrome) {
                let _ = wv.emit("wm:layout", &snap);
            }
        }
    }

    // ── Window config shared state ────────────────────────────────────────────
    let window_config = Arc::new(RwLock::new(saved_window));

    // ── Resize / move listener ────────────────────────────────────────────────
    install_window_listeners(label, &win, &tree, app, Arc::clone(&window_config));

    // ── Register ──────────────────────────────────────────────────────────────
    let state = Arc::new(TerminalState {
        id: label.to_string(),
        name: RwLock::new(display_name.clone()),
        layout_tree: tree,
        overlay,
        pool,
        window_config,
    });
    manager.register(state);

    Ok(TerminalInfo {
        id: label.to_string(),
        name: display_name,
    })
}
