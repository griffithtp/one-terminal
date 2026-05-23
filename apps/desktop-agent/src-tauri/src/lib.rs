mod app_dir;
mod broker;
mod config;
mod dacp;
mod engines;
mod fdc3_bus;
mod tcp;

use app_dir::{AppDirectoryCache, AppRecord};
use broker::{
    new_uuid, ChannelInfo, ChannelManager, IntentHandlerInfo, IntentRegistry, WindowHandle,
    WindowManager,
};
use config::AgentConfig;
use dacp::{PeerInfo, PeerRegistry};
use engines::{EngineCatalogClient, EngineRuntimeStore};
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State};

// ── Tauri commands ─────────────────────────────────────────────────────────────

/// Return all currently connected spoke instances.
#[tauri::command]
fn cda_list_connections(window_manager: State<WindowManager>) -> Vec<WindowHandle> {
    window_manager.list_all()
}

/// Return all channels with their member lists.
#[tauri::command]
fn cda_list_channels(channel_manager: State<ChannelManager>) -> Vec<ChannelInfo> {
    channel_manager.list_all()
}

/// Return the channel a specific spoke is currently joined to, if any.
#[tauri::command]
fn cda_get_spoke_channel(
    instance_id: String,
    window_manager: State<WindowManager>,
) -> Option<String> {
    window_manager
        .get(&instance_id)
        .and_then(|h| h.current_channel)
}

/// Force-disconnect a spoke.  Cleans up all registries and drops the TCP sender.
#[tauri::command]
fn cda_disconnect_spoke(
    instance_id: String,
    window_manager: State<WindowManager>,
    channel_manager: State<ChannelManager>,
    intent_registry: State<IntentRegistry>,
) -> bool {
    intent_registry.remove_all(&instance_id);
    channel_manager.remove_member(&instance_id);
    window_manager.close(&instance_id)
}

/// Broadcast a context from the CDA dashboard itself (useful for testing).
/// Returns the total number of recipients reached (local spokes + bridge peers).
#[tauri::command]
fn cda_broadcast_from_dashboard(
    channel_id: String,
    context: serde_json::Value,
    window_manager: State<WindowManager>,
    channel_manager: State<ChannelManager>,
    peer_registry: State<PeerRegistry>,
    app: AppHandle,
) -> Result<usize, String> {
    use broker::{CdaContextEvent, CdaResponse};

    let context_type = context
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    channel_manager.set_last_context(&channel_id, &context_type, context.clone());

    let targets = channel_manager.members_except(&channel_id, "__dashboard__");
    let payload = serde_json::to_string(&CdaResponse::ContextBroadcast {
        channel_id: channel_id.clone(),
        context: context.clone(),
        source_instance_id: "__dashboard__".to_string(),
        source_app_id: "desktop-agent".to_string(),
    })
    .map_err(|e| e.to_string())?;

    let mut count = 0usize;
    for target_id in &targets {
        if window_manager.send_to(target_id, &payload) {
            count += 1;
        }
    }

    count += peer_registry.forward_broadcast(&channel_id, &context, "desktop-agent");

    let _ = tauri::Emitter::emit(
        &app,
        "cda:context",
        CdaContextEvent {
            channel_id,
            context,
            source_instance_id: "__dashboard__".to_string(),
            source_app_id: "desktop-agent".to_string(),
            recipient_count: count,
        },
    );

    Ok(count)
}

/// Return all currently connected external bridge peers.
#[tauri::command]
fn cda_list_peers(peer_registry: State<PeerRegistry>) -> Vec<PeerInfo> {
    peer_registry.list_all()
}

/// Return every intent handler visible to the CDA:
///   - Local handlers from the `IntentRegistry` (spoke apps).
///   - Remote handlers from each connected peer's intent-listener set.
#[tauri::command]
fn cda_list_intent_handlers(
    intent_registry: State<IntentRegistry>,
    window_manager: State<WindowManager>,
    peer_registry: State<PeerRegistry>,
) -> Vec<IntentHandlerInfo> {
    let mut handlers = Vec::new();

    for (intent, instance_ids) in intent_registry.list_all() {
        for iid in instance_ids {
            let (app_id, display_name) = window_manager
                .get(&iid)
                .map(|h| (h.app_id, h.display_name))
                .unwrap_or_else(|| (iid.clone(), None));
            handlers.push(IntentHandlerInfo {
                intent: intent.clone(),
                app_id,
                instance_id: iid,
                display_name,
                desktop_agent_id: None,
            });
        }
    }

    for peer in peer_registry.list_all() {
        for intent in peer.intent_listeners {
            handlers.push(IntentHandlerInfo {
                intent,
                app_id: peer.desktop_agent_id.clone(),
                instance_id: peer.desktop_agent_id.clone(),
                display_name: None,
                desktop_agent_id: Some(peer.desktop_agent_id.clone()),
            });
        }
    }

    handlers
}

/// Raise an intent from the CDA dashboard (3-tier local resolution).
#[tauri::command]
fn cda_raise_intent_from_dashboard(
    intent: String,
    context: serde_json::Value,
    target_instance_id: Option<String>,
    window_manager: State<WindowManager>,
    intent_registry: State<IntentRegistry>,
    peer_registry: State<PeerRegistry>,
    app: AppHandle,
) -> Result<String, String> {
    use broker::{CdaIntentEvent, CdaResponse};

    let request_id = new_uuid();
    let source = "__dashboard__";

    let resolved = match &target_instance_id {
        Some(tid) => {
            if window_manager.get(tid).is_some() {
                Some(tid.clone())
            } else {
                None
            }
        }
        None => intent_registry.find_handler(&intent),
    };

    match resolved {
        Some(target_id) => {
            let handler = window_manager
                .get(&target_id)
                .ok_or_else(|| format!("Instance '{target_id}' not found"))?;

            let delivery = serde_json::to_string(&CdaResponse::IntentDelivery {
                intent: intent.clone(),
                context,
                source_instance_id: source.to_string(),
                request_id: request_id.clone(),
            })
            .map_err(|e| e.to_string())?;

            window_manager.send_to(&target_id, &delivery);

            let _ = tauri::Emitter::emit(
                &app,
                "cda:intent",
                CdaIntentEvent {
                    intent,
                    source_instance_id: source.to_string(),
                    handler_instance_id: target_id,
                    request_id: request_id.clone(),
                },
            );

            drop(handler);
            Ok(request_id)
        }

        None => {
            if peer_registry.forward_raise_intent(&intent, &context, source, &request_id) {
                Ok(request_id)
            } else {
                Err(format!("No handler found for intent '{intent}'"))
            }
        }
    }
}

// ── App Directory commands ─────────────────────────────────────────────────────

/// Spawn a fresh Terminal (one-terminal) window with no prior session restored.
#[tauri::command]
fn cda_open_terminal() {
    spawn_wm_instance_fresh();
}

/// Quit the Desktop Agent, preserving all Terminal state on disk.
/// One-terminal's auto-save has already persisted the current layout, so this
/// simply exits the process.
#[tauri::command]
fn cda_quit(app: AppHandle) {
    app.exit(0);
}

/// Quit the Desktop Agent and discard all Terminal state files so nothing is
/// restored on the next launch.  Deletes every directory under the
/// `com.one-terminal.one-terminal/terminals/` data folder, then exits.
#[tauri::command]
fn cda_quit_discard(app: AppHandle) -> Result<(), String> {
    // Derive one-terminal's data directory by stepping one level above the
    // Desktop Agent's own app_data_dir, then descending into the Window Manager
    // identifier.  Works on all platforms without hard-coding OS paths.
    let da_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("cannot resolve data dir: {e}"))?;
    if let Some(base) = da_data.parent() {
        let wm_terminals = base.join("com.one-terminal.one-terminal").join("terminals");
        if wm_terminals.exists() {
            std::fs::remove_dir_all(&wm_terminals)
                .map_err(|e| format!("failed to clear terminal state: {e}"))?;
        }
    }
    app.exit(0);
    Ok(())
}

/// Return the configured App Directory URL (respects OT_APP_DIR_URL override).
#[tauri::command]
fn cda_get_app_dir_url(cfg: State<'_, AgentConfig>) -> String {
    cfg.app_directory_url.clone()
}

/// Open the App Directory management UI in the system browser.
#[tauri::command]
fn cda_open_app_directory(cfg: State<'_, AgentConfig>) -> Result<(), String> {
    tauri_plugin_opener::open_url(&cfg.app_directory_url, None::<&str>)
        .map_err(|e| format!("Failed to open App Directory: {e}"))
}

/// Return the current in-memory App Directory snapshot.
#[tauri::command]
async fn cda_list_app_directory(
    app_dir_cache: State<'_, AppDirectoryCache>,
) -> Result<Vec<AppRecord>, String> {
    Ok(app_dir_cache.list_all().await)
}

/// Trigger a fresh fetch from the App Directory service and return the count.
/// Also rebuilds the tray "Launch App" submenu so it reflects the updated list.
#[tauri::command]
async fn cda_refresh_app_directory(
    app_dir_cache: State<'_, AppDirectoryCache>,
    app: AppHandle,
) -> Result<usize, String> {
    let count = app_dir_cache.refresh().await?;
    if let Ok(menu) = build_tray_menu(&app).await {
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_menu(Some(menu));
        }
    }
    Ok(count)
}

/// Open an app from the App Directory.
/// `onlineNative` apps open in a Tauri WebView container (respecting any
/// engine binding declared in the app's directory record); all other types
/// use the system browser.
/// `engine_override` is debug-only: in release builds it is silently ignored
/// so dev-only UI state can't influence production launch decisions.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn cda_launch_app(
    app_id: String,
    app_url: String,
    app_type: String,
    app_title: Option<String>,
    engine_override: Option<engines::EngineBinding>,
    app_dir_cache: State<'_, AppDirectoryCache>,
    engine_catalog: State<'_, EngineCatalogClient>,
    engine_store: State<'_, EngineRuntimeStore>,
    app: AppHandle,
) -> Result<(), String> {
    use engines::router::{plan_launch, resolve_binding, LaunchContext};

    if app_type != "onlineNative" {
        return tauri_plugin_opener::open_url(app_url, None::<&str>)
            .map_err(|e| format!("Failed to launch app: {e}"));
    }

    // Focus an existing standalone window for this app if we already opened one.
    let label = format!(
        "app-{}",
        app_id.replace(|c: char| !c.is_alphanumeric() && c != '-', "-")
    );
    if let Some(win) = app.get_webview_window(&label) {
        win.set_focus()
            .map_err(|e| format!("Failed to focus window: {e}"))?;
        return Ok(());
    }

    // Prefer the cached App Directory record (it has engine_bindings); fall
    // back to a synthesized record from the UI's args so a missing cache
    // entry doesn't block launch.
    let records = app_dir_cache.list_all().await;
    let appd_owned;
    let appd: &AppRecord = match records.iter().find(|r| r.app_id == app_id) {
        Some(r) => r,
        None => {
            appd_owned = AppRecord {
                app_id: app_id.clone(),
                name: app_title.clone().unwrap_or_else(|| app_id.clone()),
                app_type: app_type.clone(),
                title: app_title.clone(),
                description: None,
                version: None,
                details: Some(app_dir::AppDetails {
                    url: Some(app_url.clone()),
                }),
                categories: Vec::new(),
                interop: None,
                engine_bindings: None,
            };
            &appd_owned
        }
    };

    // Debug-only override > AppD binding > default.
    #[cfg(debug_assertions)]
    let effective_binding = engine_override.or_else(|| resolve_binding(appd));
    #[cfg(not(debug_assertions))]
    let effective_binding = {
        let _ = engine_override;
        resolve_binding(appd)
    };

    // Resolve → ensure → plan.
    let installed = if let Some(binding) = effective_binding {
        let snapshot = engine_catalog.snapshot().await;
        Some(engine_store.ensure(&binding, &snapshot, &app).await?)
    } else {
        None
    };

    let manifests = app.state::<EngineRuntimeStore>().plugins().to_vec();
    let plan = plan_launch(
        appd,
        &LaunchContext::Standalone,
        installed.as_ref(),
        &manifests,
    )?;
    execute_plan(&app, &label, plan).await
}

/// Carry out a [`LaunchPlan`]: open in-process, spawn the pinned-WebView2
/// host, or spawn the Electron host (latter is Phase 5 — not installed yet).
async fn execute_plan(
    app: &AppHandle,
    label: &str,
    plan: engines::router::LaunchPlan,
) -> Result<(), String> {
    use engines::router::LaunchPlan;

    match plan {
        LaunchPlan::InProcessWebview { url, title } => {
            let parsed: tauri::Url = url.parse().map_err(|e| format!("Invalid URL: {e}"))?;
            tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::External(parsed))
                .title(title)
                .inner_size(1280.0, 800.0)
                .build()
                .map_err(|e| format!("Failed to open WebView: {e}"))?;
            Ok(())
        }
        LaunchPlan::SpawnTauriHost {
            runtime_folder,
            url,
            title,
        } => {
            let host_bin = locate_host_binary()?;
            let mut cmd = std::process::Command::new(&host_bin);
            cmd.env("OT_APP_ID", label)
                .env("OT_URL", &url)
                .env("OT_TITLE", &title);
            if let Some(folder) = runtime_folder {
                cmd.env("WEBVIEW2_BROWSER_EXECUTABLE_FOLDER", folder);
            }
            cmd.spawn()
                .map(|_| ())
                .map_err(|e| format!("Failed to spawn {}: {e}", host_bin.display()))
        }
        LaunchPlan::SpawnElectronHost {
            binding,
            url,
            title,
        } => {
            // The shared launcher locates the electron-host shell folder
            // (OT_ELECTRON_HOST_OVERRIDE in dev, sibling-of-exe in release)
            // and the Electron binary (OT_ELECTRON_BIN, catalog cache, or
            // the shell's own node_modules from `npm install`).
            let cache_root = ot_core::engine::default_cache_root();
            ot_core::electron_host::spawn_electron_app(&binding, &cache_root, label, &url, &title)
        }
        LaunchPlan::SpawnBinary {
            binary_name,
            env_vars,
        } => {
            let mut cmd = std::process::Command::new(&binary_name);
            for (k, v) in env_vars {
                cmd.env(k, v);
            }
            cmd.spawn()
                .map(|_| ())
                .map_err(|e| format!("Failed to spawn plugin binary '{binary_name}': {e}"))
        }
    }
}

/// Find the `tauri-webview-host` binary. Honors `OT_HOST_BINARY` for dev
/// overrides; otherwise looks next to the currently-running desktop-agent binary.
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

/// Find the `one-terminal` binary. Honors `OT_WM_BINARY` for dev
/// overrides; otherwise looks next to the currently-running desktop-agent binary.
fn locate_wm_binary() -> Result<std::path::PathBuf, String> {
    if let Ok(path) = std::env::var("OT_WM_BINARY") {
        return Ok(std::path::PathBuf::from(path));
    }
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "exe has no parent dir".to_string())?;
    let name = if cfg!(windows) {
        "one-terminal.exe"
    } else {
        "one-terminal"
    };
    let candidate = dir.join(name);
    if !candidate.exists() {
        return Err(format!(
            "one-terminal not found at {} — build it (`cargo build -p one-terminal`) or set OT_WM_BINARY",
            candidate.display()
        ));
    }
    Ok(candidate)
}

/// Spawn a Window Manager instance that restores previously saved terminals.
/// Called once at Desktop Agent startup.
fn spawn_wm_instance() {
    match locate_wm_binary() {
        Ok(bin) => {
            if let Err(e) = std::process::Command::new(&bin).spawn() {
                eprintln!("[cda/tray] Failed to spawn window-manager: {e}");
            }
        }
        Err(e) => eprintln!("[cda/tray] {e}"),
    }
}

/// Spawn a Window Manager instance that starts fresh with zero dashboards.
/// Called from the tray "Open New Terminal" action so the user always gets a
/// clean slate rather than a restoration of a previous session.
fn spawn_wm_instance_fresh() {
    match locate_wm_binary() {
        Ok(bin) => {
            if let Err(e) = std::process::Command::new(&bin)
                .env("OT_FRESH_START", "1")
                .spawn()
            {
                eprintln!("[cda/tray] Failed to spawn fresh window-manager: {e}");
            }
        }
        Err(e) => eprintln!("[cda/tray] {e}"),
    }
}

// ── Tray helpers ───────────────────────────────────────────────────────────────

/// Build the full tray context menu from the current App Directory cache snapshot.
async fn build_tray_menu(app: &AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    let apps = app.state::<AppDirectoryCache>().list_all().await;

    let open_item = MenuItemBuilder::with_id("open", "Open CDA").build(app)?;
    let terminal_item = MenuItemBuilder::with_id("terminal", "Open New Terminal").build(app)?;
    let exit_item = MenuItemBuilder::with_id("exit", "Exit").build(app)?;

    let mut sub = SubmenuBuilder::new(app, "Launch App");
    if apps.is_empty() {
        let placeholder = MenuItemBuilder::with_id("no-apps", "No apps available")
            .enabled(false)
            .build(app)?;
        sub = sub.item(&placeholder);
    } else {
        for record in &apps {
            let label = record.title.as_deref().unwrap_or(&record.name);
            let item =
                MenuItemBuilder::with_id(format!("launch:{}", record.app_id), label).build(app)?;
            sub = sub.item(&item);
        }
    }
    let apps_sub = sub.build()?;

    MenuBuilder::new(app)
        .item(&open_item)
        .item(&terminal_item)
        .item(&apps_sub)
        .separator()
        .item(&exit_item)
        .build()
}

/// Launch an app by its App Directory ID, reusing an existing window if one is open.
async fn launch_app_from_tray(app: AppHandle, app_id: String) {
    use engines::router::{plan_launch, resolve_binding, LaunchContext};

    let apps = app.state::<AppDirectoryCache>().list_all().await;
    let Some(record) = apps.into_iter().find(|r| r.app_id == app_id) else {
        eprintln!("[cda/tray] App '{app_id}' not found in directory");
        return;
    };

    let label = format!(
        "app-{}",
        app_id.replace(|c: char| !c.is_alphanumeric() && c != '-', "-"),
    );
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.set_focus();
        return;
    }

    let effective_binding = resolve_binding(&record);
    let installed = if let Some(binding) = effective_binding {
        let snapshot = app.state::<EngineCatalogClient>().snapshot().await;
        match app
            .state::<EngineRuntimeStore>()
            .ensure(&binding, &snapshot, &app)
            .await
        {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("[cda/tray] Engine ensure failed for '{app_id}': {e}");
                return;
            }
        }
    } else {
        None
    };

    let manifests = app.state::<EngineRuntimeStore>().plugins().to_vec();
    match plan_launch(
        &record,
        &LaunchContext::Standalone,
        installed.as_ref(),
        &manifests,
    ) {
        Ok(plan) => {
            if let Err(e) = execute_plan(&app, &label, plan).await {
                eprintln!("[cda/tray] Launch failed for '{app_id}': {e}");
            }
        }
        Err(e) => eprintln!("[cda/tray] Plan failed for '{app_id}': {e}"),
    }
}

// ── App entry point ────────────────────────────────────────────────────────────

/// When `OT_ELECTRON_HOST_OVERRIDE` is set, probe both the shell directory and
/// the Electron binary so the user gets an actionable warning at startup
/// instead of a cryptic error when they first try to launch an Electron app.
fn validate_electron_override() {
    let Ok(raw) = std::env::var("OT_ELECTRON_HOST_OVERRIDE") else {
        return;
    };

    // ── Shell directory check ──────────────────────────────────────────────
    let shell = {
        let path = std::path::PathBuf::from(&raw);
        if path.is_dir() && path.join("main.js").is_file() {
            path
        } else {
            eprintln!(
                "[desktop-agent] warning: OT_ELECTRON_HOST_OVERRIDE={raw} is not a valid \
                 electron-host directory (expected main.js inside)\n  \
                 Fix: run `npm run setup:electron-host` from the workspace root, then retry."
            );
            return;
        }
    };

    // ── Electron binary check ──────────────────────────────────────────────
    let cache_root = ot_core::engine::default_cache_root();
    let probe = ot_core::engine::EngineBinding {
        family: ot_core::engine::EngineFamily::Electron,
        version: "system".to_string(),
    };
    if let Err(msg) = ot_core::electron_host::locate_electron_binary(&probe, &cache_root, &shell) {
        eprintln!("[desktop-agent] warning: Electron binary not available — {msg}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let cfg = AgentConfig::load();

    if let Err(msg) = cfg.ports.check_availability() {
        eprintln!("[desktop-agent] {msg}");
        std::process::exit(1);
    }

    validate_electron_override();

    let app_dir_cache = AppDirectoryCache::new(&cfg.app_directory_url);
    let engine_catalog = EngineCatalogClient::new(&cfg.engine_catalog_url);
    let channel_manager = ChannelManager::new();

    // Seed user channels from config.
    for ch in &cfg.user_channels {
        channel_manager.create(&ch.id, &ch.display_name, &ch.color);
    }

    tauri::Builder::default()
        .manage(WindowManager::new())
        .manage(channel_manager)
        .manage(IntentRegistry::new())
        .manage(PeerRegistry::new())
        .manage(app_dir_cache.clone())
        .manage(engine_catalog.clone())
        .manage(cfg.clone())
        .setup(|app| {
            let wm = app.state::<WindowManager>().inner().clone();
            let cm = app.state::<ChannelManager>().inner().clone();
            let ir = app.state::<IntentRegistry>().inner().clone();
            let pr = app.state::<PeerRegistry>().inner().clone();
            let adc = app.state::<AppDirectoryCache>().inner().clone();
            let app_handle = app.handle().clone();
            let ports = app.state::<AgentConfig>().ports.clone();

            // Engine runtime cache lives at the shared cross-process location
            // so the one-terminal can find runtimes installed by desktop-agent.
            let engine_root = ot_core::engine::default_cache_root();
            std::fs::create_dir_all(&engine_root)
                .map_err(|e| format!("create engine root {}: {e}", engine_root.display()))?;
            app.manage(EngineRuntimeStore::new(engine_root));

            let ec = app.state::<EngineCatalogClient>().inner().clone();

            // TCP loopback server for spoke apps.
            tauri::async_runtime::spawn(tcp::server::start(
                ports.tcp_broker,
                wm.clone(),
                cm.clone(),
                ir.clone(),
                pr.clone(),
                adc.clone(),
                app_handle.clone(),
            ));

            // FDC3 Bus WebSocket server for browser apps.
            tauri::async_runtime::spawn(fdc3_bus::start(
                ports.fdc3_bus,
                wm,
                cm,
                ir,
                pr.clone(),
                adc.clone(),
                app_handle.clone(),
            ));

            // DACP subsystem: discovery, inbound WS server, file announce.
            let dacp_instance_id = new_uuid();
            let app_handle_tray = app_handle.clone();
            tauri::async_runtime::spawn(dacp::start(
                pr,
                app_handle,
                dacp_instance_id,
                ports.dacp_bridge,
            ));

            // Initial App Directory fetch (non-fatal if the service is not running).
            tauri::async_runtime::spawn(async move {
                match adc.refresh().await {
                    Ok(n) => {
                        println!("[cda] App Directory: {n} app(s) loaded at startup");
                        if let Ok(menu) = build_tray_menu(&app_handle_tray).await {
                            if let Some(tray) = app_handle_tray.tray_by_id("main") {
                                let _ = tray.set_menu(Some(menu));
                            }
                        }
                    }
                    Err(e) => eprintln!("[cda] App Directory unavailable at startup: {e}"),
                }
            });

            // Initial engine catalog fetch (same non-fatal policy).
            tauri::async_runtime::spawn(async move {
                match ec.refresh().await {
                    Ok(n) => println!("[cda] Engine catalog: {n} entry/entries loaded at startup"),
                    Err(e) => eprintln!("[cda] Engine catalog unavailable at startup: {e}"),
                }
            });

            // ── Auto-launch one-terminal ───────────────────────────────────────
            // Restore previously open Terminal windows. one-terminal's own startup
            // handles restoring all saved non-main terminals via load_persisted_terminals.
            spawn_wm_instance();

            // ── System tray ────────────────────────────────────────────────────
            // Initial menu: apps list is empty until the App Directory fetch completes.
            let open_item = MenuItemBuilder::with_id("open", "Open CDA").build(app)?;
            let terminal_item =
                MenuItemBuilder::with_id("terminal", "Open New Terminal").build(app)?;
            let exit_item = MenuItemBuilder::with_id("exit", "Exit").build(app)?;
            let placeholder = MenuItemBuilder::with_id("no-apps", "No apps available")
                .enabled(false)
                .build(app)?;
            let apps_sub = SubmenuBuilder::new(app, "Launch App")
                .item(&placeholder)
                .build()?;
            let init_menu = MenuBuilder::new(app)
                .item(&open_item)
                .item(&terminal_item)
                .item(&apps_sub)
                .separator()
                .item(&exit_item)
                .build()?;

            let _tray = TrayIconBuilder::with_id("main")
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(true)
                .menu(&init_menu)
                .tooltip("OneTerminal Desktop Agent")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("desktop-agent-dashboard") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "terminal" => {
                        spawn_wm_instance_fresh();
                    }
                    "exit" => {
                        // Show the dashboard window and ask the user whether to
                        // save or discard Terminal state before quitting.
                        if let Some(w) =
                            app.get_webview_window("desktop-agent-dashboard")
                        {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                            let _ = tauri::Emitter::emit(app, "cda:quit-requested", ());
                        } else {
                            // No dashboard window (shouldn't happen), fall back.
                            app.exit(0);
                        }
                    }
                    id if id.starts_with("launch:") => {
                        let app_id = id.trim_start_matches("launch:").to_string();
                        let app_clone = app.clone();
                        tauri::async_runtime::spawn(launch_app_from_tray(app_clone, app_id));
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("desktop-agent-dashboard") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "desktop-agent-dashboard" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            cda_list_connections,
            cda_list_channels,
            cda_get_spoke_channel,
            cda_disconnect_spoke,
            cda_broadcast_from_dashboard,
            cda_list_peers,
            cda_list_intent_handlers,
            cda_raise_intent_from_dashboard,
            cda_open_terminal,
            cda_get_app_dir_url,
            cda_open_app_directory,
            cda_quit,
            cda_quit_discard,
            cda_list_app_directory,
            cda_refresh_app_directory,
            cda_launch_app,
            engines::commands::engine_catalog,
            engines::commands::engine_refresh_catalog,
            engines::commands::engine_list_installed,
            engines::commands::engine_ensure,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OneTerminal Desktop Agent");
}
