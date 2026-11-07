//! Popout windows (Plan 17) — a panel detached from a Terminal's tiling tree
//! into its own OS window, and docked back.
//!
//! A popout is a dumb host: the panel's `LeafMeta` (and its entry in the
//! floating list) stays on the owning Terminal's `LayoutTree`, so the
//! navigation / page-load closures captured when the panel webview was
//! created stay valid. The window holds two child webviews, bottom to top:
//!
//! 1. the panel itself, moved in with `Webview::reparent` — no reload, so its
//!    JS state, WebSocket and FDC3 `instanceId` survive — inset by
//!    [`POPOUT_EDGE`] on the left/right/bottom so the bare window border takes
//!    resize hit-tests;
//! 2. `<popout>-chrome` (`popout.html`): the header strip, which also doubles
//!    as the menu overlay — it grows to the full window while a menu is open
//!    (`wm_popout_menu`), so menus need no extra webview or z-order dance.
//!
//! `reparent`/`add_child` always insert on top, so the chrome is always added
//! after the panel.
//!
//! Popouts of one Terminal can be glued together (Plan 17-H): the header is
//! dragged by [`wm_popout_drag_start`] / [`wm_popout_drag_move`] /
//! [`wm_popout_drag_end`] rather than the OS, so the windows can be snapped
//! against each other live ([`super::popout_snap`]) and a glued group moves as
//! one. Group membership lives on the floating entries, so it's saved with
//! the dashboard and survives stash / restore unchanged.
//!
//! Popout window labels are `<terminal_id>-popout-<id>`: still matched by the
//! `terminal-*` capability glob, and resolved back to their owning Terminal by
//! [`TerminalManager::resolve`] so every existing panel command works when
//! invoked from a popout.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, PhysicalPosition, State,
    WebviewBuilder, WebviewUrl, Window,
};
use tokio::sync::oneshot;

use crate::layout::host::StackTab;
use crate::layout::persist::{FloatingPanel, PersistedWindowConfig};
use crate::layout::store::LayoutTree;
use crate::layout::{ADDRESS_BAR_HEIGHT, GENERIC_WEB_WIDGET_APP_ID, PANEL_HEADER_HEIGHT};

use super::manager::TerminalManager;
use super::popout_snap::{self, Rect, SNAP_PX};
use super::spawn::is_rect_on_any_monitor;
use super::state::TerminalState;

/// Inset of the panel from the window's left, right and bottom edges. On
/// Windows the strip of bare window it leaves is where the OS resize hit-test
/// lands (a child webview covering the edge would swallow it); it's painted in
/// the theme surface colour ([`set_background`]). macOS resizes a frameless
/// window from its edges even over child views — as the Terminal window does —
/// so there the panel runs edge to edge with no border at all.
#[cfg(target_os = "macos")]
pub const POPOUT_EDGE: f64 = 0.0;
#[cfg(not(target_os = "macos"))]
pub const POPOUT_EDGE: f64 = 4.0;
pub const POPOUT_MIN_W: f64 = 320.0;
pub const POPOUT_MIN_H: f64 = 200.0;
const DEFAULT_W: f64 = 800.0;
const DEFAULT_H: f64 = 600.0;
/// Upper bound on how long a new popout stays hidden waiting for its chrome
/// to report ready — the panel is already live, so showing it a little
/// before the header paints beats a perceptible delay.
const SHOW_TIMEOUT: Duration = Duration::from_millis(150);

/// How the panel webview gets into a new popout window.
pub enum PanelSource {
    /// Move the live webview in from wherever it is now (no reload).
    Reparent,
    /// Create the panel webview fresh inside the popout — restore, a
    /// non-keep-alive dashboard switch back, or a Desktop Agent change.
    Create { url: String, init_script: String },
}

// ── Runtime state ─────────────────────────────────────────────────────────────

/// Popouts whose chrome hasn't reported ready yet → the waiter that shows
/// the window.
fn ready_waiters() -> &'static Mutex<HashMap<String, oneshot::Sender<()>>> {
    static W: OnceLock<Mutex<HashMap<String, oneshot::Sender<()>>>> = OnceLock::new();
    W.get_or_init(Default::default)
}

/// Theme surface colour last reported by a popout chrome — new popout windows
/// are built with it so the native background never flashes the OS default.
/// Defaults to the dark theme's `--ot-bg-surface`.
fn last_background() -> &'static Mutex<(u8, u8, u8)> {
    static C: OnceLock<Mutex<(u8, u8, u8)>> = OnceLock::new();
    C.get_or_init(|| Mutex::new((0x0f, 0x17, 0x2a)))
}

fn set_background(win: &Window, (r, g, b): (u8, u8, u8)) {
    if let Err(e) = win.set_background_color(Some(tauri::window::Color(r, g, b, 255))) {
        eprintln!("[popout] set background on {}: {e}", win.label());
    }
}

/// Popouts whose chrome is currently expanded to the full window for a menu.
fn menus_open() -> &'static Mutex<HashSet<String>> {
    static M: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    M.get_or_init(Default::default)
}

/// A header drag in progress (Plan 17-H), keyed by the dragged popout.
struct Drag {
    /// Cursor position at drag start, physical desktop pixels.
    cursor: PhysicalPosition<f64>,
    /// Windows that move: `(popout, start rect)` — the dragged one and the
    /// members of its glue group.
    moving: Vec<(String, Rect)>,
    /// The owner's other popouts, which stay put and attract the drag.
    others: Vec<(String, Rect)>,
    /// Snap distance in physical pixels at the dragged window's scale.
    threshold: f64,
    /// Popouts the moving windows currently touch — what they glue to on
    /// release, highlighted meanwhile.
    contacts: Vec<String>,
}

fn drags() -> &'static Mutex<HashMap<String, Drag>> {
    static D: OnceLock<Mutex<HashMap<String, Drag>>> = OnceLock::new();
    D.get_or_init(Default::default)
}

/// Popouts focused programmatically by [`raise_group`], whose `Focused`
/// event must not raise their group again. Entries expire so a focus event
/// that never arrives can't swallow a later real click.
fn focus_echoes() -> &'static Mutex<HashMap<String, Instant>> {
    static F: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    F.get_or_init(Default::default)
}
const FOCUS_ECHO_TTL: Duration = Duration::from_millis(1000);

pub fn popout_label(owner: &str) -> String {
    format!(
        "{owner}-popout-{}",
        &uuid::Uuid::new_v4().simple().to_string()[..8]
    )
}

fn chrome_label(popout: &str) -> String {
    format!("{popout}-chrome")
}

// ── Geometry ──────────────────────────────────────────────────────────────────

/// Height the popout header reserves above the panel — the same reservation
/// `reflow` makes for a standalone panel header, plus the Generic Web Widget
/// address bar when shown.
fn header_height(tab: Option<&StackTab>) -> f64 {
    match tab {
        Some(t) if t.app_id == GENERIC_WEB_WIDGET_APP_ID && t.show_address_bar => {
            PANEL_HEADER_HEIGHT + ADDRESS_BAR_HEIGHT
        }
        _ => PANEL_HEADER_HEIGHT,
    }
}

/// Position the chrome and panel webviews inside a popout of logical size
/// `w`×`h`. Pure bounds updates — no tree reflow, no IPC to JS — so it's
/// cheap enough to run on every `Resized` event during a live resize.
pub fn place(app: &AppHandle, popout: &str, panel: &str, tree: &LayoutTree, w: f64, h: f64) {
    let header = header_height(tree.panel_tab(panel).as_ref());
    let menu_open = menus_open().lock().unwrap().contains(popout);
    if let Some(chrome) = app.get_webview(&chrome_label(popout)) {
        let ch = if menu_open { h } else { header };
        let _ = chrome.set_bounds(rect(0.0, 0.0, w, ch));
    }
    if let Some(wv) = app.get_webview(panel) {
        let _ = wv.set_bounds(rect(
            POPOUT_EDGE,
            header,
            (w - 2.0 * POPOUT_EDGE).max(1.0),
            (h - header - POPOUT_EDGE).max(1.0),
        ));
    }
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> tauri::Rect {
    tauri::Rect {
        position: tauri::Position::Logical(LogicalPosition::new(x, y)),
        size: tauri::Size::Logical(LogicalSize::new(w, h)),
    }
}

fn logical_inner_size(win: &Window) -> (f64, f64) {
    let sf = win.scale_factor().unwrap_or(1.0);
    win.inner_size()
        .map(|s| (s.width as f64 / sf, s.height as f64 / sf))
        .unwrap_or((DEFAULT_W, DEFAULT_H))
}

/// Screen-space logical rect of panel `label` as currently laid out in
/// `window` — a popout opens exactly there, so the widget appears to lift off
/// in place. `None` if the panel is parked offscreen or geometry is missing.
pub fn panel_screen_rect(
    app: &AppHandle,
    window: &str,
    label: &str,
) -> Option<PersistedWindowConfig> {
    let win = app.get_window(window)?;
    let wv = app.get_webview(label)?;
    let sf = win.scale_factor().ok()?;
    let origin = win.inner_position().ok()?;
    let pos = wv.position().ok()?;
    let size = wv.size().ok()?;
    let (x, y) = (pos.x as f64 / sf, pos.y as f64 / sf);
    let (w, h) = (size.width as f64 / sf, size.height as f64 / sf);
    if x < 0.0 || y < 0.0 || w < 50.0 || h < 50.0 {
        return None;
    }
    // Grow by the popout's own header + edges so the content keeps its size.
    Some(PersistedWindowConfig {
        x: origin.x as f64 / sf + x - POPOUT_EDGE,
        y: origin.y as f64 / sf + y - PANEL_HEADER_HEIGHT,
        width: (w + 2.0 * POPOUT_EDGE).max(POPOUT_MIN_W),
        height: (h + PANEL_HEADER_HEIGHT + POPOUT_EDGE).max(POPOUT_MIN_H),
        monitor: None,
    })
}

/// Saved bounds if they're set and still on a connected monitor; else
/// `None` (the OS picks a position, at the default size).
fn usable_bounds(app: &AppHandle, b: &PersistedWindowConfig) -> Option<PersistedWindowConfig> {
    (b.width > 0.0 && b.height > 0.0 && is_rect_on_any_monitor(app, b.x, b.y, b.width, b.height))
        .then(|| b.clone())
}

// ── Open / dock ───────────────────────────────────────────────────────────────

/// Open a popout window for the floating panel `floating` owned by
/// `terminal`, and register it with `manager`. Built hidden and shown once
/// its chrome reports ready (or after [`SHOW_TIMEOUT`]) so the user sees one
/// finished frame — no blank-window flash. Returns the popout window label.
///
/// Must not be called on the main thread: the window work is dispatched
/// there and awaited (creating windows from a synchronous context can
/// deadlock on Windows).
pub fn open_popout(
    app: &AppHandle,
    manager: &TerminalManager,
    terminal: &TerminalState,
    floating: &FloatingPanel,
    source: PanelSource,
) -> Result<String, String> {
    let started = std::time::Instant::now();
    let popout = popout_label(&terminal.id);
    let panel = floating.label.clone();
    let tree = terminal.layout_tree.clone();
    let bounds = usable_bounds(app, &floating.bounds);
    let title = tree
        .panel_tab(&panel)
        .map(|t| t.display_name.unwrap_or(t.title))
        .unwrap_or_default();

    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    {
        let app_m = app.clone();
        let popout_m = popout.clone();
        let panel_m = panel.clone();
        let owner_m = terminal.id.clone();
        let tree_m = tree.clone();
        app.run_on_main_thread(move || {
            let r = (|| -> Result<(), String> {
                let (w, h) = bounds
                    .as_ref()
                    .map(|b| (b.width, b.height))
                    .unwrap_or((DEFAULT_W, DEFAULT_H));
                let mut builder = tauri::WindowBuilder::new(&app_m, &popout_m)
                    .title(format!("OneTerminal — {title}"))
                    .inner_size(w, h)
                    .min_inner_size(POPOUT_MIN_W, POPOUT_MIN_H)
                    .resizable(true)
                    .decorations(false)
                    .background_color({
                        let (r, g, b) = *last_background().lock().unwrap();
                        tauri::window::Color(r, g, b, 255)
                    })
                    .visible(false);
                if let Some(b) = &bounds {
                    builder = builder.position(b.x, b.y);
                }
                let win = builder.build().map_err(|e| e.to_string())?;
                // Tauri attaches its undecorated-window resize handler only
                // to windows created with a content webview; toggling
                // resizable re-attaches it for this bare window (Windows).
                let _ = win.set_resizable(true);

                match source {
                    PanelSource::Reparent => {
                        let wv = app_m
                            .get_webview(&panel_m)
                            .ok_or_else(|| format!("panel '{panel_m}' not found"))?;
                        wv.reparent(&win).map_err(|e| e.to_string())?;
                    }
                    PanelSource::Create { url, init_script } => {
                        let parsed: tauri::Url = url.parse().map_err(|e| format!("{e}"))?;
                        win.add_child(
                            WebviewBuilder::new(&panel_m, WebviewUrl::External(parsed))
                                .initialization_script(&init_script)
                                .on_navigation(crate::address_bar_navigation_handler(
                                    app_m.clone(),
                                    owner_m.clone(),
                                    panel_m.clone(),
                                    tree_m.clone(),
                                ))
                                .on_page_load(crate::channel_sync_page_load_handler(
                                    panel_m.clone(),
                                    tree_m.clone(),
                                )),
                            LogicalPosition::new(0.0, 0.0),
                            LogicalSize::new(1.0, 1.0),
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }

                // Chrome last so it sits above the panel (menus draw over it).
                let url = format!("popout.html?panel={}", urlencode(&panel_m));
                win.add_child(
                    WebviewBuilder::new(chrome_label(&popout_m), WebviewUrl::App(url.into()))
                        .transparent(true),
                    LogicalPosition::new(0.0, 0.0),
                    LogicalSize::new(w, PANEL_HEADER_HEIGHT),
                )
                .map_err(|e| e.to_string())?;

                let (lw, lh) = logical_inner_size(&win);
                place(&app_m, &popout_m, &panel_m, &tree_m, lw, lh);
                Ok(())
            })();
            let _ = tx.send(r);
        })
        .map_err(|e| e.to_string())?;
    }
    if let Err(e) = rx.recv().map_err(|e| e.to_string())? {
        // Roll back a half-built window so no invisible orphan lingers.
        if let Some(w) = app.get_window(&popout) {
            let _ = w.destroy();
        }
        return Err(e);
    }

    manager.register_popout(&popout, &terminal.id, &panel);
    install_listeners(app, &popout, &panel, &tree);
    show_when_ready(app, &popout);
    eprintln!(
        "[popout] open {popout} (panel={panel}) in {} ms",
        started.elapsed().as_millis()
    );
    Ok(popout)
}

/// Move panel `panel` from popout `popout` back into its owner Terminal's
/// window and destroy the popout. The caller docks the tree (`tree.dock`),
/// reflows, and marks the owner's overlay stale (the panel now sits above
/// it in z-order).
pub fn reparent_back(
    app: &AppHandle,
    manager: &TerminalManager,
    owner: &str,
    popout: &str,
    panel: &str,
) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let app_m = app.clone();
    let owner_m = owner.to_string();
    let panel_m = panel.to_string();
    app.run_on_main_thread(move || {
        let r = (|| -> Result<(), String> {
            let win = app_m
                .get_window(&owner_m)
                .ok_or_else(|| format!("window '{owner_m}' not found"))?;
            if let Some(wv) = app_m.get_webview(&panel_m) {
                wv.reparent(&win).map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        let _ = tx.send(r);
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())??;
    destroy_popout(app, manager, popout);
    Ok(())
}

/// Forget and destroy popout window `popout` (whatever it still hosts goes
/// with it — callers move the panel out first when it should survive).
pub fn destroy_popout(app: &AppHandle, manager: &TerminalManager, popout: &str) {
    manager.remove_popout(popout);
    menus_open().lock().unwrap().remove(popout);
    ready_waiters().lock().unwrap().remove(popout);
    drags().lock().unwrap().remove(popout);
    focus_echoes().lock().unwrap().remove(popout);
    if let Some(w) = app.get_window(popout) {
        let _ = w.destroy();
    }
}

/// Show `popout` once its chrome calls `wm_popout_ready`, or after
/// [`SHOW_TIMEOUT`] — whichever comes first.
fn show_when_ready(app: &AppHandle, popout: &str) {
    let (tx, rx) = oneshot::channel();
    ready_waiters()
        .lock()
        .unwrap()
        .insert(popout.to_string(), tx);
    let app = app.clone();
    let popout = popout.to_string();
    tauri::async_runtime::spawn(async move {
        let _ = tokio::time::timeout(SHOW_TIMEOUT, rx).await;
        ready_waiters().lock().unwrap().remove(&popout);
        if let Some(w) = app.get_window(&popout) {
            let _ = w.show();
            let _ = w.set_focus();
        }
    });
}

/// Resize/move/close listeners for a popout window.
fn install_listeners(app: &AppHandle, popout: &str, panel: &str, tree: &LayoutTree) {
    let Some(win) = app.get_window(popout) else {
        return;
    };
    let app_h = app.clone();
    let popout_s = popout.to_string();
    let panel_s = panel.to_string();
    let tree_c = tree.clone();
    win.on_window_event(move |evt| match evt {
        tauri::WindowEvent::Resized(_) | tauri::WindowEvent::ScaleFactorChanged { .. } => {
            let Some(w) = app_h.get_window(&popout_s) else {
                return;
            };
            let (lw, lh) = logical_inner_size(&w);
            place(&app_h, &popout_s, &panel_s, &tree_c, lw, lh);
            save_bounds(&w, &panel_s, &tree_c);
        }
        tauri::WindowEvent::Moved(_) => {
            if let Some(w) = app_h.get_window(&popout_s) {
                save_bounds(&w, &panel_s, &tree_c);
            }
        }
        tauri::WindowEvent::Focused(true) => {
            // Widget hotkeys / palette commands target the active panel.
            tree_c.set_active_panel(&panel_s);
            // Bring glued members up with it so the group stays together in
            // z-order — unless this focus is our own raise echoing back.
            let echo = focus_echoes()
                .lock()
                .unwrap()
                .remove(&popout_s)
                .is_some_and(|at| at.elapsed() < FOCUS_ECHO_TTL);
            if !echo {
                let manager = app_h.state::<TerminalManager>();
                raise_group(&app_h, &manager, &tree_c, &popout_s, &panel_s);
            }
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            // Closing the window docks the widget back; closing the widget
            // itself is the menu's Close item.
            api.prevent_close();
            let app = app_h.clone();
            let panel = panel_s.clone();
            tauri::async_runtime::spawn(async move {
                let manager = app.state::<TerminalManager>();
                if let Err(e) = dock_panel(&app, &manager, &panel) {
                    eprintln!("[popout] dock on close failed for {panel}: {e}");
                }
            });
        }
        _ => {}
    });
}

/// Record the window's current geometry on the floating entry (debounced by
/// the tree's own save). Skipped while minimised or filling the monitor's
/// work area (maximised), so restoring the popout reopens it at its normal
/// size.
///
/// Never calls `Window::is_maximized` here: on macOS, tao answers it for a
/// frameless window by briefly toggling the NSWindow style mask, which
/// detaches and re-attaches the webviews and fires another `Resized` /
/// `Moved` — calling it from those events loops forever and freezes the app.
fn save_bounds(win: &Window, panel: &str, tree: &LayoutTree) {
    if win.is_minimized().unwrap_or(false) {
        return;
    }
    let sf = win.scale_factor().unwrap_or(1.0);
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.inner_size()) else {
        return;
    };
    let monitor = win.current_monitor().ok().flatten();
    if monitor
        .as_ref()
        .is_some_and(|m| fills_work_area(m.work_area(), pos, size))
    {
        return;
    }
    tree.set_floating_bounds(
        panel,
        PersistedWindowConfig {
            x: pos.x as f64 / sf,
            y: pos.y as f64 / sf,
            width: size.width as f64 / sf,
            height: size.height as f64 / sf,
            monitor: monitor.and_then(|m| m.name().cloned()),
        },
    );
}

/// `true` when a window at `pos` with inner `size` covers (almost) all of a
/// monitor's work area — what maximising does. A safe stand-in for
/// `is_maximized`, see [`save_bounds`]. All values are physical pixels.
fn fills_work_area(
    area: &tauri::PhysicalRect<i32, u32>,
    pos: tauri::PhysicalPosition<i32>,
    size: tauri::PhysicalSize<u32>,
) -> bool {
    const SLACK: i64 = 16;
    let (ax, ay) = (area.position.x as i64, area.position.y as i64);
    let (aw, ah) = (area.size.width as i64, area.size.height as i64);
    (pos.x as i64 - ax).abs() <= SLACK
        && (pos.y as i64 - ay).abs() <= SLACK
        && size.width as i64 >= aw - SLACK
        && size.height as i64 >= ah - SLACK
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ── Lifecycle (dashboard switch, Desktop Agent change, close) ─────────────────

/// Move every popped-out panel of `terminal` back into the Terminal window
/// (parked offscreen) and close their popout windows, leaving the floating
/// entries untouched. Run before an operation that swaps the live layout, so
/// its existing park-or-destroy handling covers popped-out panels exactly
/// like tiled ones; [`restore_popouts`] reopens them afterwards.
pub fn stash_popouts(app: &AppHandle, manager: &TerminalManager, terminal: &TerminalState) {
    for (popout, panel) in manager.popouts_of(&terminal.id) {
        if let Err(e) = reparent_back(app, manager, &terminal.id, &popout, &panel) {
            eprintln!("[popout] stash {panel}: {e}");
            destroy_popout(app, manager, &popout);
        }
        crate::layout::reflow::park_label_offscreen(app, &panel);
    }
}

/// Close every popout window of `terminal` together with the panel webview it
/// hosts. The floating entries stay, so [`restore_popouts`] recreates them.
pub fn close_popouts(app: &AppHandle, manager: &TerminalManager, terminal: &TerminalState) {
    for (popout, _) in manager.popouts_of(&terminal.id) {
        destroy_popout(app, manager, &popout);
    }
}

/// Open a popout for every floating panel of `terminal`'s live layout that
/// doesn't have one: reusing its webview if it's still alive (just stashed,
/// or parked by keep-alive) or creating it fresh otherwise.
pub fn restore_popouts(app: &AppHandle, manager: &TerminalManager, terminal: &TerminalState) {
    let started = std::time::Instant::now();
    let base_script = crate::agent::panel_init_script(app);
    let mut opened = 0;
    for (floating, url, channel) in terminal.layout_tree.floating() {
        if manager.popout_for_panel(&floating.label).is_some() {
            continue;
        }
        let source = if app.get_webview(&floating.label).is_some() {
            terminal.layout_tree.take_parked(&floating.label);
            PanelSource::Reparent
        } else {
            PanelSource::Create {
                url,
                init_script: crate::config::append_initial_channel(
                    &base_script,
                    channel.as_deref(),
                ),
            }
        };
        match open_popout(app, manager, terminal, &floating, source) {
            Ok(_) => opened += 1,
            Err(e) => eprintln!("[popout] restore {}: {e}", floating.label),
        }
    }
    if opened > 0 {
        eprintln!(
            "[popout] restored {opened} popout(s) for {} in {} ms",
            terminal.id,
            started.elapsed().as_millis()
        );
    }
}

/// Run `op` (which swaps `terminal`'s live layout) with its popouts stashed,
/// then reopen popouts for whatever layout is live afterwards — also when
/// `op` fails, so a rejected switch (e.g. needs confirm) loses nothing.
pub fn with_popouts_stashed<T>(
    app: &AppHandle,
    manager: &TerminalManager,
    terminal: &TerminalState,
    op: impl FnOnce() -> T,
) -> T {
    stash_popouts(app, manager, terminal);
    let out = op();
    restore_popouts(app, manager, terminal);
    out
}

// ── Flows shared by commands and window events ────────────────────────────────

/// Pop panel `label` out of `terminal`'s tree into a new popout window,
/// opening it over the panel's current on-screen rect.
pub fn popout_panel(
    app: &AppHandle,
    manager: &TerminalManager,
    terminal: &TerminalState,
    label: &str,
) -> Result<String, String> {
    if manager.popout_for_panel(label).is_some() {
        return Err(format!("panel '{label}' is already popped out"));
    }
    let rect = panel_screen_rect(app, &terminal.id, label);
    let mut floating = terminal
        .layout_tree
        .detach(label)
        .ok_or_else(|| format!("no tiled panel '{label}'"))?;
    if let Some(r) = rect {
        floating.bounds = r.clone();
        terminal.layout_tree.set_floating_bounds(label, r);
    }
    let popout = match open_popout(app, manager, terminal, &floating, PanelSource::Reparent) {
        Ok(p) => p,
        Err(e) => {
            // Put it back so a failed popout never strands the panel.
            terminal.layout_tree.dock(label);
            terminal.layout_tree.reflow(app);
            return Err(e);
        }
    };
    terminal.layout_tree.reflow(app);
    manager.notify_panel_changed(app, terminal, label);
    Ok(popout)
}

/// Dock floating panel `label` back into its owner Terminal.
pub fn dock_panel(app: &AppHandle, manager: &TerminalManager, label: &str) -> Result<(), String> {
    let started = std::time::Instant::now();
    let terminal = manager
        .owner_of_panel(label)
        .ok_or_else(|| format!("no panel '{label}'"))?;
    if !terminal.layout_tree.is_floating(label) {
        return Err(format!("panel '{label}' is not popped out"));
    }
    let group = group_of(&terminal.layout_tree, label);
    if let Some(popout) = manager.popout_for_panel(label) {
        reparent_back(app, manager, &terminal.id, &popout, label)?;
    }
    terminal.layout_tree.dock(label);
    terminal.layout_tree.reflow(app);
    manager.notify_panel_changed(app, &terminal, label);
    notify_panels(app, manager, &terminal, &group);
    // The panel re-entered above the overlay — recreate the overlay on top
    // in the background so the next menu opens instantly.
    {
        let mut inner = terminal.overlay.lock().unwrap();
        inner.stale = true;
        inner.is_ready = false;
    }
    crate::overlay_prewarm_in_background(Arc::clone(&terminal.overlay), &terminal.id, app);
    if let Some(w) = app.get_window(&terminal.id) {
        let _ = w.set_focus();
    }
    eprintln!(
        "[popout] dock {label} in {} ms",
        started.elapsed().as_millis()
    );
    Ok(())
}

// ── Glue groups (Plan 17-H) ───────────────────────────────────────────────────

/// Labels of every panel glued with `label`, itself included; empty if it
/// isn't glued.
pub fn group_of(tree: &LayoutTree, label: &str) -> Vec<String> {
    tree.floating_group(label)
        .map(|g| tree.group_members(&g))
        .unwrap_or_default()
}

/// Re-send header state to each of `labels` — after a glue change, so every
/// affected popout chrome shows its glued state and menu.
pub fn notify_panels(
    app: &AppHandle,
    manager: &TerminalManager,
    terminal: &TerminalState,
    labels: &[String],
) {
    for label in labels {
        manager.notify_panel_changed(app, terminal, label);
    }
}

/// Panel hosted by popout window `popout` of Terminal `owner`.
fn panel_in(manager: &TerminalManager, owner: &str, popout: &str) -> Option<String> {
    manager
        .popouts_of(owner)
        .into_iter()
        .find(|(p, _)| p == popout)
        .map(|(_, panel)| panel)
}

/// Outer rect of window `label` in physical desktop pixels.
fn outer_rect(app: &AppHandle, label: &str) -> Option<Rect> {
    let w = app.get_window(label)?;
    let pos = w.outer_position().ok()?;
    let size = w.outer_size().ok()?;
    Some(Rect::new(
        pos.x as f64,
        pos.y as f64,
        size.width as f64,
        size.height as f64,
    ))
}

/// Bring the other members of `panel`'s glue group to the front behind
/// popout `front`, restoring any that are minimised, then refocus `front`.
fn raise_group(
    app: &AppHandle,
    manager: &TerminalManager,
    tree: &LayoutTree,
    front: &str,
    panel: &str,
) {
    let others: Vec<String> = group_of(tree, panel)
        .iter()
        .filter(|l| *l != panel)
        .filter_map(|l| manager.popout_for_panel(l))
        .collect();
    if others.is_empty() {
        return;
    }
    {
        let now = Instant::now();
        let mut echoes = focus_echoes().lock().unwrap();
        for p in others.iter().map(String::as_str).chain([front]) {
            echoes.insert(p.to_string(), now);
        }
    }
    for p in &others {
        if let Some(w) = app.get_window(p) {
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }
    if let Some(w) = app.get_window(front) {
        let _ = w.set_focus();
    }
}

/// Tell the chromes whose glue highlight changed from `before` to `after`.
fn emit_snap_highlight(app: &AppHandle, before: &[String], after: &[String]) {
    for p in before.iter().chain(after) {
        let on = after.contains(p);
        if on == before.contains(p) {
            continue;
        }
        if let Some(wv) = app.get_webview(&chrome_label(p)) {
            let _ = wv.emit("wm:popout-snap", on);
        }
    }
}

/// Start dragging the invoking popout by its header. Its glue group moves
/// with it, unless `tear_off` (Alt / Option held), which first unglues it so
/// it moves — and can snap — on its own.
#[tauri::command]
pub fn wm_popout_drag_start(
    tear_off: bool,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let popout = window.label().to_string();
    let terminal = manager
        .resolve(&popout)
        .ok_or_else(|| format!("popout '{popout}' not found"))?;
    let tree = &terminal.layout_tree;
    let panel = panel_in(&manager, &terminal.id, &popout)
        .ok_or_else(|| format!("popout '{popout}' not found"))?;
    if tear_off {
        let group = group_of(tree, &panel);
        if tree.unglue(&panel) {
            notify_panels(&app, &manager, &terminal, &group);
        }
    }
    let members = group_of(tree, &panel);
    let moving_popouts: HashSet<String> = members
        .iter()
        .filter_map(|l| manager.popout_for_panel(l))
        .chain([popout.clone()])
        .collect();
    let rect_of = |(p, _): (String, String)| outer_rect(&app, &p).map(|r| (p, r));
    let (moving, others): (Vec<_>, Vec<_>) = manager
        .popouts_of(&terminal.id)
        .into_iter()
        .filter_map(rect_of)
        .partition(|(p, _)| moving_popouts.contains(p));
    let cursor = window.cursor_position().map_err(|e| e.to_string())?;
    let threshold = SNAP_PX * window.scale_factor().unwrap_or(1.0);
    drags().lock().unwrap().insert(
        popout.clone(),
        Drag {
            cursor,
            moving,
            others,
            threshold,
            contacts: Vec::new(),
        },
    );
    raise_group(&app, &manager, tree, &popout, &panel);
    Ok(())
}

/// Move the dragged popout (and its group) to follow the cursor, snapping
/// flush onto any nearby popout edge.
#[tauri::command]
pub fn wm_popout_drag_move(window: Window, app: AppHandle) -> Result<(), String> {
    let popout = window.label();
    let cursor = window.cursor_position().map_err(|e| e.to_string())?;
    let mut drags = drags().lock().unwrap();
    let Some(drag) = drags.get_mut(popout) else {
        return Ok(()); // drag ended or never started
    };
    let (dx, dy) = (cursor.x - drag.cursor.x, cursor.y - drag.cursor.y);
    let moved: Vec<Rect> = drag
        .moving
        .iter()
        .map(|(_, r)| Rect::new(r.x + dx, r.y + dy, r.w, r.h))
        .collect();
    let snap = popout_snap::snap(&moved, &drag.others, drag.threshold);
    for ((p, _), r) in drag.moving.iter().zip(&moved) {
        if let Some(w) = app.get_window(p) {
            let _ = w.set_position(PhysicalPosition::new(
                (r.x + snap.dx).round() as i32,
                (r.y + snap.dy).round() as i32,
            ));
        }
    }
    // While touching anything, the moving windows and what they touch light up.
    let lit = |contacts: &[String]| -> Vec<String> {
        if contacts.is_empty() {
            return Vec::new();
        }
        let moving = drag.moving.iter().map(|(p, _)| p.clone());
        moving.chain(contacts.iter().cloned()).collect()
    };
    let (before_lit, now_lit) = (lit(&drag.contacts), lit(&snap.contacts));
    drag.contacts = snap.contacts;
    emit_snap_highlight(&app, &before_lit, &now_lit);
    Ok(())
}

/// Finish a header drag: glue the moving windows to every popout they're
/// left touching.
#[tauri::command]
pub fn wm_popout_drag_end(
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let popout = window.label().to_string();
    let Some(drag) = drags().lock().unwrap().remove(&popout) else {
        return Ok(());
    };
    let moving: Vec<String> = drag.moving.iter().map(|(p, _)| p.clone()).collect();
    if !drag.contacts.is_empty() {
        emit_snap_highlight(
            &app,
            &moving
                .iter()
                .chain(&drag.contacts)
                .cloned()
                .collect::<Vec<_>>(),
            &[],
        );
    }
    let terminal = manager
        .resolve(&popout)
        .ok_or_else(|| format!("popout '{popout}' not found"))?;
    let tree = &terminal.layout_tree;
    let panels: Vec<String> = moving
        .iter()
        .chain(&drag.contacts)
        .filter_map(|p| panel_in(&manager, &terminal.id, p))
        .collect();
    if !drag.contacts.is_empty() {
        if let Some(group) = tree.glue(&panels) {
            notify_panels(&app, &manager, &terminal, &tree.group_members(&group));
            eprintln!("[popout] glued {} popout(s) into {group}", panels.len());
        }
    }
    for p in &moving {
        if let (Some(w), Some(panel)) = (app.get_window(p), panel_in(&manager, &terminal.id, p)) {
            save_bounds(&w, &panel, tree);
        }
    }
    Ok(())
}

/// Unglue panel `label` from its group, leaving its window where it is.
#[tauri::command]
pub fn wm_popout_unglue(
    label: String,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let terminal = manager
        .owner_of_panel(&label)
        .ok_or_else(|| format!("no panel '{label}'"))?;
    let group = group_of(&terminal.layout_tree, &label);
    if terminal.layout_tree.unglue(&label) {
        notify_panels(&app, &manager, &terminal, &group);
    }
    Ok(())
}

/// Minimise the invoking popout together with every popout glued to it.
#[tauri::command]
pub fn wm_popout_minimize(
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let popout = window.label().to_string();
    let mut targets = vec![popout.clone()];
    if let Some(terminal) = manager.resolve(&popout) {
        if let Some(panel) = panel_in(&manager, &terminal.id, &popout) {
            targets.extend(
                group_of(&terminal.layout_tree, &panel)
                    .iter()
                    .filter_map(|l| manager.popout_for_panel(l))
                    .filter(|p| *p != popout),
            );
        }
    }
    for p in targets {
        if let Some(w) = app.get_window(&p) {
            let _ = w.minimize();
        }
    }
    Ok(())
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// Pop the panel `label` out of the invoking Terminal into its own window.
#[tauri::command]
pub async fn wm_popout_panel(
    label: String,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<String, String> {
    let terminal = manager
        .resolve(window.label())
        .ok_or_else(|| format!("terminal '{}' not found", window.label()))?;
    popout_panel(&app, &manager, &terminal, &label)
}

/// Dock the popped-out panel `label` back into its owning Terminal.
/// Callable from the popout or from the Terminal.
#[tauri::command]
pub async fn wm_dock_panel(
    label: String,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    dock_panel(&app, &manager, &label)
}

/// Bring the popout hosting panel `label` to the front.
#[tauri::command]
pub fn wm_focus_popout(
    label: String,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let popout = manager
        .popout_for_panel(&label)
        .ok_or_else(|| format!("panel '{label}' is not popped out"))?;
    let w = app
        .get_window(&popout)
        .ok_or_else(|| format!("window '{popout}' not found"))?;
    let _ = w.unminimize();
    let _ = w.show();
    w.set_focus().map_err(|e| e.to_string())
}

/// Called by the popout chrome once mounted: shows the (hidden) window and
/// returns the panel's header state for the first render.
#[tauri::command]
pub fn wm_popout_ready(window: Window, manager: State<'_, TerminalManager>) -> Option<StackTab> {
    let popout = window.label().to_string();
    if let Some(tx) = ready_waiters().lock().unwrap().remove(&popout) {
        let _ = tx.send(());
    }
    let terminal = manager.resolve(&popout)?;
    let panel = panel_in(&manager, &terminal.id, &popout)?;
    terminal.layout_tree.panel_tab(&panel)
}

/// Paint a popout's native window background (the thin resize border around
/// the panel on Windows) in the theme surface colour `rgb`. Called by the
/// popout chrome on mount and on theme change; remembered for new popouts.
#[tauri::command]
pub fn wm_popout_background(rgb: (u8, u8, u8), window: Window) {
    *last_background().lock().unwrap() = rgb;
    set_background(&window, rgb);
}

/// Grow the popout chrome to the full window while a menu is open (so the
/// menu draws above the panel), or shrink it back to the header strip.
#[tauri::command]
pub fn wm_popout_menu(
    open: bool,
    window: Window,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<(), String> {
    let popout = window.label().to_string();
    let terminal = manager
        .resolve(&popout)
        .ok_or_else(|| format!("popout '{popout}' not found"))?;
    let panel = panel_in(&manager, &terminal.id, &popout)
        .ok_or_else(|| format!("popout '{popout}' not found"))?;
    {
        let mut set = menus_open().lock().unwrap();
        if open {
            set.insert(popout.clone());
        } else {
            set.remove(&popout);
        }
    }
    let (w, h) = logical_inner_size(&window);
    place(&app, &popout, &panel, &terminal.layout_tree, w, h);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri::{PhysicalPosition, PhysicalRect, PhysicalSize};

    fn area() -> PhysicalRect<i32, u32> {
        PhysicalRect {
            position: PhysicalPosition::new(0, 50),
            size: PhysicalSize::new(2880, 1750),
        }
    }

    #[test]
    fn a_window_covering_the_work_area_counts_as_maximised() {
        assert!(fills_work_area(
            &area(),
            PhysicalPosition::new(0, 50),
            PhysicalSize::new(2880, 1750)
        ));
        // Within slack (frame rounding, title-bar inset).
        assert!(fills_work_area(
            &area(),
            PhysicalPosition::new(4, 58),
            PhysicalSize::new(2872, 1738)
        ));
    }

    #[test]
    fn a_normal_window_does_not() {
        assert!(!fills_work_area(
            &area(),
            PhysicalPosition::new(200, 200),
            PhysicalSize::new(800, 600)
        ));
        // Full width but not full height (e.g. snapped to the top half).
        assert!(!fills_work_area(
            &area(),
            PhysicalPosition::new(0, 50),
            PhysicalSize::new(2880, 875)
        ));
    }
}
