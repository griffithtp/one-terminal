//! Engine plugin manifests.
//!
//! Drop a `manifest.json` file into `<engine-cache>/plugins/<family>/` to
//! register a new browser engine without recompiling OneTerminal. The manifest
//! declares the engine's family name, how to detect an install, and how to
//! launch it.
//!
//! # Manifest format
//!
//! ```json
//! {
//!   "family": "cef",
//!   "label": "Chromium Embedded Framework",
//!   "supportedPlatforms": ["windows", "macos", "linux"],
//!   "launchMode": {
//!     "type": "spawnBinary",
//!     "binaryName": "cef-host",
//!     "envTemplates": [
//!       { "key": "CEF_URL",   "value": "{{url}}" },
//!       { "key": "CEF_TITLE", "value": "{{title}}" }
//!     ]
//!   }
//! }
//! ```
//!
//! # Template variables
//!
//! The following placeholders are substituted in `envTemplates.value` at launch:
//! - `{{url}}` — the URL to load
//! - `{{title}}` — the window title
//! - `{{app_id}}` — the FDC3 app ID
//! - `{{runtime_path}}` — absolute path to the installed runtime folder, if any

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── LaunchMode ────────────────────────────────────────────────────────────────

/// An (key, template-value) pair expanded at launch time.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EnvTemplate {
    pub key: String,
    /// Template value; `{{url}}`, `{{title}}`, `{{app_id}}`, `{{runtime_path}}`
    /// are substituted at launch.
    pub value: String,
}

/// How the host should open a window for this engine family.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum LaunchMode {
    /// Open a Tauri `WebviewWindow` in the current process (system engine).
    InProcess,

    /// Spawn the `tauri-webview-host` sidecar; pass `runtime_path` via
    /// `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` if present.
    SpawnTauriHost,

    /// Use the `ot-core` Electron launcher (`spawn_electron_app`).
    SpawnElectronHost,

    /// Spawn a named binary with fully configurable env vars.
    SpawnBinary {
        binary_name: String,
        #[serde(default)]
        env_templates: Vec<EnvTemplate>,
    },
}

// ── EngineManifest ────────────────────────────────────────────────────────────

/// Declarative description of a plugin engine loaded from a JSON manifest.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineManifest {
    /// Engine family name — must match the `family` field in App Directory records.
    pub family: String,
    /// Human-readable label shown in UI pickers.
    pub label: String,
    /// How to launch a window with this engine.
    pub launch_mode: LaunchMode,
    /// OS keys on which this engine is available. Empty = all platforms.
    #[serde(default)]
    pub supported_platforms: Vec<String>,
}

impl EngineManifest {
    /// Load a single manifest from a JSON file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("read manifest {}: {e}", path.display()))?;
        serde_json::from_str(&json)
            .map_err(|e| format!("parse manifest {}: {e}", path.display()))
    }

    /// Scan `<root>/plugins/` for subdirectories each containing a
    /// `manifest.json` and return all successfully-loaded manifests.
    pub fn scan_plugins(root: &Path) -> Vec<Self> {
        let plugins_dir = root.join("plugins");
        let Ok(entries) = std::fs::read_dir(&plugins_dir) else {
            return Vec::new();
        };
        let mut manifests = Vec::new();
        for entry in entries.flatten() {
            let manifest_path = entry.path().join("manifest.json");
            if manifest_path.exists() {
                match Self::load(&manifest_path) {
                    Ok(m) => {
                        println!("[engine-plugin] loaded manifest for '{}'", m.family);
                        manifests.push(m);
                    }
                    Err(e) => eprintln!("[engine-plugin] {e}"),
                }
            }
        }
        manifests
    }

    /// Expand an env-template value, substituting known placeholders.
    pub fn expand_template(
        template: &str,
        url: &str,
        title: &str,
        app_id: &str,
        runtime_path: &str,
    ) -> String {
        template
            .replace("{{url}}", url)
            .replace("{{title}}", title)
            .replace("{{app_id}}", app_id)
            .replace("{{runtime_path}}", runtime_path)
    }
}

/// Path where plugin manifests live inside `root`.
pub fn plugins_dir(root: &Path) -> PathBuf {
    root.join("plugins")
}
