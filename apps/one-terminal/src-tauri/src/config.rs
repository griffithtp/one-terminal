//! OneTerminal configuration — loaded once at startup, injected via Tauri State.
//!
//! Resolution order (later overrides earlier):
//!   1. Compiled-in defaults (always present — app works with no config file)
//!   2. `terminal.config.json` alongside the binary (or in the resource dir in release)
//!   3. Environment variable overrides (`OT_TITLE`, `OT_APP_DIR_URL`, `OT_CATALOG_URL`)
//!
//! Integrators only need to ship a `terminal.config.json` — no code changes required.

use serde::{Deserialize, Serialize};

// ── Sub-structs ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowConfig {
    pub width: f64,
    pub height: f64,
    pub min_width: f64,
    pub min_height: f64,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1600.0,
            height: 900.0,
            min_width: 640.0,
            min_height: 400.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutConfig {
    pub header_height: f64,
    pub panel_header_height: f64,
    pub tab_strip_height: f64,
    pub splitter_thickness: f64,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            header_height: 40.0,
            panel_header_height: 28.0,
            tab_strip_height: 28.0,
            splitter_thickness: 4.0,
        }
    }
}

// ── FDC3 client configuration ─────────────────────────────────────────────────
// Standalone scaffolds set `agent_url` to an external FDC3 agent's WS endpoint.
// Enterprise scaffolds omit it — nothing is injected and fdc3-plugin falls back
// to the bundled Desktop Agent's bus (ws://localhost:7891/fdc3).
//
// `agents` / `default_agent` / `allow_user_override` drive runtime agent
// selection — see `docs/fdc3.md` and `crate::agent` for how they resolve against the
// `OT_FDC3_AGENT_URL` env var and the user's saved choice.

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fdc3Config {
    /// Legacy single-agent URL.
    /// - `None` (key absent): nothing injected; fdc3-plugin uses its default
    ///   resolution chain (meta tag → `OT_FDC3_BUS_URL` → localhost:7891).
    /// - `Some("")`: FDC3 explicitly disabled — `connect()` rejects with
    ///   NoAgentConfigured.
    /// - `Some(url)`: widgets connect to `url`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_url: Option<String>,
    /// Agents the user can choose between in App Menu → Desktop Agent.
    #[serde(default)]
    pub agents: Vec<AgentProfile>,
    /// `id` of the agent used when the user hasn't chosen one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_agent: Option<String>,
    /// When `false`, the user's saved choice is ignored and the Desktop Agent
    /// section is read-only.
    #[serde(default = "default_true")]
    pub allow_user_override: bool,
}

impl Default for Fdc3Config {
    fn default() -> Self {
        Self {
            agent_url: None,
            agents: Vec::new(),
            default_agent: None,
            allow_user_override: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// One selectable FDC3 Desktop Agent.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    /// Stable id — what the user's saved choice and `defaultAgent` refer to.
    pub id: String,
    /// Display name, e.g. "OneTerminal UAT".
    pub name: String,
    /// Environment label used for grouping and the header badge colour
    /// (`prod` is highlighted), e.g. "uat", "prod".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    /// Connector kind. Only `onet` (OneTerminal's `fdc3:*` WebSocket
    /// protocol) can be connected to today; other kinds (`openfin`,
    /// `interopio`, `finsemble`) are listed but disabled until their
    /// connector lands (see "Connector kinds" in `docs/fdc3.md`).
    #[serde(default = "default_agent_kind")]
    pub kind: String,
    /// Agent endpoint, e.g. `ws://localhost:7891/fdc3`.
    pub url: String,
}

fn default_agent_kind() -> String {
    "onet".into()
}

// ── Widget source ─────────────────────────────────────────────────────────────
// Deprecated/non-gating. The launcher now reads the *union* of the App Directory
// and the local widgets.config.json (see widgets.rs): an empty appDirectoryUrl
// disables the appd source, a missing widgets.config.json disables the local
// source. This enum is retained only for back-compat deserialization of older
// terminal.config.json files and no longer selects between sources.

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum WidgetSourceKind {
    #[default]
    Appd,
    Local,
}

fn default_local_widgets_path() -> String {
    "widgets.config.json".into()
}

// ── Root config ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalConfig {
    /// Display name shown in the title bar and header toolbar.
    pub title: String,
    /// FDC3 App Directory endpoint used by the app launcher.
    pub app_directory_url: String,
    /// Engine catalog endpoint (usually the same host as the app directory).
    pub engine_catalog_url: String,
    /// Which widget catalog source the launcher reads from.
    #[serde(default)]
    pub widget_source: WidgetSourceKind,
    /// When widget_source == Local, the path (relative to cwd or the binary)
    /// to the widgets.config.json file.
    #[serde(default = "default_local_widgets_path")]
    pub local_widgets_path: String,
    /// FDC3 client configuration (external agent URL, etc).
    #[serde(default)]
    pub fdc3: Fdc3Config,
    /// Initial and minimum window dimensions.
    #[serde(default)]
    pub window: WindowConfig,
    /// Layout geometry constants — shared between Rust (reflow) and the
    /// frontend (hit-testing) via the `wm_config` IPC command.
    #[serde(default)]
    pub layout: LayoutConfig,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            title: "OneTerminal".into(),
            app_directory_url: "http://localhost:3005/v2/apps".into(),
            engine_catalog_url: "http://localhost:3005/v2/engines".into(),
            widget_source: WidgetSourceKind::default(),
            local_widgets_path: default_local_widgets_path(),
            fdc3: Fdc3Config::default(),
            window: WindowConfig::default(),
            layout: LayoutConfig::default(),
        }
    }
}

impl TerminalConfig {
    /// Load config from `terminal.config.json` next to the binary (dev) or in
    /// the Tauri resource directory (release), then apply env var overrides.
    pub fn load() -> Self {
        let mut cfg = Self::from_file().unwrap_or_default();
        cfg.apply_env_overrides();
        cfg
    }

    fn from_file() -> Option<Self> {
        // In dev mode the binary sits at target/debug/one-terminal, so look
        // one level up toward the src-tauri directory for the resources folder.
        let search_paths: Vec<std::path::PathBuf> = {
            let mut paths = Vec::new();
            if let Ok(exe) = std::env::current_exe() {
                // Release: resources/ next to binary
                if let Some(dir) = exe.parent() {
                    paths.push(dir.join("resources").join("terminal.config.json"));
                    paths.push(dir.join("terminal.config.json"));
                }
            }
            // Dev: walk up from cwd toward src-tauri/resources/
            if let Ok(cwd) = std::env::current_dir() {
                paths.push(cwd.join("resources").join("terminal.config.json"));
                paths.push(cwd.join("terminal.config.json"));
            }
            paths
        };

        for path in &search_paths {
            if path.is_file() {
                match std::fs::read_to_string(path) {
                    Ok(text) => match serde_json::from_str::<TerminalConfig>(&text) {
                        Ok(cfg) => {
                            println!("[config] loaded from {}", path.display());
                            return Some(cfg);
                        }
                        Err(e) => {
                            eprintln!("[config] parse error in {}: {e}", path.display());
                        }
                    },
                    Err(e) => {
                        eprintln!("[config] read error at {}: {e}", path.display());
                    }
                }
            }
        }

        None
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("OT_TITLE") {
            self.title = v;
        }
        if let Ok(v) = std::env::var("OT_APP_DIR_URL") {
            self.app_directory_url = v;
        }
        if let Ok(v) = std::env::var("OT_CATALOG_URL") {
            self.engine_catalog_url = v;
        }
        if let Ok(v) = std::env::var("OT_WIDGET_SOURCE") {
            match v.to_lowercase().as_str() {
                "local" => self.widget_source = WidgetSourceKind::Local,
                "appd" => self.widget_source = WidgetSourceKind::Appd,
                other => eprintln!("[config] unknown OT_WIDGET_SOURCE value: {other}"),
            }
        }
        if let Ok(v) = std::env::var("OT_WIDGETS_CONFIG_PATH") {
            self.local_widgets_path = v;
        }
        if let Ok(v) = std::env::var("OT_WINDOW_WIDTH") {
            if let Ok(n) = v.parse() {
                self.window.width = n;
            }
        }
        if let Ok(v) = std::env::var("OT_WINDOW_HEIGHT") {
            if let Ok(n) = v.parse() {
                self.window.height = n;
            }
        }
    }
}

/// Append a `window.OT_FDC3_INITIAL_CHANNEL` assignment to `base_script` when
/// `channel_id` is `Some`; returns `base_script` unchanged otherwise.
/// `fdc3-plugin.js` reads this once its handshake completes, to auto-rejoin
/// a panel's previously-selected FDC3 channel after the webview is destroyed
/// and recreated (dashboard switch, restart). Used at every panel-webview
/// creation site: `lib.rs`'s cold-start restore loop, `terminal/spawn.rs`'s
/// secondary-Terminal restore, and `LayoutTree::reconcile_panel_webviews`
/// (dashboard switch/discard/delete).
pub fn append_initial_channel(base_script: &str, channel_id: Option<&str>) -> String {
    match channel_id {
        Some(id) => {
            let encoded = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".to_string());
            format!("{base_script}\nwindow.OT_FDC3_INITIAL_CHANNEL = {encoded};")
        }
        None => base_script.to_string(),
    }
}
