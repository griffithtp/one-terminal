//! FDC3 Desktop Agent selection. User-facing docs: `docs/fdc3.md`.
//!
//! Decides which Desktop Agent this Terminal process's widgets connect to,
//! and lets the user switch at runtime. The choice is expressed to widgets
//! as `window.OT_FDC3_AGENT_URL`, injected into every panel webview by
//! [`panel_init_script`].
//!
//! Precedence (first match wins):
//!   1. `OT_FDC3_AGENT_URL` env var — forced; the UI shows it as locked
//!   2. The user's saved choice (`<app_data_dir>/agent-selection.json`),
//!      unless `fdc3.allowUserOverride` is `false`
//!   3. `fdc3.defaultAgent`
//!   4. Legacy `fdc3.agentUrl`
//!   5. The first configured agent, else the built-in `local` agent
//!      (fdc3-plugin's own default chain)
//!
//! The saved choice is shared by every Terminal process for this user. It is
//! read once at startup; a running process only changes agent when the user
//! switches in it.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, WebviewUrl};

use crate::config::{AgentProfile, Fdc3Config};
use crate::terminal::{TerminalManager, TerminalState};

/// fdc3-plugin's fallback when no agent URL is injected.
const PLUGIN_DEFAULT_URL: &str = "ws://localhost:7891/fdc3";
const SELECTION_FILE: &str = "agent-selection.json";
const ENV_VAR: &str = "OT_FDC3_AGENT_URL";
const ONET_KIND: &str = "onet";

/// Built-in entry: fdc3-plugin's default chain (nothing injected).
pub const LOCAL_ID: &str = "local";
/// Built-in entry: the legacy `fdc3.agentUrl`.
pub const CONFIG_ID: &str = "config";
/// Built-in entry: FDC3 disabled.
pub const NONE_ID: &str = "none";
/// Synthetic entry when `OT_FDC3_AGENT_URL` forces the agent.
pub const ENV_ID: &str = "env";

// ── Types ─────────────────────────────────────────────────────────────────────

/// Where the active selection came from.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SelectionSource {
    Env,
    User,
    Config,
    Builtin,
}

impl SelectionSource {
    fn label(self) -> &'static str {
        match self {
            Self::Env => "env OT_FDC3_AGENT_URL",
            Self::User => "user choice",
            Self::Config => "terminal.config.json",
            Self::Builtin => "built-in default",
        }
    }
}

/// One row in the Desktop Agent picker.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentEntry {
    pub id: String,
    pub name: String,
    pub env: Option<String>,
    pub kind: String,
    /// Endpoint shown to the user and probed for reachability. Empty for
    /// the `none` entry.
    pub url: String,
    /// `false` for connector kinds that don't exist yet (see "Connector
    /// kinds" in `docs/fdc3.md`).
    pub supported: bool,
    /// One of the built-in entries rather than a configured agent.
    pub builtin: bool,
}

/// Snapshot returned to the frontend by `wm_agent_list`.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentListing {
    pub agents: Vec<AgentEntry>,
    pub selected_id: String,
    pub default_id: String,
    pub source: SelectionSource,
    /// Forced by `OT_FDC3_AGENT_URL` — the picker is read-only.
    pub locked: bool,
    pub allow_override: bool,
    /// One-off message for the user, e.g. a saved agent that was removed
    /// from config.
    pub notice: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedSelection {
    agent_id: String,
}

// ── Resolution (pure, unit-tested) ────────────────────────────────────────────

fn build_agents(cfg: &Fdc3Config) -> Vec<AgentEntry> {
    let mut agents: Vec<AgentEntry> = Vec::new();
    for p in &cfg.agents {
        let AgentProfile {
            id,
            name,
            env,
            kind,
            url,
        } = p;
        if [LOCAL_ID, CONFIG_ID, NONE_ID, ENV_ID].contains(&id.as_str()) {
            eprintln!("[agent] fdc3.agents: id '{id}' is reserved — skipped");
            continue;
        }
        if agents.iter().any(|a| &a.id == id) {
            eprintln!("[agent] fdc3.agents: duplicate id '{id}' — skipped");
            continue;
        }
        if url.trim().is_empty() {
            eprintln!("[agent] fdc3.agents: '{id}' has no url — skipped");
            continue;
        }
        agents.push(AgentEntry {
            id: id.clone(),
            name: name.clone(),
            env: env.clone(),
            kind: kind.clone(),
            url: url.clone(),
            supported: kind == ONET_KIND,
            builtin: false,
        });
    }
    if let Some(url) = cfg.agent_url.as_deref().filter(|u| !u.is_empty()) {
        if !agents.iter().any(|a| a.url == url) {
            agents.push(builtin(CONFIG_ID, "Configured agent", url));
        }
    }
    if !agents.iter().any(|a| a.url == PLUGIN_DEFAULT_URL) {
        agents.push(builtin(LOCAL_ID, "Local Desktop Agent", PLUGIN_DEFAULT_URL));
    }
    agents.push(builtin(NONE_ID, "No agent (FDC3 off)", ""));
    agents
}

fn builtin(id: &str, name: &str, url: &str) -> AgentEntry {
    AgentEntry {
        id: id.into(),
        name: name.into(),
        env: None,
        kind: ONET_KIND.into(),
        url: url.into(),
        supported: true,
        builtin: true,
    }
}

/// The agent used when the user hasn't chosen one, and where that came from.
fn default_selection(cfg: &Fdc3Config, agents: &[AgentEntry]) -> (String, SelectionSource) {
    if let Some(id) = &cfg.default_agent {
        match agents.iter().find(|a| &a.id == id) {
            Some(a) if a.supported => return (a.id.clone(), SelectionSource::Config),
            Some(_) => eprintln!(
                "[agent] fdc3.defaultAgent '{id}' uses an unsupported connector — ignored"
            ),
            None => eprintln!("[agent] fdc3.defaultAgent '{id}' is not in fdc3.agents — ignored"),
        }
    }
    match cfg.agent_url.as_deref() {
        Some("") => return (NONE_ID.into(), SelectionSource::Config),
        Some(url) => {
            if let Some(a) = agents.iter().find(|a| a.url == url) {
                return (a.id.clone(), SelectionSource::Config);
            }
        }
        None => {}
    }
    if let Some(a) = agents.iter().find(|a| !a.builtin && a.supported) {
        return (a.id.clone(), SelectionSource::Config);
    }
    let local = agents
        .iter()
        .find(|a| a.url == PLUGIN_DEFAULT_URL)
        .map(|a| a.id.clone())
        .unwrap_or_else(|| LOCAL_ID.into());
    (local, SelectionSource::Builtin)
}

#[derive(Clone, Debug)]
struct Resolved {
    agents: Vec<AgentEntry>,
    selected_id: String,
    default_id: String,
    default_source: SelectionSource,
    source: SelectionSource,
    env_url: Option<String>,
    allow_override: bool,
    notice: Option<String>,
    /// The saved choice no longer matches a usable agent and should be
    /// deleted so the notice only shows once.
    discard_saved: bool,
}

fn resolve(cfg: &Fdc3Config, env_url: Option<String>, saved_id: Option<String>) -> Resolved {
    let mut agents = build_agents(cfg);
    let (default_id, default_source) = default_selection(cfg, &agents);
    let mut r = Resolved {
        selected_id: default_id.clone(),
        source: default_source,
        default_id,
        default_source,
        env_url: None,
        allow_override: cfg.allow_user_override,
        notice: None,
        discard_saved: false,
        agents: Vec::new(),
    };

    if let Some(url) = env_url {
        agents.insert(
            0,
            AgentEntry {
                name: if url.is_empty() {
                    "FDC3 off (OT_FDC3_AGENT_URL)".into()
                } else {
                    "Set by OT_FDC3_AGENT_URL".into()
                },
                ..builtin(ENV_ID, "", &url)
            },
        );
        r.selected_id = ENV_ID.into();
        r.source = SelectionSource::Env;
        r.env_url = Some(url);
    } else if let Some(id) = saved_id.filter(|_| cfg.allow_user_override) {
        match agents.iter().find(|a| a.id == id) {
            Some(a) if a.supported => {
                r.selected_id = a.id.clone();
                r.source = SelectionSource::User;
            }
            _ => {
                let default_name = agents
                    .iter()
                    .find(|a| a.id == r.default_id)
                    .map(|a| a.name.as_str())
                    .unwrap_or(r.default_id.as_str());
                r.notice = Some(format!(
                    "Your saved Desktop Agent '{id}' is no longer available, so the default \
                     '{default_name}' is being used."
                ));
                r.discard_saved = true;
            }
        }
    }
    r.agents = agents;
    r
}

impl Resolved {
    fn selected(&self) -> Option<&AgentEntry> {
        self.agents.iter().find(|a| a.id == self.selected_id)
    }

    fn init_script(&self) -> String {
        let url = match self.selected() {
            // The built-in local agent injects nothing so fdc3-plugin's own
            // chain (page meta tag → OT_FDC3_BUS_URL → localhost:7891) still
            // applies, exactly as before agent selection existed.
            Some(a) if a.id == LOCAL_ID => return String::new(),
            Some(a) => a.url.clone(),
            None => return String::new(),
        };
        let encoded = serde_json::to_string(&url).unwrap_or_else(|_| "\"\"".to_string());
        format!("window.OT_FDC3_AGENT_URL = {encoded};")
    }

    fn listing(&self) -> AgentListing {
        AgentListing {
            agents: self.agents.clone(),
            selected_id: self.selected_id.clone(),
            default_id: self.default_id.clone(),
            source: self.source,
            locked: self.env_url.is_some(),
            allow_override: self.allow_override,
            notice: self.notice.clone(),
        }
    }

    /// Apply a user selection. `None` resets to the default. Returns whether
    /// the injected script changed (i.e. webviews must be recreated).
    fn select(&mut self, id: Option<&str>) -> Result<bool, String> {
        if self.env_url.is_some() {
            return Err(format!(
                "The Desktop Agent is set by {ENV_VAR} and can't be changed here."
            ));
        }
        if !self.allow_override {
            return Err("Choosing a Desktop Agent is disabled by your administrator.".into());
        }
        let before = self.init_script();
        match id {
            None => {
                self.selected_id = self.default_id.clone();
                self.source = self.default_source;
            }
            Some(id) => {
                let agent = self
                    .agents
                    .iter()
                    .find(|a| a.id == id)
                    .ok_or_else(|| format!("Unknown Desktop Agent '{id}'."))?;
                if !agent.supported {
                    return Err(format!(
                        "'{}' needs a '{}' connector, which isn't available yet.",
                        agent.name, agent.kind
                    ));
                }
                self.selected_id = agent.id.clone();
                self.source = SelectionSource::User;
            }
        }
        self.notice = None;
        Ok(self.init_script() != before)
    }
}

// ── Managed state ─────────────────────────────────────────────────────────────

/// Tauri-managed agent selection. Cheap to clone; all clones share state.
#[derive(Clone)]
pub struct AgentSelection {
    inner: Arc<RwLock<Resolved>>,
    saved_path: Option<PathBuf>,
}

impl AgentSelection {
    /// Resolve the startup selection from config, env and the saved choice.
    pub fn load(cfg: &Fdc3Config, data_dir: Option<&Path>) -> Self {
        let saved_path = data_dir.map(|d| d.join(SELECTION_FILE));
        let saved_id = saved_path.as_deref().and_then(read_saved);
        let resolved = resolve(cfg, std::env::var(ENV_VAR).ok(), saved_id);
        if resolved.discard_saved {
            if let Some(p) = &saved_path {
                let _ = std::fs::remove_file(p);
            }
        }
        if let Some(n) = &resolved.notice {
            eprintln!("[agent] {n}");
        }
        let sel = Self {
            inner: Arc::new(RwLock::new(resolved)),
            saved_path,
        };
        sel.log();
        sel
    }

    fn log(&self) {
        let r = self.inner.read().unwrap();
        match r.selected() {
            Some(a) if a.id == LOCAL_ID => println!(
                "[agent] Desktop Agent: {} = {PLUGIN_DEFAULT_URL} (fdc3-plugin default; source: {})",
                a.name,
                r.source.label()
            ),
            Some(a) if a.url.is_empty() => eprintln!(
                "[agent] Desktop Agent: none — FDC3 disabled for all widgets (source: {})",
                r.source.label()
            ),
            Some(a) => println!(
                "[agent] Desktop Agent: {} = {} (source: {})",
                a.name,
                a.url,
                r.source.label()
            ),
            None => {}
        }
    }

    pub fn init_script(&self) -> String {
        self.inner.read().unwrap().init_script()
    }

    pub fn listing(&self) -> AgentListing {
        self.inner.read().unwrap().listing()
    }

    /// Apply and persist a user selection. Returns whether webviews need to
    /// be recreated.
    fn select(&self, id: Option<&str>) -> Result<bool, String> {
        let changed = self.inner.write().unwrap().select(id)?;
        if let Some(path) = &self.saved_path {
            let result = match id {
                Some(id) => write_saved(path, id),
                None => match std::fs::remove_file(path) {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                    _ => Ok(()),
                },
            };
            if let Err(e) = result {
                eprintln!(
                    "[agent] could not save selection to {}: {e}",
                    path.display()
                );
            }
        }
        self.log();
        Ok(changed)
    }
}

fn read_saved(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<SavedSelection>(&text) {
        Ok(s) => Some(s.agent_id),
        Err(e) => {
            eprintln!("[agent] ignoring unreadable {}: {e}", path.display());
            None
        }
    }
}

fn write_saved(path: &Path, id: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(&SavedSelection {
        agent_id: id.to_string(),
    })?;
    std::fs::write(path, json)
}

/// The script injected into every panel webview at creation. Call at each
/// creation site so webviews created after a switch get the new agent.
///
/// Always defines `window.OT_FDC3_INITIAL_CHANNEL` (as `null`; restore sites
/// override it via `config::append_initial_channel`) so widgets can tell
/// they're hosted by a Terminal that manages their FDC3 channel — and must
/// not auto-join a channel of their own — by checking `!== undefined`.
pub fn panel_init_script(app: &AppHandle) -> String {
    format!(
        "window.OT_FDC3_INITIAL_CHANNEL = null;\n{}",
        app.state::<AgentSelection>().init_script()
    )
}

// ── Switching: recreate webviews in place ─────────────────────────────────────

/// Recreate every panel webview in `terminal` so it picks up the current
/// init script. Parked (kept-alive) and pooled webviews are closed instead —
/// parked ones are rebuilt when their Dashboard is next activated, and the
/// pool is replenished. Each panel is closed and re-added under the same
/// label in one main-thread closure (the same pattern `overlay_prewarm` uses
/// for the overlay).
fn recreate_webviews(terminal: &Arc<TerminalState>, app: &AppHandle) -> Result<(), String> {
    let script = panel_init_script(app);
    let panels = terminal.layout_tree.panels_for_restore();
    let mut to_close = terminal.layout_tree.take_parked_labels();
    to_close.extend(terminal.pool.drain());

    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let app_main = app.clone();
    let terminal_id = terminal.id.clone();
    let tree = terminal.layout_tree.clone();
    let base_script = script.clone();
    app.run_on_main_thread(move || {
        let result = (|| -> Result<(), String> {
            let win = app_main
                .get_window(&terminal_id)
                .ok_or_else(|| format!("window '{terminal_id}' not found"))?;
            for label in &to_close {
                if let Some(wv) = app_main.get_webview(label) {
                    wv.close().map_err(|e| e.to_string())?;
                }
            }
            for (label, url, channel) in &panels {
                let Ok(parsed) = url.parse::<tauri::Url>() else {
                    eprintln!("[agent] recreate '{label}': invalid url '{url}'");
                    continue;
                };
                if let Some(wv) = app_main.get_webview(label) {
                    wv.close().map_err(|e| e.to_string())?;
                }
                let script =
                    crate::config::append_initial_channel(&base_script, channel.as_deref());
                win.add_child(
                    WebviewBuilder::new(label, WebviewUrl::External(parsed))
                        .initialization_script(&script)
                        .on_navigation(crate::address_bar_navigation_handler(
                            app_main.clone(),
                            terminal_id.clone(),
                            label.clone(),
                            tree.clone(),
                        ))
                        .on_page_load(crate::channel_sync_page_load_handler(
                            label.clone(),
                            tree.clone(),
                        )),
                    LogicalPosition::new(0.0, 0.0),
                    LogicalSize::new(1.0, 1.0),
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })();
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())??;

    // New panel webviews sit above the overlay in z-order.
    terminal.overlay.lock().unwrap().stale = true;
    for _ in 0..terminal.pool.target_size {
        terminal.pool.replenish(
            app,
            &terminal.overlay,
            &terminal.id,
            &script,
            &terminal.layout_tree,
        );
    }
    terminal.layout_tree.reflow(app);
    terminal.layout_tree.emit_host(app);
    crate::overlay_prewarm_in_background(Arc::clone(&terminal.overlay), &terminal.id, app);
    Ok(())
}

// ── Commands ──────────────────────────────────────────────────────────────────

/// List the selectable agents and the current selection.
#[tauri::command]
pub fn wm_agent_list(agent: State<'_, AgentSelection>) -> AgentListing {
    agent.listing()
}

/// Switch agent (`id = null` resets to the default), persist the choice, and
/// recreate every panel webview in every Terminal window of this process.
/// Emits `wm:agent-changed` with the new listing.
#[tauri::command]
pub async fn wm_agent_select(
    id: Option<String>,
    agent: State<'_, AgentSelection>,
    manager: State<'_, TerminalManager>,
    app: AppHandle,
) -> Result<AgentListing, String> {
    let changed = agent.select(id.as_deref())?;
    if changed {
        for terminal in manager.list() {
            if let Err(e) = recreate_webviews(&terminal, &app) {
                eprintln!("[agent] '{}': recreating webviews failed: {e}", terminal.id);
            }
        }
    }
    let listing = agent.listing();
    let _ = app.emit("wm:agent-changed", &listing);
    Ok(listing)
}

/// Reachability check: can a TCP connection be opened to the URL's host and
/// port? Does not speak the FDC3 protocol, so a reachable endpoint may still
/// reject the handshake.
#[tauri::command]
pub async fn wm_agent_probe(url: String) -> bool {
    tauri::async_runtime::spawn_blocking(move || probe(&url))
        .await
        .unwrap_or(false)
}

fn probe(url: &str) -> bool {
    use std::net::{TcpStream, ToSocketAddrs};
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let (Some(host), Some(port)) = (parsed.host_str(), parsed.port_or_known_default()) else {
        return false;
    };
    let Ok(addrs) = (host, port).to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(800)).is_ok())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(id: &str, env: &str, kind: &str, url: &str) -> AgentProfile {
        AgentProfile {
            id: id.into(),
            name: id.to_uppercase(),
            env: Some(env.into()),
            kind: kind.into(),
            url: url.into(),
        }
    }

    fn cfg_with_agents() -> Fdc3Config {
        Fdc3Config {
            agents: vec![
                profile("uat", "uat", "onet", "ws://uat:7891/fdc3"),
                profile("prod", "prod", "onet", "wss://prod/fdc3"),
                profile("io", "prod", "interopio", "ws://io:8385"),
            ],
            default_agent: Some("uat".into()),
            ..Fdc3Config::default()
        }
    }

    #[test]
    fn no_config_uses_local_and_injects_nothing() {
        let r = resolve(&Fdc3Config::default(), None, None);
        assert_eq!(r.selected_id, LOCAL_ID);
        assert_eq!(r.source, SelectionSource::Builtin);
        assert_eq!(r.init_script(), "");
    }

    #[test]
    fn legacy_agent_url_still_works() {
        let cfg = Fdc3Config {
            agent_url: Some("ws://host:9/fdc3".into()),
            ..Fdc3Config::default()
        };
        let r = resolve(&cfg, None, None);
        assert_eq!(r.selected_id, CONFIG_ID);
        assert_eq!(
            r.init_script(),
            r#"window.OT_FDC3_AGENT_URL = "ws://host:9/fdc3";"#
        );

        let off = Fdc3Config {
            agent_url: Some(String::new()),
            ..Fdc3Config::default()
        };
        let r = resolve(&off, None, None);
        assert_eq!(r.selected_id, NONE_ID);
        assert_eq!(r.init_script(), r#"window.OT_FDC3_AGENT_URL = "";"#);
    }

    #[test]
    fn default_agent_then_saved_choice_then_env() {
        let cfg = cfg_with_agents();
        assert_eq!(resolve(&cfg, None, None).selected_id, "uat");

        let r = resolve(&cfg, None, Some("prod".into()));
        assert_eq!(r.selected_id, "prod");
        assert_eq!(r.source, SelectionSource::User);

        let r = resolve(&cfg, Some("ws://forced/fdc3".into()), Some("prod".into()));
        assert_eq!(r.selected_id, ENV_ID);
        assert!(r.listing().locked);
        assert_eq!(
            r.init_script(),
            r#"window.OT_FDC3_AGENT_URL = "ws://forced/fdc3";"#
        );
    }

    #[test]
    fn saved_choice_removed_from_config_falls_back_with_notice() {
        let r = resolve(&cfg_with_agents(), None, Some("gone".into()));
        assert_eq!(r.selected_id, "uat");
        assert!(r.notice.as_deref().unwrap().contains("'gone'"));
        assert!(r.discard_saved);
    }

    #[test]
    fn override_disallowed_ignores_saved_choice_and_rejects_select() {
        let cfg = Fdc3Config {
            allow_user_override: false,
            ..cfg_with_agents()
        };
        let mut r = resolve(&cfg, None, Some("prod".into()));
        assert_eq!(r.selected_id, "uat");
        assert!(r.select(Some("prod")).is_err());
    }

    #[test]
    fn select_rejects_unsupported_kind_and_reports_change() {
        let mut r = resolve(&cfg_with_agents(), None, None);
        assert!(r.select(Some("io")).is_err());
        assert_eq!(r.select(Some("prod")), Ok(true));
        assert_eq!(r.select(Some("prod")), Ok(false));
        assert_eq!(r.select(None), Ok(true));
        assert_eq!(r.selected_id, "uat");
        assert_eq!(r.source, SelectionSource::Config);
    }

    #[test]
    fn local_not_duplicated_when_configured() {
        let cfg = Fdc3Config {
            agents: vec![profile("mine", "local", "onet", PLUGIN_DEFAULT_URL)],
            ..Fdc3Config::default()
        };
        let r = resolve(&cfg, None, None);
        assert!(!r.agents.iter().any(|a| a.id == LOCAL_ID));
        assert_eq!(r.selected_id, "mine");
    }

    #[test]
    fn saved_selection_round_trips() {
        let dir = std::env::temp_dir().join(format!("ot-agent-sel-{}", std::process::id()));
        let path = dir.join(SELECTION_FILE);
        write_saved(&path, "prod").unwrap();
        assert_eq!(read_saved(&path).as_deref(), Some("prod"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
