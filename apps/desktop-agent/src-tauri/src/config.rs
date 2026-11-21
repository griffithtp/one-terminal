//! Desktop Agent configuration — loaded once at startup.
//!
//! Layers, later overriding earlier (objects merge per key; arrays replace):
//!   1. Compiled-in defaults
//!   2. Bundled `agent.config.json` (next to the binary / in app resources)
//!   3. User `agent.config.json` in the per-user config dir (see
//!      [`user_config_path`]) — survives app updates, needs no rebuild
//!   4. `OT_*` environment variables
//!
//! Each file may be partial, e.g. `{ "ports": { "fdc3Bus": 7999 } }`. The
//! effective value of every setting and the layer it came from is logged at
//! startup — after telemetry is initialised, since telemetry's own settings
//! come from this file (see [`AgentConfig::load_quiet`]).

use ot_telemetry::TelemetryConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

// ── Sub-structs ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortsConfig {
    pub fdc3_bus: u16,
}

impl PortsConfig {
    /// Probe each port by attempting a short-lived bind on `127.0.0.1`.
    /// Returns `Err` listing every conflict so the caller can print them all at once.
    pub fn check_availability(&self) -> Result<(), String> {
        let checks = [(self.fdc3_bus, "OT_FDC3_BUS_PORT", "ports.fdc3Bus")];
        let conflicts: Vec<String> = checks
            .iter()
            .filter_map(|&(port, env_var, key)| {
                TcpListener::bind(("127.0.0.1", port)).err().map(|_| {
                    format!(
                        "  port {port} is already in use — set {key} in the user config or {env_var}=<port> to override"
                    )
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserChannel {
    pub id: String,
    pub display_name: String,
    pub color: String,
}

// ── AgentConfig ───────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub title: String,
    pub app_directory_url: String,
    pub engine_catalog_url: String,
    pub ports: PortsConfig,
    pub user_channels: Vec<UserChannel>,
    /// Start an in-process App Directory server bound to `app_directory_url`'s
    /// port (see `packages/ot-appdirectory`). Defaults to `true`; set
    /// `OT_APPD_EMBEDDED=0` when pointing `OT_APP_DIR_URL` at a remote/hosted
    /// App Directory instead.
    #[serde(default = "default_appd_embedded")]
    pub appd_embedded: bool,
    /// Log files and opt-in OTLP export. Environment overrides (`OT_LOG`,
    /// `OT_LOG_DIR`, `OT_OTLP_ENDPOINT`, standard `OTEL_*`) are applied by
    /// `ot_telemetry::resolve`, not here.
    #[serde(default)]
    pub telemetry: TelemetryConfig,
}

fn default_appd_embedded() -> bool {
    true
}

/// Best-effort port extraction from an `http(s)://host[:port]` URL, falling
/// back to 3005 (the App Directory default) if no explicit port is present
/// or the URL can't be parsed. Avoids pulling in a full URL-parsing crate for
/// this one call site.
pub fn port_from_url(url: &str) -> u16 {
    url.rsplit_once(':')
        .and_then(|(_, rest)| rest.trim_end_matches('/').parse().ok())
        .unwrap_or(3005)
}

impl AgentConfig {
    /// Load the config without logging: logging isn't set up yet, because
    /// telemetry's settings come from this config. Call
    /// [`LoadReport::log`] once `ot_telemetry::init` has run.
    pub fn load_quiet() -> (Self, LoadReport) {
        let mut report = LoadReport::default();
        let bundled = Self::search_paths().into_iter().find(|p| p.is_file());
        let user = user_config_path();
        let (cfg, sources) = Self::load_layers(bundled.as_deref(), user.as_deref(), &mut report);
        cfg.log_effective(&sources, bundled.as_deref(), user.as_deref(), &mut report);
        (cfg, report)
    }

    /// Merge defaults → bundled file → user file, then apply env overrides.
    /// Returns the config plus a map of setting path → source label.
    fn load_layers(
        bundled: Option<&Path>,
        user: Option<&Path>,
        report: &mut LoadReport,
    ) -> (Self, Sources) {
        let defaults = Self::defaults();
        let mut sources = Sources::new();
        let mut merged = serde_json::to_value(&defaults).expect("defaults serialize");
        let mut without_user = None;

        if let Some(path) = bundled {
            if let Some(v) = read_json(path, report) {
                merge(&mut merged, v, "", "bundled config", &mut sources, report);
            }
        }
        if let Some(path) = user.filter(|p| p.is_file()) {
            if let Some(v) = read_json(path, report) {
                without_user = Some((merged.clone(), sources.clone()));
                merge(&mut merged, v, "", "user config", &mut sources, report);
            }
        }

        let cfg = match serde_json::from_value::<Self>(merged) {
            Ok(cfg) => cfg,
            Err(e) => {
                // A bad value type (e.g. a port given as a string) — drop the
                // most recent layer rather than refusing to start.
                report.warn(format!("config: invalid value ({e}); ignoring user config"));
                match without_user {
                    Some((v, s)) => {
                        sources = s;
                        serde_json::from_value(v).unwrap_or_else(|e| {
                            report.warn(format!(
                                "config: bundled config also invalid ({e}); using defaults"
                            ));
                            sources.clear();
                            defaults
                        })
                    }
                    None => {
                        sources.clear();
                        defaults
                    }
                }
            }
        };
        (
            Self::apply_env_overrides(cfg, &mut sources, report),
            sources,
        )
    }

    /// Record every setting with the layer it came from.
    fn log_effective(
        &self,
        sources: &Sources,
        bundled: Option<&Path>,
        user: Option<&Path>,
        report: &mut LoadReport,
    ) {
        match bundled {
            Some(p) => report.info(format!("bundled config: {}", p.display())),
            None => report.info("bundled config: none found"),
        }
        match user {
            Some(p) if p.is_file() => report.info(format!("user config: {}", p.display())),
            Some(p) => report.info(format!(
                "user config: {} (not present — create it to override settings)",
                p.display()
            )),
            None => {}
        }
        let src = |key: &str| sources.get(key).map(String::as_str).unwrap_or("default");
        let rows: [(&str, String); 6] = [
            ("title", self.title.clone()),
            (
                "appDirectoryUrl",
                ot_telemetry::redact::url(&self.app_directory_url),
            ),
            (
                "engineCatalogUrl",
                ot_telemetry::redact::url(&self.engine_catalog_url),
            ),
            ("appdEmbedded", self.appd_embedded.to_string()),
            ("ports.fdc3Bus", self.ports.fdc3_bus.to_string()),
            (
                "userChannels",
                format!("{} channels", self.user_channels.len()),
            ),
        ];
        for (key, value) in rows {
            report.info(format!("config {key} = {value} ({})", src(key)));
        }
        // Effective telemetry settings (with environment overrides applied)
        // are logged by `ot_telemetry::init`.
    }

    fn defaults() -> Self {
        Self {
            title: "OneTerminal Desktop Agent".to_string(),
            app_directory_url: "http://localhost:3005".to_string(),
            engine_catalog_url: "http://localhost:3005".to_string(),
            ports: PortsConfig { fdc3_bus: 7891 },
            user_channels: Self::default_channels(),
            appd_embedded: true,
            telemetry: TelemetryConfig::default(),
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

    fn search_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        // Next to binary (release).
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                paths.push(dir.join("agent.config.json"));
                paths.push(dir.join("resources").join("agent.config.json"));
                // macOS .app bundles: `bundle.resources` is placed under
                // Contents/Resources/, not next to the executable in
                // Contents/MacOS/ — current_exe() resolves to the latter, so
                // check the sibling `../Resources/resources/` location too.
                if let Some(contents_dir) = dir.parent() {
                    paths.push(
                        contents_dir
                            .join("Resources")
                            .join("resources")
                            .join("agent.config.json"),
                    );
                }
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

    fn apply_env_overrides(mut cfg: Self, sources: &mut Sources, report: &mut LoadReport) -> Self {
        let mut set = |key: &str, var: &str| {
            sources.insert(key.to_string(), format!("env {var}"));
        };
        if let Ok(v) = std::env::var("OT_TITLE") {
            cfg.title = v;
            set("title", "OT_TITLE");
        }
        if let Ok(v) = std::env::var("OT_APP_DIR_URL") {
            cfg.app_directory_url = v;
            set("appDirectoryUrl", "OT_APP_DIR_URL");
        }
        if let Ok(v) = std::env::var("OT_CATALOG_URL") {
            cfg.engine_catalog_url = v;
            set("engineCatalogUrl", "OT_CATALOG_URL");
        }
        if let Ok(v) = std::env::var("OT_FDC3_BUS_PORT") {
            match v.parse() {
                Ok(p) => {
                    cfg.ports.fdc3_bus = p;
                    set("ports.fdc3Bus", "OT_FDC3_BUS_PORT");
                }
                Err(_) => report.warn(format!(
                    "config: ignoring OT_FDC3_BUS_PORT={v:?} (not a port number)"
                )),
            }
        }
        if let Ok(v) = std::env::var("OT_APPD_EMBEDDED") {
            cfg.appd_embedded = !matches!(v.as_str(), "0" | "false" | "FALSE" | "False");
            set("appdEmbedded", "OT_APPD_EMBEDDED");
        }
        cfg
    }
}

// ── Layer helpers ─────────────────────────────────────────────────────────────

/// Setting path (e.g. `ports.fdc3Bus`) → label of the layer that last set it.
type Sources = BTreeMap<String, String>;

/// Messages produced while loading the config, held until logging is set up.
#[derive(Debug, Default)]
pub struct LoadReport {
    lines: Vec<(Severity, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Severity {
    Info,
    Warn,
}

impl LoadReport {
    fn info(&mut self, msg: impl Into<String>) {
        self.lines.push((Severity::Info, msg.into()));
    }

    fn warn(&mut self, msg: impl Into<String>) {
        self.lines.push((Severity::Warn, msg.into()));
    }

    /// Emit the collected messages through `tracing`.
    pub fn log(&self) {
        for (severity, msg) in &self.lines {
            match severity {
                Severity::Info => tracing::info!("{msg}"),
                Severity::Warn => tracing::warn!("{msg}"),
            }
        }
    }
}

/// Per-user `agent.config.json`, shared `one-terminal` product dir (same
/// convention as `ot_core::engine::default_cache_root`):
/// - macOS:   `~/Library/Application Support/one-terminal/agent.config.json`
/// - Windows: `%APPDATA%\one-terminal\agent.config.json`
/// - Linux:   `$XDG_CONFIG_HOME/one-terminal/…` or `~/.config/one-terminal/…`
pub fn user_config_path() -> Option<PathBuf> {
    let product = "one-terminal";
    let file = "agent.config.json";

    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME").ok().map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(product)
                .join(file)
        })
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|appdata| PathBuf::from(appdata).join(product).join(file))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var("XDG_CONFIG_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| PathBuf::from(h).join(".config"))
            })
            .map(|dir| dir.join(product).join(file))
    }
}

fn read_json(path: &Path, report: &mut LoadReport) -> Option<Value> {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            report.warn(format!("config read error at {}: {e}", path.display()));
            return None;
        }
    };
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),
        Err(e) => {
            report.warn(format!("config parse error at {}: {e}", path.display()));
            None
        }
    }
}

/// Deep-merge `overlay` into `base`. Objects merge per key; any other value
/// (including arrays) replaces. Keys unknown to `base` are warned about and
/// skipped so typos don't fail silently. An object that is empty in the
/// defaults is a free-form map (`telemetry.otlp.headers`,
/// `telemetry.resourceAttributes`): the overlay replaces it whole.
fn merge(
    base: &mut Value,
    overlay: Value,
    prefix: &str,
    source: &str,
    sources: &mut Sources,
    report: &mut LoadReport,
) {
    let (Value::Object(base), Value::Object(overlay)) = (base, overlay) else {
        return;
    };
    for (key, value) in overlay {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match base.get_mut(&key) {
            None => report.warn(format!(
                "config: unknown key '{path}' in {source} — ignored"
            )),
            Some(slot @ Value::Object(_))
                if value.is_object() && slot.as_object().is_some_and(|m| !m.is_empty()) =>
            {
                merge(slot, value, &path, source, sources, report);
            }
            Some(slot) => {
                *slot = value;
                sources.insert(path, source.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, json: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, json).unwrap();
        p
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ot-agent-cfg-{tag}-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn user_file_overrides_bundled_per_key() {
        let d = tmp_dir("merge");
        let bundled = write(
            &d,
            "b.json",
            r#"{"title":"Bundled","ports":{"fdc3Bus":1001}}"#,
        );
        let user = write(&d, "u.json", r#"{"ports":{"fdc3Bus":7999}}"#);
        let (cfg, sources) =
            AgentConfig::load_layers(Some(&bundled), Some(&user), &mut LoadReport::default());
        assert_eq!(cfg.title, "Bundled");
        assert_eq!(cfg.ports.fdc3_bus, 7999);
        assert_eq!(sources["ports.fdc3Bus"], "user config");
        assert_eq!(cfg.user_channels.len(), 8, "untouched arrays keep defaults");
    }

    #[test]
    fn missing_files_give_defaults() {
        let d = tmp_dir("none");
        let (cfg, sources) = AgentConfig::load_layers(
            None,
            Some(&d.join("absent.json")),
            &mut LoadReport::default(),
        );
        assert_eq!(cfg.ports.fdc3_bus, 7891);
        assert!(sources.keys().all(|k| !k.starts_with("ports")));
    }

    #[test]
    fn invalid_user_value_falls_back_to_bundled() {
        let d = tmp_dir("bad");
        let bundled = write(&d, "b.json", r#"{"ports":{"fdc3Bus":1001}}"#);
        let user = write(&d, "u.json", r#"{"ports":{"fdc3Bus":"nope"}}"#);
        let (cfg, _) =
            AgentConfig::load_layers(Some(&bundled), Some(&user), &mut LoadReport::default());
        assert_eq!(cfg.ports.fdc3_bus, 1001);
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let mut base = serde_json::json!({"ports":{"fdc3Bus":1}});
        let mut sources = Sources::new();
        let mut report = LoadReport::default();
        merge(
            &mut base,
            serde_json::json!({"ports":{"fdc3bus":2},"typo":true}),
            "",
            "u",
            &mut sources,
            &mut report,
        );
        assert_eq!(base, serde_json::json!({"ports":{"fdc3Bus":1}}));
        assert!(sources.is_empty());
        assert_eq!(report.lines.len(), 2, "one warning per unknown key");
    }

    #[test]
    fn telemetry_layers_merge_per_key() {
        let d = tmp_dir("telemetry");
        let bundled = write(
            &d,
            "b.json",
            r#"{"telemetry":{"logLevel":"debug","otlp":{"timeoutMs":5000}}}"#,
        );
        let user = write(
            &d,
            "u.json",
            r#"{"telemetry":{"otlp":{"endpoint":"http://localhost:4318","headers":{"authorization":"Bearer x"}},"resourceAttributes":{"deployment.environment":"uat"}}}"#,
        );
        let mut report = LoadReport::default();
        let (cfg, sources) = AgentConfig::load_layers(Some(&bundled), Some(&user), &mut report);
        let t = &cfg.telemetry;
        assert_eq!(t.log_level.as_deref(), Some("debug"));
        assert_eq!(t.otlp.timeout_ms, 5000);
        assert_eq!(t.otlp.endpoint.as_deref(), Some("http://localhost:4318"));
        assert_eq!(
            t.otlp.headers.0["authorization"], "Bearer x",
            "free-form map kept"
        );
        assert_eq!(t.resource_attributes["deployment.environment"], "uat");
        assert!(t.otlp.signals.traces, "untouched keys keep defaults");
        assert_eq!(sources["telemetry.otlp.endpoint"], "user config");
        assert_eq!(sources["telemetry.logLevel"], "bundled config");
        assert!(
            report.lines.iter().all(|(s, _)| *s == Severity::Info),
            "no unknown-key warnings: {:?}",
            report.lines
        );
    }

    #[test]
    fn startup_report_redacts_url_credentials() {
        let d = tmp_dir("redact");
        let user = write(
            &d,
            "u.json",
            r#"{"appDirectoryUrl":"http://u:pw@appd.example/?token=t0k"}"#,
        );
        let mut report = LoadReport::default();
        let (cfg, sources) = AgentConfig::load_layers(None, Some(&user), &mut report);
        cfg.log_effective(&sources, None, Some(&user), &mut report);
        let text = format!("{:?}", report.lines);
        assert!(!text.contains("pw") && !text.contains("t0k"), "{text}");
    }

    #[test]
    fn telemetry_defaults_to_no_export() {
        let (cfg, _) = AgentConfig::load_layers(None, None, &mut LoadReport::default());
        assert!(cfg.telemetry.otlp.endpoint.is_none());
    }
}
