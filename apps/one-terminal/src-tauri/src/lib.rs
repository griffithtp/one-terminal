mod config;
mod engine;
mod engines;
mod layout;
mod webview_pool;

use config::TerminalConfig;
use engine::WmHostIdentity;
use layout::commands::{
    close_tab, update_layout, wm_begin_tab_drag, wm_close_leaf, wm_close_stack, wm_end_tab_drag,
    wm_rename_panel, wm_rename_tab, wm_set_active_tab, wm_set_zoom, wm_splitter_drag,
    wm_toggle_maximize_stack,
};
use layout::drag::wm_drag_move;
use layout::store::{LayoutTree, PanelSpec};
use layout::{LayoutSnapshot, SplitDir};
use ot_core::engine::{is_system_version, EngineBinding, EngineFamily};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State, WebviewBuilder, WebviewUrl};
use tauri::{Emitter, LogicalPosition, LogicalSize};
use tokio::sync::oneshot;
use webview_pool::WebviewPool;

const CHROME: &str = "wm-chrome";
const OVERLAY: &str = "wm-overlay";
const WIN: &str = "wm";

// ── Overlay webview state ─────────────────────────────────────────────────────
//
// The overlay webview (`wm-overlay`) renders floating UI — context menus, etc.
// — that must appear above panel content webviews.  Because Tauri's child-
// webview z-order equals insertion order, the overlay must be the *last*
// webview added after each `wm_open`.  Rather than recreating it eagerly on
// every panel open (which would reload the bundle), we mark it stale and
// recreate on-demand inside `wm_ctx_menu_open`, then wait for the overlay to
// signal readiness before emitting the menu payload.

struct OverlayInner {
    is_ready: bool,
    /// Set to `true` when a new content panel is added after the overlay was
    /// last created — the overlay is no longer the topmost child webview.
    stale: bool,
    /// Multiple `wm_ctx_menu_open` calls can be in flight concurrently (rapid
    /// right-clicks).  All of them wait on the same ready signal, so we keep
    /// every pending sender — `wm_overlay_ready` drains and notifies all.
    wakers: Vec<oneshot::Sender<()>>,
}

type OverlayState = Arc<Mutex<OverlayInner>>;

// ── Out-of-process launch (engine that this WM can't host) ────────────────────

/// Spawn an external host pinned to `binding`. Used when the user picks a
/// browser engine that doesn't match this WM's pinned engine — the launch
/// pops out as a stand-alone window instead of a tab.
///
/// Routing by family:
/// - **Electron**: spawn the Electron binary located inside the extracted
///   runtime folder, passing the URL on the command line.
/// - **WebView2 / WKWebView**: spawn `tauri-webview-host`, optionally pinning
///   `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` for fixed-runtime WebView2.
///
/// The frontend is expected to have already verified the engine is installed
/// via `wm_engine_status` / `wm_engine_install`. We re-check the install
/// sentinel so a stale frontend can't silently fall back to the system engine.
fn spawn_external_host(
    app: &AppHandle,
    identity: &WmHostIdentity,
    binding: &EngineBinding,
    app_id: &str,
    url: &str,
    title: &str,
) -> Result<(), String> {
    use ot_core::engine::{binding_path, is_installed};

    match &binding.family {
        EngineFamily::Electron => {
            // Pre-flight: if the binary is absent and OT_ELECTRON_HOST_OVERRIDE
            // is set, spawn_electron_app will auto-install (blocking ~30 s on
            // first run). Emit a UI event so the chrome can show a status banner
            // instead of appearing frozen.
            let needs_install = !ot_core::electron_host::is_ready(binding, &identity.cache_root);
            if needs_install {
                let _ = app.emit("wm:electron-installing", serde_json::json!(null));
            }
            let result = ot_core::electron_host::spawn_electron_app(
                binding,
                &identity.cache_root,
                app_id,
                url,
                title,
            );
            if needs_install {
                match &result {
                    Ok(_) => {
                        let _ = app.emit("wm:electron-ready", serde_json::json!(null));
                    }
                    Err(e) => {
                        let _ = app.emit(
                            "wm:electron-install-failed",
                            serde_json::json!({ "error": e }),
                        );
                    }
                }
            }
            result?;
        }
        EngineFamily::Webview2 | EngineFamily::Wkwebview => {
            let runtime_dir = if is_system_version(&binding.version) {
                None
            } else {
                let path = binding_path(&identity.cache_root, binding);
                if !is_installed(&path) {
                    let _ = app.emit(
                        "wm:engine-missing",
                        serde_json::json!({
                            "family": binding.family.as_dir(),
                            "version": binding.version,
                            "expectedPath": path.display().to_string(),
                            "hint": "Install the engine via the picker's download prompt before retrying.",
                        }),
                    );
                    return Err(format!(
                        "Engine {}@{} is not installed at {}",
                        binding.family.as_dir(),
                        binding.version,
                        path.display(),
                    ));
                }
                Some(path)
            };

            let host_bin = locate_host_binary()?;
            let mut cmd = std::process::Command::new(&host_bin);
            cmd.env("OT_APP_ID", app_id)
                .env("OT_URL", url)
                .env("OT_TITLE", title);
            if let Some(path) = runtime_dir {
                cmd.env("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", &path);
            }
            cmd.spawn()
                .map(|_| ())
                .map_err(|e| format!("spawn tauri-webview-host {}: {e}", host_bin.display()))?;
        }
        EngineFamily::Custom(name) => {
            // Plugin engines are launched by the desktop-agent's plugin router.
            // The Terminal doesn't know how to launch them directly — open a
            // tauri-webview-host without a pinned runtime so the user gets
            // something rather than a hard error.
            eprintln!("[wm] custom engine '{name}' — plugin launch not supported in Terminal; falling back to system webview");
            let host_bin = locate_host_binary()?;
            std::process::Command::new(&host_bin)
                .env("OT_APP_ID", app_id)
                .env("OT_URL", url)
                .env("OT_TITLE", title)
                .spawn()
                .map(|_| ())
                .map_err(|e| format!("spawn tauri-webview-host {}: {e}", host_bin.display()))?;
        }
    }

    let _ = app.emit(
        "wm:external-launched",
        serde_json::json!({
            "family": binding.family.as_dir(),
            "version": binding.version,
            "url": url,
            "title": title,
        }),
    );
    Ok(())
}

/// Locate the `tauri-webview-host` binary. Honors `OT_HOST_BINARY` for dev
/// overrides; otherwise looks next to the currently-running one-terminal binary (the
/// release layout puts the sidecar in the same directory).
fn locate_host_binary() -> Result<std::path::PathBuf, String> {
    if let Ok(path) = std::env::var("OT_HOST_BINARY") {
        return Ok(std::path::PathBuf::from(path));
    }
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "exe has no parent dir".to_string())?;
    let name = if cfg!(windows) {
        "tauri-webview-host.exe"
    } else {
        "tauri-webview-host"
    };
    let candidate = dir.join(name);
    if !candidate.exists() {
        return Err(format!(
            "tauri-webview-host not found at {} — build it (`cargo build -p tauri-webview-host`) or set OT_HOST_BINARY",
            candidate.display()
        ));
    }
    Ok(candidate)
}

// ── IPC Commands ──────────────────────────────────────────────────────────────

/// Return the current layout snapshot (None if no panels are open).
#[tauri::command]
fn wm_snapshot(tree: State<'_, LayoutTree>) -> Option<LayoutSnapshot> {
    tree.snapshot()
}

/// Open a new panel.
///
/// - `target`         — panel id to insert relative to. Defaults to the
///                      currently active panel, or (if none) the first leaf.
/// - `dir`            — `"horizontal"` / `"vertical"` to split along that axis,
///                      or `null` / omitted to insert as a tab in the target's
///                      Stack (auto-wrapping the target into a new Stack if
///                      needed).
/// - `engine_binding` — the engine the user picked for this tab. When it
///                      matches this WM's pinned engine, the panel is added
///                      as a tab; otherwise the launch is sent to a
///                      stand-alone `tauri-webview-host` and the WM layout
///                      is left unchanged.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn wm_open(
    app_id: String,
    url: String,
    title: String,
    target: Option<String>,
    dir: Option<SplitDir>,
    engine_binding: Option<EngineBinding>,
    tree: State<'_, LayoutTree>,
    identity: State<'_, WmHostIdentity>,
    overlay: State<'_, OverlayState>,
    pool: State<'_, WebviewPool>,
    app: AppHandle,
) -> Result<LayoutSnapshot, String> {
    // Engine the user picked doesn't match this WM's engine — pop out into a
    // stand-alone host window. WM tabs always share the WM's own engine.
    if !identity.matches(engine_binding.as_ref()) {
        let binding = engine_binding
            .as_ref()
            .expect("matches() returned false so binding must be Some");
        spawn_external_host(&app, &identity, binding, &app_id, &url, &title)?;
        return Ok(tree.snapshot().unwrap_or_else(LayoutSnapshot::empty));
    }

    // Try the pool first; fall back to cold creation when empty.
    // Guard against the unlikely case where the pool webview was closed externally.
    let pool_label = pool.take().filter(|lbl| app.get_webview(lbl).is_some());

    let spec = PanelSpec {
        app_id: app_id.clone(),
        url: url.clone(),
        title: title.clone(),
        engine_binding: engine_binding.clone(),
    };

    let panel_id = match &pool_label {
        Some(lbl) => tree.add_panel_with_label(lbl, spec, target.as_deref(), dir),
        None => tree.add_panel(spec, target.as_deref(), dir),
    };

    let parsed_url: tauri::Url = url.parse().map_err(|e: url::ParseError| e.to_string())?;

    // Dispatch the webview operation to the UI/main thread.
    //
    // On Windows, child-webview creation must happen on the UI thread — and
    // Tauri's internal `add_child` implementation marshals to the main thread
    // and waits for the reply. If the command handler itself is already
    // running on the main thread (or holding it), that inner wait deadlocks
    // and `add_child` never returns. The command is `async` (so it runs on
    // the async runtime) and we explicitly dispatch via `run_on_main_thread`.
    //
    // Pool path: navigate the pre-created blank webview to the real URL.
    // Cold path: create a new child webview as before.
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let app_for_main = app.clone();
    let panel_id_for_main = panel_id.clone();
    let url_for_main = url.clone();
    let using_pool = pool_label.is_some();

    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            if using_pool {
                let wv = app_for_main
                    .get_webview(&panel_id_for_main)
                    .ok_or_else(|| format!("pool webview '{}' not found", panel_id_for_main))?;
                wv.navigate(parsed_url).map_err(|e| e.to_string())?;
            } else {
                let win = app_for_main
                    .get_window(WIN)
                    .ok_or_else(|| "wm window not found".to_string())?;
                // Placeholder bounds — `tree.reflow` below positions the webview
                // correctly once it's created.
                win.add_child(
                    WebviewBuilder::new(&panel_id_for_main, WebviewUrl::External(parsed_url)),
                    LogicalPosition::new(0.0, 0.0),
                    LogicalSize::new(1.0, 1.0),
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        match &result {
            Ok(_) => eprintln!(
                "[wm_open] {} (panel={panel_id_for_main}, url={url_for_main})",
                if using_pool {
                    "pool->navigate Ok"
                } else {
                    "add_child Ok"
                }
            ),
            Err(e) => eprintln!(
                "[wm_open] {} Err: {e}",
                if using_pool {
                    "pool->navigate"
                } else {
                    "add_child"
                }
            ),
        }
        let _ = tx.send(result);
    })
    .map_err(|e| {
        eprintln!("[wm_open] run_on_main_thread dispatch -> Err: {e}");
        e.to_string()
    })?;

    rx.recv().map_err(|e| e.to_string())??;

    // Mark the overlay as stale: the panel webview is above it in z-order
    // (pool webviews are inserted after the overlay at startup; cold webviews
    // are always appended last). wm_ctx_menu_open recreates the overlay as
    // the topmost child before showing any menu.
    {
        let mut inner = overlay.lock().unwrap();
        inner.stale = true;
        inner.is_ready = false;
    }

    // Replenish the pool in the background after a slot was consumed.
    // No-op on the cold path (pool not used) or when already at capacity.
    pool.replenish(&app, &overlay);

    // Reflow positions every webview (including the new/navigated one).
    tree.reflow(&app);
    tree.emit_host(&app);

    let snap = tree.snapshot().ok_or("layout empty after add")?;
    app.emit("wm:layout", &snap).ok();
    Ok(snap)
}

/// Close an open panel and remove it from the layout tree.
#[tauri::command]
fn wm_close(
    panel_id: String,
    tree: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<Option<LayoutSnapshot>, String> {
    if !tree.remove_panel(&panel_id) {
        return Err(format!("panel '{panel_id}' not found"));
    }
    if let Some(wv) = app.get_webview(&panel_id) {
        wv.close().map_err(|e| e.to_string())?;
    }
    tree.reflow(&app);
    tree.emit_host(&app);

    let snap = tree.snapshot();
    match &snap {
        Some(s) => {
            app.emit("wm:layout", s).ok();
        }
        None => {
            app.emit("wm:layout", serde_json::Value::Null).ok();
        }
    }
    Ok(snap)
}

// ── Engine availability + install ─────────────────────────────────────────────

/// Tell the frontend whether the engine the user picked is ready to launch,
/// needs to be downloaded first, or isn't supported. Drives the picker's
/// "Download X MB?" confirmation dialog.
#[tauri::command]
async fn wm_engine_status(
    binding: EngineBinding,
    cfg: State<'_, TerminalConfig>,
) -> Result<engines::EngineStatus, ()> {
    Ok(engines::engine_status(&binding, &cfg.engine_catalog_url).await)
}

/// Download + verify + extract the runtime for `binding`. Idempotent: a
/// second call returns immediately once the install sentinel is in place.
/// Progress is emitted as `engine:download:start|progress|complete|error`.
#[tauri::command]
async fn wm_engine_install(
    binding: EngineBinding,
    app: AppHandle,
    cfg: State<'_, TerminalConfig>,
) -> Result<(), String> {
    engines::engine_install(&binding, &app, &cfg.engine_catalog_url)
        .await
        .map(|_| ())
}

// ── Overlay webview commands ──────────────────────────────────────────────────

/// Called by the overlay webview's React component on mount to signal that
/// event listeners are registered and the menu payload can be sent.
#[tauri::command]
fn wm_overlay_ready(overlay: State<'_, OverlayState>) {
    let mut inner = overlay.lock().unwrap();
    inner.is_ready = true;
    inner.stale = false;
    for tx in inner.wakers.drain(..) {
        let _ = tx.send(());
    }
}

/// Show the context menu overlay at window position (`x`, `y`) for the Stack
/// at `stack_path` containing `n_tabs` tabs.  If the overlay is stale (a panel
/// was opened since it was last created), it is closed and recreated on the
/// main thread so it regains the topmost z-order, then this function awaits
/// the overlay's ready signal before emitting `wm:ctx-menu`.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn wm_ctx_menu_open(
    x: f64,
    y: f64,
    stack_path: Vec<usize>,
    n_tabs: usize,
    tab_label: Option<String>,
    app_id: Option<String>,
    display_name: Option<String>,
    zoom_factor: Option<f64>,
    overlay: State<'_, OverlayState>,
    app: AppHandle,
) -> Result<(), String> {
    let overlay_arc = Arc::clone(&*overlay);

    // Atomically take ownership of the recreate.  If we see `stale=true`,
    // clear it inside the same lock so a second concurrent right-click doesn't
    // also try to close+recreate the overlay — it'll fall through and just
    // wait on the ready signal we're about to produce.
    let must_recreate = {
        let mut inner = overlay_arc.lock().unwrap();
        if inner.stale {
            inner.stale = false;
            inner.is_ready = false;
            true
        } else {
            false
        }
    };
    if must_recreate {
        let (create_tx, create_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let app_for_main = app.clone();
        app.run_on_main_thread(move || {
            let result = (|| -> Result<(), String> {
                if let Some(old) = app_for_main.get_webview(OVERLAY) {
                    old.close().map_err(|e| e.to_string())?;
                }
                let win = app_for_main.get_window(WIN).ok_or("wm window not found")?;
                let sf = win.scale_factor().unwrap_or(1.0);
                let sz = win.inner_size().unwrap_or(tauri::PhysicalSize {
                    width: 1600,
                    height: 900,
                });
                let (w, h) = (sz.width as f64 / sf, sz.height as f64 / sf);
                win.add_child(
                    WebviewBuilder::new(OVERLAY, WebviewUrl::App("index.html#overlay".into()))
                        .transparent(true),
                    LogicalPosition::new(-20000.0, -20000.0),
                    LogicalSize::new(w, h),
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })();
            let _ = create_tx.send(result);
        })
        .map_err(|e| e.to_string())?;

        create_rx.recv().map_err(|e| e.to_string())??;
    }

    // Wait for the overlay to signal it is ready (event listeners registered).
    let rx = {
        let mut inner = overlay_arc.lock().unwrap();
        if inner.is_ready {
            None
        } else {
            let (tx, rx) = oneshot::channel();
            inner.wakers.push(tx);
            Some(rx)
        }
    };
    if let Some(rx) = rx {
        tokio::time::timeout(std::time::Duration::from_secs(5), rx)
            .await
            .map_err(|_| "timeout waiting for overlay".to_string())?
            .map_err(|_| "overlay waker dropped".to_string())?;
    }

    // Move the overlay to cover the full window so its backdrop captures
    // outside-clicks and the menu renders at the correct cursor position.
    let win = app.get_window(WIN).ok_or("wm window not found")?;
    let sf = win.scale_factor().unwrap_or(1.0);
    let sz = win.inner_size().unwrap_or(tauri::PhysicalSize {
        width: 1600,
        height: 900,
    });
    let (w, h) = (sz.width as f64 / sf, sz.height as f64 / sf);
    if let Some(wv) = app.get_webview(OVERLAY) {
        wv.set_bounds(tauri::Rect {
            position: tauri::Position::Logical(LogicalPosition::new(0.0, 0.0)),
            size: tauri::Size::Logical(LogicalSize::new(w, h)),
        })
        .map_err(|e| e.to_string())?;
    } else {
        return Err("overlay webview not found after ready".to_string());
    }

    app.emit(
        "wm:ctx-menu",
        serde_json::json!({
            "x": x,
            "y": y,
            "stackPath": stack_path,
            "nTabs": n_tabs,
            "tabLabel": tab_label,
            "appId": app_id,
            "displayName": display_name,
            "zoomFactor": zoom_factor,
        }),
    )
    .map_err(|e| e.to_string())?;

    Ok(())
}

/// Signal the chrome webview to enter inline rename mode for `label`.
/// Emitted as `wm:request-rename` so the tab strip can focus the input
/// without a backend round-trip for state.
#[tauri::command]
fn wm_request_rename(label: String, app: AppHandle) {
    let _ = app.emit("wm:request-rename", serde_json::json!({ "label": label }));
}

/// Hide the overlay by parking it offscreen.  Called by the overlay itself
/// when the user dismisses the menu or selects an action.
#[tauri::command]
fn wm_ctx_menu_close(app: AppHandle) {
    if let Some(wv) = app.get_webview(OVERLAY) {
        let _ = wv.set_position(tauri::Position::Logical(LogicalPosition::new(
            -20000.0, -20000.0,
        )));
    }
}

// ── Picker overlay parking ────────────────────────────────────────────────────
//
// Panel webviews sit above the chrome webview in z-order, so the chrome can't
// render dialogs that overlay them. The frontend calls `wm_park_panels` to
// hide all panels offscreen while the engine picker / download confirm
// dialog is visible, and `wm_unpark_panels` to restore the layout afterward.
// The webview processes stay alive — only their bounds move.

#[tauri::command]
fn wm_park_panels(tree: State<'_, LayoutTree>, app: AppHandle) -> Result<(), String> {
    tree.park_all(&app);
    Ok(())
}

#[tauri::command]
fn wm_unpark_panels(tree: State<'_, LayoutTree>, app: AppHandle) -> Result<(), String> {
    tree.reflow(&app);
    tree.emit_host(&app);
    Ok(())
}

// ── Config command ────────────────────────────────────────────────────────────

/// Return the active TerminalConfig to the frontend. Called once on startup so
/// the chrome can use the correct app-directory URL, title, and layout metrics
/// without duplicating constants between Rust and TypeScript.
#[tauri::command]
fn wm_config(cfg: State<'_, TerminalConfig>) -> TerminalConfig {
    cfg.inner().clone()
}

// ── App entry point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = TerminalConfig::load();
    let tree = LayoutTree::new(cfg.window.width, cfg.window.height);
    let identity = WmHostIdentity::from_env();
    let overlay_state: OverlayState = Arc::new(Mutex::new(OverlayInner {
        is_ready: false,
        stale: false,
        wakers: Vec::new(),
    }));
    let pool_size = std::env::var("OT_WEBVIEW_POOL_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let pool = WebviewPool::new(pool_size);
    println!(
        "[wm] engine: {}@{} (runtime={:?})",
        identity.binding.family.as_dir(),
        identity.binding.version,
        identity.runtime_path,
    );
    println!("[wm] webview pool size: {pool_size}");

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut("CmdOrCtrl+K")
                .expect("CmdOrCtrl+K is a valid shortcut")
                .with_handler(|app, _shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let _ = app.emit("global-shortcut", "CmdOrCtrl+K");
                    }
                })
                .build(),
        )
        .manage(tree.clone())
        .manage(identity.clone())
        .manage(overlay_state.clone())
        .manage(cfg.clone())
        .manage(pool.clone())
        .setup(move |app| {
            // ── Load persisted layout state ───────────────────────────────
            // Hydrates the LayoutTree from layout.json if it exists. Must run
            // before any webview is created so the restored tree is in place.
            tree.init(app.handle())?;

            // ── Create the bare container window (no default webview) ──────
            let win = tauri::WindowBuilder::new(app.handle(), WIN)
                .title(&cfg.title)
                .inner_size(cfg.window.width, cfg.window.height)
                .min_inner_size(cfg.window.min_width, cfg.window.min_height)
                .resizable(true)
                .decorations(false)
                .build()?;

            // Sync layout manager with the actual initial logical size.
            if let (Ok(sz), Ok(sf)) = (win.inner_size(), win.scale_factor()) {
                let lw = sz.width as f64 / sf;
                let lh = sz.height as f64 / sf;
                tree.set_size(lw, lh);
            }

            // ── Chrome webview — full window, transparent background ───────
            // Added FIRST → lowest z-order.  Panel webviews added later sit
            // on top, leaving the header and splitter gaps exposed.
            let (init_w, init_h) = {
                let sz = win.inner_size().unwrap_or(tauri::PhysicalSize {
                    width: cfg.window.width as u32,
                    height: cfg.window.height as u32,
                });
                let sf = win.scale_factor().unwrap_or(1.0);
                (sz.width as f64 / sf, sz.height as f64 / sf)
            };

            win.add_child(
                WebviewBuilder::new(CHROME, WebviewUrl::App("index.html".into())),
                LogicalPosition::new(0.0, 0.0),
                LogicalSize::new(init_w, init_h),
            )?;

            // Overlay webview — added second so it's initially above chrome
            // but below any content panels (which are added later via wm_open).
            // wm_ctx_menu_open recreates it as the last child whenever it has
            // become stale, ensuring it is always topmost when shown.
            // `.transparent(true)` is required so the WebView2/WKWebView
            // control itself is transparent; CSS `background: transparent`
            // alone only removes the HTML layer.
            win.add_child(
                WebviewBuilder::new(OVERLAY, WebviewUrl::App("index.html#overlay".into()))
                    .transparent(true),
                LogicalPosition::new(-20000.0, -20000.0),
                LogicalSize::new(init_w, init_h),
            )?;

            // ── Webview pool — pre-create blank hidden webviews ───────────
            // Created after the overlay so they land above it in z-order,
            // matching the position of panels added via the cold path. The
            // overlay stale flag is set when each pool webview is first
            // activated in wm_open (same flow as cold panel creation).
            if pool.target_size > 0 {
                for _ in 0..pool.target_size {
                    let label = webview_pool::pool_label();
                    match win.add_child(
                        WebviewBuilder::new(
                            &label,
                            WebviewUrl::External(
                                "about:blank".parse().expect("about:blank is a valid URL"),
                            ),
                        ),
                        LogicalPosition::new(-20000.0, -20000.0),
                        LogicalSize::new(init_w, init_h),
                    ) {
                        Ok(_) => {
                            eprintln!("[pool] pre-warmed: {label}");
                            pool.push(label);
                        }
                        Err(e) => eprintln!("[pool] pre-warm failed: {e}"),
                    }
                }
            }

            // ── Restore persisted panel webviews ──────────────────────────
            // Re-create webview children for every leaf in the hydrated tree.
            // The overlay must be marked stale if any panels are added so
            // wm_ctx_menu_open knows to push it to the back of the z-order.
            let panels_to_restore = tree.panels_for_restore();
            if !panels_to_restore.is_empty() {
                for (label, url) in &panels_to_restore {
                    if let Ok(parsed_url) = url.parse::<tauri::Url>() {
                        if let Err(e) = win.add_child(
                            WebviewBuilder::new(label, WebviewUrl::External(parsed_url)),
                            LogicalPosition::new(0.0, 0.0),
                            LogicalSize::new(1.0, 1.0),
                        ) {
                            eprintln!("[wm] restore panel '{label}': {e}");
                        }
                    } else {
                        eprintln!("[wm] restore panel '{label}': invalid url '{url}'");
                    }
                }
                {
                    let mut inner = overlay_state.lock().unwrap();
                    inner.stale = true;
                    inner.is_ready = false;
                }
                tree.reflow(app.handle());
                tree.emit_host(app.handle());
                if let Some(snap) = tree.snapshot() {
                    let _ = app.handle().emit("wm:layout", &snap);
                }
            }

            // ── Resize listener — reposition all webviews on window resize ─
            let app_h = app.handle().clone();
            let tree_resize = tree.clone();
            win.on_window_event(move |evt| {
                if let tauri::WindowEvent::Resized(phys) = evt {
                    let sf = app_h
                        .get_window(WIN)
                        .and_then(|w| w.scale_factor().ok())
                        .unwrap_or(1.0);
                    let lw = phys.width as f64 / sf;
                    let lh = phys.height as f64 / sf;

                    tree_resize.set_size(lw, lh);

                    // Resize the chrome to fill the whole window.
                    if let Some(chrome) = app_h.get_webview(CHROME) {
                        let _ = chrome.set_bounds(tauri::Rect {
                            position: tauri::Position::Logical(LogicalPosition::new(0.0, 0.0)),
                            size: tauri::Size::Logical(LogicalSize::new(lw, lh)),
                        });
                    }

                    // Reflow all panels and republish overlays.
                    tree_resize.reflow(&app_h);
                    tree_resize.emit_host(&app_h);
                    if let Some(snap) = tree_resize.snapshot() {
                        let _ = app_h.emit("wm:layout", &snap);
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            wm_config,
            wm_snapshot,
            wm_open,
            wm_close,
            wm_drag_move,
            update_layout,
            wm_splitter_drag,
            wm_begin_tab_drag,
            wm_end_tab_drag,
            wm_set_active_tab,
            wm_close_leaf,
            wm_close_stack,
            wm_rename_tab,
            wm_toggle_maximize_stack,
            close_tab,
            wm_engine_status,
            wm_engine_install,
            wm_rename_panel,
            wm_set_zoom,
            wm_park_panels,
            wm_unpark_panels,
            wm_overlay_ready,
            wm_ctx_menu_open,
            wm_ctx_menu_close,
            wm_request_rename,
        ])
        .run(tauri::generate_context!())
        .expect("error while running one-terminal");
}
