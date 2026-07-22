mod config;
mod engine;
mod engines;
mod layout;
mod sample_widget;
mod terminal;
mod webview_pool;
mod widgets;

use config::TerminalConfig;
use engine::WmHostIdentity;
use layout::commands::{
    close_tab, update_layout, wm_begin_tab_drag, wm_close_leaf, wm_close_stack,
    wm_create_dashboard, wm_delete_dashboard, wm_end_tab_drag, wm_get_panel_fdc3_channel,
    wm_list_dashboards, wm_rename_dashboard, wm_rename_panel, wm_rename_tab, wm_reorder_dashboards,
    wm_save_dashboard, wm_set_active_tab, wm_set_auto_save, wm_set_panel_fdc3_channel, wm_set_zoom,
    wm_splitter_drag, wm_toggle_maximize_stack,
};
use layout::dashboard::DashboardError;
use layout::drag::wm_drag_move;
use layout::host::HostLayout;
use layout::persist::{self as layout_persist, PersistedWindowConfig};
use layout::store::{LayoutTree, PanelSpec};
use layout::{LayoutSnapshot, SplitDir};
use ot_core::engine::{is_system_version, EngineBinding, EngineFamily};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State, WebviewBuilder, WebviewUrl, Window};
use tauri::{Emitter, LogicalPosition, LogicalSize};
use terminal::spawn::{install_window_listeners, is_rect_on_any_monitor};
use terminal::state::{OverlayInner, OverlayState, TerminalState};
use terminal::TerminalManager;
use tokio::sync::oneshot;
use webview_pool::WebviewPool;

/// Resolve a terminal from the invoking window; return a descriptive error on
/// miss.  Mirrors the macro in `layout/commands.rs` for use in lib.rs commands.
macro_rules! get_terminal {
    ($manager:expr, $window:expr) => {
        match $manager.get($window.label()) {
            Some(t) => t,
            None => return Err(format!("terminal '{}' not found", $window.label())),
        }
    };
}

const CHROME: &str = "terminal-main-chrome";
const OVERLAY: &str = "terminal-main-overlay";
const WIN: &str = "terminal-main";

// ── Overlay webview state ─────────────────────────────────────────────────────
//
// The overlay webview renders floating UI — context menus, command palette,
// overflow dropdowns — that must appear above panel content webviews.  Because
// Tauri's child-webview z-order equals insertion order, the overlay must be
// the *last* webview added after each `wm_open`.  Rather than recreating it
// eagerly on every panel open (which would reload the bundle), we mark it
// stale and recreate on-demand inside `wm_ctx_menu_open`, then wait for the
// overlay to signal readiness before emitting the menu payload.
//
// `OverlayInner` and `OverlayState` are defined in `terminal::state` and
// re-exported here for use by the command layer.

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
fn wm_snapshot(window: Window, manager: State<'_, TerminalManager>) -> Option<LayoutSnapshot> {
    manager
        .get(window.label())
        .and_then(|t| t.layout_tree.snapshot())
}

/// Return the current host-shell projection (tab strips + splitter handles).
/// The chrome calls this on mount to hydrate `useHostLayout` in case the
/// initial `wm:host-layout` event fired before the frontend listener was ready.
#[tauri::command]
fn wm_host_snapshot(window: Window, manager: State<'_, TerminalManager>) -> HostLayout {
    match manager.get(window.label()) {
        Some(t) => t.layout_tree.host_snapshot(),
        None => HostLayout {
            window_width: 0.0,
            window_height: 0.0,
            stacks: vec![],
            splitters: vec![],
        },
    }
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
    window: Window,
    manager: State<'_, TerminalManager>,
    identity: State<'_, WmHostIdentity>,
    cfg: State<'_, TerminalConfig>,
    app: AppHandle,
) -> Result<LayoutSnapshot, String> {
    let panel_init_script = cfg.panel_init_script();
    let terminal = get_terminal!(manager, window);
    let tree = &terminal.layout_tree;
    let overlay = &terminal.overlay;
    let pool = &terminal.pool;

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
    let terminal_id_for_main = window.label().to_string();
    let panel_init_script = panel_init_script.clone();

    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            if using_pool {
                let wv = app_for_main
                    .get_webview(&panel_id_for_main)
                    .ok_or_else(|| format!("pool webview '{}' not found", panel_id_for_main))?;
                wv.navigate(parsed_url).map_err(|e| e.to_string())?;
            } else {
                let win = app_for_main
                    .get_window(&terminal_id_for_main)
                    .ok_or_else(|| format!("window '{}' not found", terminal_id_for_main))?;
                // Placeholder bounds — `tree.reflow` below positions the webview
                // correctly once it's created.
                win.add_child(
                    WebviewBuilder::new(&panel_id_for_main, WebviewUrl::External(parsed_url))
                        .initialization_script(&panel_init_script),
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

    // Prewarm the overlay in the background so the next kebab / palette /
    // engine-picker click hits an already-ready overlay instead of paying
    // the ~200–500 ms recreate latency (close old → add_child new → React
    // mount → register listeners → wm_overlay_ready). Without this the
    // first click after any widget launch appears unresponsive.
    overlay_prewarm_in_background(Arc::clone(overlay), window.label(), &app);

    // Replenish the pool in the background after a slot was consumed.
    // No-op on the cold path (pool not used) or when already at capacity.
    pool.replenish(&app, overlay, window.label(), &cfg.panel_init_script());

    // Reflow positions every webview (including the new/navigated one).
    tree.reflow(&app);
    tree.emit_host(&app);

    let snap = tree.snapshot().ok_or("layout empty after add")?;
    let chrome = format!("{}-chrome", window.label());
    if let Some(wv) = app.get_webview(&chrome) {
        wv.emit("wm:layout", &snap).ok();
    }
    Ok(snap)
}

/// Close an open panel and remove it from the layout tree.
#[tauri::command]
fn wm_close(
    panel_id: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<Option<LayoutSnapshot>, String> {
    let terminal = get_terminal!(manager, window);
    if !terminal.layout_tree.remove_panel(&panel_id) {
        return Err(format!("panel '{panel_id}' not found"));
    }
    if let Some(wv) = app.get_webview(&panel_id) {
        wv.close().map_err(|e| e.to_string())?;
    }
    terminal.layout_tree.reflow(&app);
    terminal.layout_tree.emit_host(&app);

    let snap = terminal.layout_tree.snapshot();
    let chrome = format!("{}-chrome", window.label());
    if let Some(wv) = app.get_webview(&chrome) {
        match &snap {
            Some(s) => {
                wv.emit("wm:layout", s).ok();
            }
            None => {
                wv.emit("wm:layout", serde_json::Value::Null).ok();
            }
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
///
/// NOTE: this does NOT reset `stale`. The stale flag tracks whether a new
/// content panel was added *after* the overlay was last inserted as a child
/// webview. An overlay that calls `wm_overlay_ready` may not be the topmost
/// child webview — e.g. when the initial overlay (position 2) loads after
/// panels have already been restored on top of it. The stale flag is cleared
/// only when `wm_ctx_menu_open` / `wm_palette_open` / `wm_overflow_menu_open`
/// explicitly decide to recreate the overlay.
#[tauri::command]
fn wm_overlay_ready(window: Window, manager: State<'_, TerminalManager>) {
    let Some(terminal) = manager.get(window.label()) else {
        eprintln!("[wm_overlay_ready] terminal '{}' not found", window.label());
        return;
    };
    let mut inner = terminal.overlay.lock().unwrap();
    inner.is_ready = true;
    for tx in inner.wakers.drain(..) {
        let _ = tx.send(());
    }
}

/// Ensure the overlay webview is the topmost child and ready to receive
/// events, but leave it parked offscreen. Idempotent and safe to call
/// concurrently — a second caller sees `stale=false` and just waits for
/// the ready signal already in flight.
///
/// Called both from `overlay_raise` (synchronously, before showing a menu)
/// and from background prewarm tasks spawned by `wm_open` / pool replenish
/// so the next user click hits an already-ready overlay instead of paying
/// a 200–500 ms recreate latency.
pub(crate) async fn overlay_prewarm(
    overlay_arc: Arc<Mutex<OverlayInner>>,
    terminal_id: &str,
    app: &AppHandle,
) -> Result<(), String> {
    let overlay_label = format!("{terminal_id}-overlay");
    let win_label = terminal_id.to_string();

    // Atomically take ownership of the recreate. If we see `stale=true`,
    // clear it inside the same lock so a second concurrent call doesn't also
    // try to close+recreate the overlay — it falls through and waits on the
    // ready signal we're about to produce.
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
        let overlay_label_main = overlay_label.clone();
        let win_label_main = win_label.clone();
        app.run_on_main_thread(move || {
            let result = (|| -> Result<(), String> {
                if let Some(old) = app_for_main.get_webview(&overlay_label_main) {
                    old.close().map_err(|e| e.to_string())?;
                }
                let win = app_for_main
                    .get_window(&win_label_main)
                    .ok_or_else(|| format!("window '{}' not found", win_label_main))?;
                let sf = win.scale_factor().unwrap_or(1.0);
                let sz = win.inner_size().unwrap_or(tauri::PhysicalSize {
                    width: 1600,
                    height: 900,
                });
                let (w, h) = (sz.width as f64 / sf, sz.height as f64 / sf);
                win.add_child(
                    WebviewBuilder::new(
                        &overlay_label_main,
                        WebviewUrl::App("index.html#overlay".into()),
                    )
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

    Ok(())
}

/// Spawn a background task that prewarms the overlay so the next
/// menu-open click doesn't pay recreate latency. Fire-and-forget — errors
/// are logged but don't surface; the on-click path will retry if needed.
///
/// Call this immediately after any operation that sets `stale=true`
/// (`wm_open`, pool replenish, dashboard switch/discard, terminal restore).
pub(crate) fn overlay_prewarm_in_background(
    overlay_arc: Arc<Mutex<OverlayInner>>,
    terminal_id: &str,
    app: &AppHandle,
) {
    let terminal_id = terminal_id.to_string();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = overlay_prewarm(overlay_arc, &terminal_id, &app).await {
            eprintln!("[overlay_prewarm bg] {terminal_id}: {e}");
        }
    });
}

/// Ensure the overlay webview is the topmost child, then position it
/// full-screen. Shared by all overlay-show commands.
async fn overlay_raise(
    overlay_arc: Arc<Mutex<OverlayInner>>,
    terminal_id: &str,
    app: &AppHandle,
) -> Result<(), String> {
    let overlay_label = format!("{terminal_id}-overlay");
    let win_label = terminal_id.to_string();

    overlay_prewarm(Arc::clone(&overlay_arc), &win_label, app).await?;

    // Restore the overlay to cover the full window so its backdrop
    // captures outside-clicks and menus render at the correct cursor
    // position. `set_bounds` updates both position and size in one OS-
    // level frame change, which is the most reliable way to bring the
    // webview back into hit-testing on macOS.
    let win = app
        .get_window(&win_label)
        .ok_or_else(|| format!("window '{}' not found", win_label))?;
    let sf = win.scale_factor().unwrap_or(1.0);
    let sz = win.inner_size().unwrap_or(tauri::PhysicalSize {
        width: 1600,
        height: 900,
    });
    let (w, h) = (sz.width as f64 / sf, sz.height as f64 / sf);
    app.get_webview(&overlay_label)
        .ok_or_else(|| format!("overlay webview '{}' not found", overlay_label))?
        .set_bounds(tauri::Rect {
            position: tauri::Position::Logical(LogicalPosition::new(0.0, 0.0)),
            size: tauri::Size::Logical(LogicalSize::new(w, h)),
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Show the context menu overlay at window position (`x`, `y`) for the Stack
/// at `stack_path` containing `n_tabs` tabs.
///
/// `kind` selects which item set the overlay renders:
///   - `"tab"` (default when omitted) — per-widget kebab + per-tab right-click.
///     Renders Add Widget, Duplicate, Rename, Reset name, Zoom, Reset zoom,
///     custom items, Close tab. Requires `tab_label` + `app_id`.
///   - `"stack-kebab"` — group kebab + strip-background right-click. Renders
///     Add Widget, Maximise/Restore group, Close all widgets. `tab_label` /
///     `app_id` may still be set (the active tab's, used as the `target` for
///     "Add Widget") but the menu shape is determined by `kind`.
///
/// `maximized` is forwarded so the group-kebab menu can label the toggle
/// "Maximise group" vs "Restore group" without an extra IPC round-trip.
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
    fdc3_channel: Option<String>,
    kind: Option<String>,
    maximized: Option<bool>,
    anchor: Option<String>,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    overlay_raise(Arc::clone(&terminal.overlay), window.label(), &app).await?;
    let overlay = format!("{}-overlay", window.label());
    app.get_webview(&overlay)
        .ok_or("overlay not found".to_string())?
        .emit(
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
                "fdc3Channel": fdc3_channel,
                "kind": kind.unwrap_or_else(|| "tab".to_string()),
                "maximized": maximized.unwrap_or(false),
                "anchor": anchor.unwrap_or_else(|| "left".to_string()),
            }),
        )
        .map_err(|e| e.to_string())
}

/// Signal the chrome webview to enter inline rename mode for `label`.
/// Emitted as `wm:request-rename` so the tab strip can focus the input
/// without a backend round-trip for state.
#[tauri::command]
fn wm_request_rename(label: String, app: AppHandle) {
    // Panel labels are "{terminal_id}-panel-{uuid}" — extract the terminal prefix.
    if let Some(terminal_id) = label.split("-panel-").next() {
        let chrome = format!("{}-chrome", terminal_id);
        if let Some(wv) = app.get_webview(&chrome) {
            let _ = wv.emit("wm:request-rename", serde_json::json!({ "label": label }));
        }
    }
}

/// Hide the overlay by parking it offscreen with a 1×1 hit area. Called
/// by the overlay itself when the user dismisses a menu or selects an
/// action.
///
/// We collapse the size to 1×1 (in addition to moving to -20000,-20000)
/// so that even if the OS-level hit-test caches the previous bounds for
/// a frame, the cached area is a single pixel far offscreen — clicks on
/// the chrome's kebab buttons can't accidentally land on the overlay.
/// Using `set_bounds` (rather than `hide()`/`show()`) avoids a separate
/// issue where `show()` doesn't reliably restore hit-testing in time for
/// the first click after a menu appears.
#[tauri::command]
fn wm_ctx_menu_close(window: Window, app: AppHandle) {
    let overlay_label = format!("{}-overlay", window.label());
    if let Some(wv) = app.get_webview(&overlay_label) {
        let _ = wv.set_bounds(tauri::Rect {
            position: tauri::Position::Logical(LogicalPosition::new(-20000.0, -20000.0)),
            size: tauri::Size::Logical(LogicalSize::new(1.0, 1.0)),
        });
    }
}

/// Open the command palette in the overlay webview.
#[tauri::command]
async fn wm_palette_open(
    commands: serde_json::Value,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    overlay_raise(Arc::clone(&terminal.overlay), window.label(), &app).await?;
    let overlay = format!("{}-overlay", window.label());
    app.get_webview(&overlay)
        .ok_or("overlay not found".to_string())?
        .emit("wm:palette-open", commands)
        .map_err(|e| e.to_string())
}

/// Show the tab overflow dropdown in the overlay webview. `payload` is the
/// opaque JSON built by the chrome (hidden tab list + button rect + stack path)
/// — Rust passes it through unchanged so only the frontend needs to know the
/// shape. Uses the same overlay_raise mechanism as the other menu commands.
#[tauri::command]
async fn wm_overflow_menu_open(
    payload: serde_json::Value,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    overlay_raise(Arc::clone(&terminal.overlay), window.label(), &app).await?;
    let overlay = format!("{}-overlay", window.label());
    app.get_webview(&overlay)
        .ok_or("overlay not found".to_string())?
        .emit("wm:overflow-menu", payload)
        .map_err(|e| e.to_string())
}

/// Show the engine picker dialog in the overlay webview. Used when a
/// multi-engine app is launched from the kebab / drawer / palette — moves
/// the picker out of chrome (where panel webviews would cover it and force
/// `wm_park_panels` to hide widgets) and into the overlay so widgets stay
/// visible behind the dialog and clicks land directly on the modal.
///
/// `payload` is the opaque `{ app, engines, target }` JSON built by the
/// chrome — Rust passes it through unchanged.
#[tauri::command]
async fn wm_engine_picker_open(
    payload: serde_json::Value,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    overlay_raise(Arc::clone(&terminal.overlay), window.label(), &app).await?;
    let overlay = format!("{}-overlay", window.label());
    app.get_webview(&overlay)
        .ok_or("overlay not found".to_string())?
        .emit("wm:engine-picker-open", payload)
        .map_err(|e| e.to_string())
}

/// Open the App Menu drawer in the overlay webview. Uses the same
/// overlay_raise mechanism as the palette / context menu so the drawer
/// sits above all panel webviews — widgets stay visible underneath.
///
/// `initial_section_id` is forwarded as the event payload so callers can
/// deep-link a specific section (e.g. "dashboards" for the pill right-click
/// "Manage…" item). The overlay's React side ignores it when omitted.
#[tauri::command]
async fn wm_menu_open(
    initial_section_id: Option<String>,
    target_label: Option<String>,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    overlay_raise(Arc::clone(&terminal.overlay), window.label(), &app).await?;
    let overlay = format!("{}-overlay", window.label());
    app.get_webview(&overlay)
        .ok_or("overlay not found".to_string())?
        .emit(
            "wm:menu-open",
            serde_json::json!({
                "initialSectionId": initial_section_id,
                "targetLabel": target_label,
            }),
        )
        .map_err(|e| e.to_string())
}

/// Open the "New dashboard" prompt in the overlay webview. The header's
/// "+" button and the `dashboard:create` palette command both call this;
/// the dialog sits above panels so widgets stay visible behind it.
#[tauri::command]
async fn wm_dashboard_create_open(
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    overlay_raise(Arc::clone(&terminal.overlay), window.label(), &app).await?;
    let overlay = format!("{}-overlay", window.label());
    app.get_webview(&overlay)
        .ok_or("overlay not found".to_string())?
        .emit("wm:dashboard-create-open", serde_json::json!({}))
        .map_err(|e| e.to_string())
}

/// Show the unsaved-changes confirm dialog in the overlay webview.
/// Triggered by useDashboards.switchTo when wm_switch_dashboard returns
/// NeedsConfirm. The overlay renders a Save / Discard / Cancel dialog
/// above panels (widgets stay visible underneath).
#[tauri::command]
async fn wm_dashboard_confirm_open(
    active_name: String,
    pending_name: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    overlay_raise(Arc::clone(&terminal.overlay), window.label(), &app).await?;
    let overlay = format!("{}-overlay", window.label());
    app.get_webview(&overlay)
        .ok_or("overlay not found".to_string())?
        .emit(
            "wm:dashboard-confirm-switch",
            serde_json::json!({
                "activeName": active_name,
                "pendingName": pending_name,
            }),
        )
        .map_err(|e| e.to_string())
}

// ── Dashboard commands (need OverlayState access) ─────────────────────────────

/// Switch the active dashboard to `name`, performing the full webview
/// destroy/recreate lifecycle.  Returns `DashboardError::NeedsConfirm` when
/// `auto_save` is off and there are unsaved changes — the frontend should show
/// a Save / Discard / Cancel dialog and retry accordingly.
#[tauri::command]
async fn wm_switch_dashboard(
    name: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    cfg: State<'_, TerminalConfig>,
    app: AppHandle,
) -> Result<(), DashboardError> {
    let terminal = manager
        .get(window.label())
        .ok_or_else(|| DashboardError::Other {
            message: format!("terminal '{}' not found", window.label()),
        })?;
    terminal
        .layout_tree
        .switch_dashboard(&name, &window, &app, &cfg.panel_init_script())?;
    {
        let mut inner = terminal.overlay.lock().unwrap();
        inner.stale = true;
        inner.is_ready = false;
    }
    overlay_prewarm_in_background(Arc::clone(&terminal.overlay), window.label(), &app);
    terminal.layout_tree.emit_dashboards(&app);
    Ok(())
}

/// Reload the active dashboard's saved snapshot, discarding all unsaved live
/// changes. Reconciles panel webviews (creates/destroys as needed) then
/// reflows and emits the updated layout events.
#[tauri::command]
async fn wm_discard_dashboard(
    window: Window,
    manager: State<'_, TerminalManager>,
    cfg: State<'_, TerminalConfig>,
    app: AppHandle,
) -> Result<(), DashboardError> {
    let terminal = manager
        .get(window.label())
        .ok_or_else(|| DashboardError::Other {
            message: format!("terminal '{}' not found", window.label()),
        })?;
    terminal
        .layout_tree
        .discard_dashboard(&window, &app, &cfg.panel_init_script())?;
    {
        let mut inner = terminal.overlay.lock().unwrap();
        inner.stale = true;
        inner.is_ready = false;
    }
    overlay_prewarm_in_background(Arc::clone(&terminal.overlay), window.label(), &app);
    terminal.layout_tree.emit_dashboards(&app);
    Ok(())
}

// ── Picker overlay parking ────────────────────────────────────────────────────
//
// Panel webviews sit above the chrome webview in z-order, so the chrome can't
// render dialogs that overlay them. The frontend calls `wm_park_panels` to
// hide all panels offscreen while the engine picker / download confirm
// dialog is visible, and `wm_unpark_panels` to restore the layout afterward.
// The webview processes stay alive — only their bounds move.

#[tauri::command]
fn wm_park_panels(
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    terminal.layout_tree.park_all(&app);
    Ok(())
}

#[tauri::command]
fn wm_unpark_panels(
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = get_terminal!(manager, window);
    terminal.layout_tree.reflow(&app);
    terminal.layout_tree.emit_host(&app);
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

/// Copy the current Terminal's active dashboard snapshot to another Terminal's
/// Dashboard list under `name`. Returns `false` if `name` is already taken in
/// the target Terminal or if `target_id` is not found.
#[tauri::command]
fn wm_duplicate_dashboard_to(
    name: String,
    target_id: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<bool, String> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("dashboard name must not be empty".into());
    }
    let source = get_terminal!(manager, window);
    let target = manager
        .get(&target_id)
        .ok_or_else(|| format!("terminal '{target_id}' not found"))?;

    // Take the saved snapshot of the active dashboard (not the live layout,
    // which may have unsaved changes if auto_save is off).
    let snapshot = source
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.dashboards.get(&ds.active).cloned());
    let Some(dashboard) = snapshot else {
        return Err("source has no active dashboard".into());
    };

    let created = target
        .layout_tree
        .with_dashboard_store_mut(|ds| ds.create_from(trimmed, dashboard));
    if created {
        target.layout_tree.persist_dashboards();
        target.layout_tree.emit_dashboards(&app);
    }
    Ok(created)
}

// ── Terminal lifecycle commands ───────────────────────────────────────────────

/// Spawn a brand-new Terminal window with zero dashboards and register it in
/// the TerminalManager. Called from the Desktop Agent tray "Open New Terminal"
/// action via IPC.
#[tauri::command]
fn wm_spawn_terminal(
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<terminal::state::TerminalInfo, String> {
    let label = manager.next_label();
    let pool_size = std::env::var("OT_WEBVIEW_POOL_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let info = terminal::spawn::spawn_terminal(&label, None, &manager, &app, pool_size, None)?;
    manager.emit_terminals(&app);
    Ok(info)
}

/// Return the window labels of all saved non-main terminals by scanning the
/// app data directory. Used by the Desktop Agent startup restore loop.
#[tauri::command]
fn wm_list_saved_terminals(app: AppHandle) -> Vec<String> {
    app.path()
        .app_data_dir()
        .map(|d| layout_persist::list_saved_terminal_ids(&d))
        .unwrap_or_default()
}

/// Close a Terminal window after the user has confirmed the prompt.
///
/// Removes the terminal from the registry, deletes its persisted state so it
/// is not restored on next startup, force-destroys the OS window, and emits
/// `wm:terminals` to keep any switcher UI in sync.
#[tauri::command]
fn wm_close_terminal(
    label: String,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    manager.remove(&label);

    if let Ok(data_dir) = app.path().app_data_dir() {
        if let Err(e) = layout_persist::delete_terminal_for(&label, &data_dir) {
            eprintln!("[wm_close_terminal] delete persist for {label}: {e}");
        }
    }

    // destroy() bypasses CloseRequested, so the confirm dialog is not re-shown.
    if let Some(win) = app.get_window(&label) {
        win.destroy().map_err(|e| e.to_string())?;
    }

    manager.emit_terminals(&app);
    Ok(())
}

// ── App entry point ───────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // When set by the Desktop Agent, skip all disk restore so this process
    // starts with a clean slate (zero dashboards, no extra terminal windows).
    let fresh_start = std::env::var("OT_FRESH_START").is_ok();

    let cfg = TerminalConfig::load();
    let panel_init_script = cfg.panel_init_script();
    let tree = LayoutTree::new(WIN, cfg.window.width, cfg.window.height);
    let identity = WmHostIdentity::from_env();
    let overlay_state: OverlayState = Arc::new(Mutex::new(OverlayInner::default()));
    let pool_size = std::env::var("OT_WEBVIEW_POOL_SIZE")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let pool = WebviewPool::new(pool_size);
    let manager = TerminalManager::new();
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
        .register_uri_scheme_protocol("sample-widget", sample_widget::handle)
        .manage(identity.clone())
        .manage(manager.clone())
        .manage(cfg.clone())
        .setup(move |app| {
            // ── Load persisted layout state ───────────────────────────────
            // Skip when OT_FRESH_START is set (Desktop Agent "Open New Terminal").
            if !fresh_start {
                tree.init(app.handle())?;
            }

            // ── Load saved window position for terminal-main ──────────────
            let saved_window_config: PersistedWindowConfig = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|d| layout_persist::load_terminal_for("terminal-main", &d))
                .map(|p| p.window)
                .unwrap_or_default();

            // ── Create the bare container window (no default webview) ──────
            let win = tauri::WindowBuilder::new(app.handle(), WIN)
                .title(&cfg.title)
                .inner_size(cfg.window.width, cfg.window.height)
                .min_inner_size(cfg.window.min_width, cfg.window.min_height)
                .resizable(true)
                .decorations(false)
                .build()?;

            // Apply saved position if the window rect is reachable on screen.
            if saved_window_config.width > 0.0
                && saved_window_config.height > 0.0
                && is_rect_on_any_monitor(
                    app.handle(),
                    saved_window_config.x,
                    saved_window_config.y,
                    saved_window_config.width,
                    saved_window_config.height,
                )
            {
                let _ = win.set_size(tauri::Size::Logical(LogicalSize::new(
                    saved_window_config.width,
                    saved_window_config.height,
                )));
                let _ = win.set_position(tauri::Position::Logical(LogicalPosition::new(
                    saved_window_config.x,
                    saved_window_config.y,
                )));
            }

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
                    let label = webview_pool::pool_label(WIN);
                    match win.add_child(
                        WebviewBuilder::new(
                            &label,
                            WebviewUrl::External(
                                "about:blank".parse().expect("about:blank is a valid URL"),
                            ),
                        )
                        .initialization_script(&panel_init_script),
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
                for (label, url, channel) in &panels_to_restore {
                    if let Ok(parsed_url) = url.parse::<tauri::Url>() {
                        let script =
                            config::append_initial_channel(&panel_init_script, channel.as_deref());
                        if let Err(e) = win.add_child(
                            WebviewBuilder::new(label, WebviewUrl::External(parsed_url))
                                .initialization_script(&script),
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
                overlay_prewarm_in_background(Arc::clone(&overlay_state), WIN, app.handle());
                tree.reflow(app.handle());
                tree.emit_host(app.handle());
                if let Some(snap) = tree.snapshot() {
                    if let Some(wv) = app.handle().get_webview(CHROME) {
                        let _ = wv.emit("wm:layout", &snap);
                    }
                }
            }

            // ── Register main terminal in TerminalManager ─────────────────
            let main_window_config = Arc::new(std::sync::RwLock::new(saved_window_config));
            {
                let main_state = Arc::new(TerminalState {
                    id: WIN.to_string(),
                    name: std::sync::RwLock::new(cfg.title.clone()),
                    layout_tree: tree.clone(),
                    overlay: overlay_state.clone(),
                    pool: pool.clone(),
                    window_config: Arc::clone(&main_window_config),
                });
                manager.register(main_state);
            }

            // ── Restore saved non-main terminals ──────────────────────────
            // Each saved terminal/<id>/dashboards.json becomes a new OS window
            // with its own layout and panel webviews. Skipped on fresh start.
            if !fresh_start {
                terminal::spawn::load_persisted_terminals(&manager, app.handle(), pool_size);
            }

            // Emit the initial terminal list to all chrome webviews.
            manager.emit_terminals(app.handle());

            // ── Resize + move listener for terminal-main ──────────────────
            install_window_listeners(
                WIN,
                &win,
                &tree,
                app.handle(),
                Arc::clone(&main_window_config),
            );

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            wm_config,
            widgets::wm_list_apps,
            wm_snapshot,
            wm_host_snapshot,
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
            wm_get_panel_fdc3_channel,
            wm_set_panel_fdc3_channel,
            wm_park_panels,
            wm_unpark_panels,
            wm_overlay_ready,
            wm_ctx_menu_open,
            wm_ctx_menu_close,
            wm_palette_open,
            wm_overflow_menu_open,
            wm_engine_picker_open,
            wm_menu_open,
            wm_dashboard_create_open,
            wm_dashboard_confirm_open,
            wm_request_rename,
            wm_list_dashboards,
            wm_switch_dashboard,
            wm_create_dashboard,
            wm_save_dashboard,
            wm_discard_dashboard,
            wm_rename_dashboard,
            wm_delete_dashboard,
            wm_reorder_dashboards,
            wm_set_auto_save,
            wm_duplicate_dashboard_to,
            wm_spawn_terminal,
            wm_list_saved_terminals,
            wm_close_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running one-terminal");
}
