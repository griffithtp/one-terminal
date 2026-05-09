use serde::Deserialize;
use std::fs;
use std::net::TcpListener;
use std::path::PathBuf;

// ── Sub-structs ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortsConfig {
    pub tcp_broker: u16,
    pub fdc3_bus: u16,
    pub dacp_bridge: u16,
}

impl PortsConfig {
    /// Probe each port by attempting a short-lived bind on `127.0.0.1`.
    /// Returns `Err` listing every conflict so the caller can print them all at once.
    pub fn check_availability(&self) -> Result<(), String> {
        let checks = [
            (self.tcp_broker, "OT_TCP_PORT"),
            (self.fdc3_bus, "OT_FDC3_BUS_PORT"),
            (self.dacp_bridge, "OT_DACP_PORT"),
        ];
        let conflicts: Vec<String> = checks
            .iter()
            .filter_map(|&(port, env_var)| {
                TcpListener::bind(("127.0.0.1", port)).err().map(|_| {
                    format!("  port {port} is already in use — set {env_var}=<port> to override")
                })
            })
            .collect();

        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "startup aborted: port conflict(s) detected\n{}",
                conflicts.join("\n")
            ))
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserChannel {
    pub id: String,
    pub display_name: String,
    pub color: String,
}

// ── AgentConfig ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub title: String,
    pub app_directory_url: String,
    pub engine_catalog_url: String,
    pub ports: PortsConfig,
    pub user_channels: Vec<UserChannel>,
}

impl AgentConfig {
    pub fn load() -> Self {
        let file_cfg = Self::from_file().unwrap_or_else(Self::defaults);
        Self::apply_env_overrides(file_cfg)
    }

    fn defaults() -> Self {
        Self {
            title: "OneTerminal Desktop Agent".to_string(),
            app_directory_url: "http://localhost:3005".to_string(),
            engine_catalog_url: "http://localhost:3005".to_string(),
            ports: PortsConfig {
                tcp_broker: 7890,
                fdc3_bus: 7891,
                dacp_bridge: 4475,
            },
            user_channels: Self::default_channels(),
        }
    }

    fn default_channels() -> Vec<UserChannel> {
        vec![
            UserChannel {
                id: "Red".into(),
                display_name: "Red".into(),
                color: "#E74C3C".into(),
            },
            UserChannel {
                id: "Orange".into(),
                display_name: "Orange".into(),
                color: "#E67E22".into(),
            },
            UserChannel {
                id: "Yellow".into(),
                display_name: "Yellow".into(),
                color: "#F1C40F".into(),
            },
            UserChannel {
                id: "Green".into(),
                display_name: "Green".into(),
                color: "#2ECC71".into(),
            },
            UserChannel {
                id: "Teal".into(),
                display_name: "Teal".into(),
                color: "#1ABC9C".into(),
            },
            UserChannel {
                id: "Blue".into(),
                display_name: "Blue".into(),
                color: "#3498DB".into(),
            },
            UserChannel {
                id: "Purple".into(),
                display_name: "Purple".into(),
                color: "#9B59B6".into(),
            },
            UserChannel {
                id: "Pink".into(),
                display_name: "Pink".into(),
                color: "#EC407A".into(),
            },
        ]
    }

    fn from_file() -> Option<Self> {
        let paths = Self::search_paths();
        for path in paths {
            if path.exists() {
                if let Ok(json) = fs::read_to_string(&path) {
                    match serde_json::from_str(&json) {
                        Ok(cfg) => {
                            println!("[desktop-agent] config loaded from {}", path.display());
                            return Some(cfg);
                        }
                        Err(e) => eprintln!(
                            "[desktop-agent] config parse error at {}: {e}",
                            path.display()
                        ),
                    }
                }
            }
        }
        None
    }

    fn search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        // Next to binary (release).
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                paths.push(dir.join("agent.config.json"));
                paths.push(dir.join("resources").join("agent.config.json"));
            }
        }
        // Tauri dev: cwd/src-tauri/resources.
        if let Ok(cwd) = std::env::current_dir() {
            paths.push(cwd.join("resources").join("agent.config.json"));
            paths.push(
                cwd.join("src-tauri")
                    .join("resources")
                    .join("agent.config.json"),
            );
        }
        paths
    }

    fn apply_env_overrides(mut cfg: Self) -> Self {
        if let Ok(v) = std::env::var("OT_TITLE") {
            cfg.title = v;
        }
        if let Ok(v) = std::env::var("OT_APP_DIR_URL") {
            cfg.app_directory_url = v;
        }
        if let Ok(v) = std::env::var("OT_CATALOG_URL") {
            cfg.engine_catalog_url = v;
        }
        if let Ok(v) = std::env::var("OT_TCP_PORT") {
            if let Ok(p) = v.parse() {
                cfg.ports.tcp_broker = p;
            }
        }
        if let Ok(v) = std::env::var("OT_FDC3_BUS_PORT") {
            if let Ok(p) = v.parse() {
                cfg.ports.fdc3_bus = p;
            }
        }
        if let Ok(v) = std::env::var("OT_DACP_PORT") {
            if let Ok(p) = v.parse() {
                cfg.ports.dacp_bridge = p;
            }
        }
        cfg
    }
}
