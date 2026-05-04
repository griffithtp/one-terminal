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
