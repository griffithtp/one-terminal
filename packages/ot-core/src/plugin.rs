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
//! # The plugin binary
//!
//! `binaryName` is a bare file name inside the manifest's own folder
//! (`<engine-cache>/plugins/<family>/cef-host`, plus `.exe` on Windows). It is
//! never looked up on `PATH`: [`EngineManifest::resolve_binary`] refuses names
//! with path separators, `..` or a drive prefix, and a file that resolves
//! (through a symlink, say) to anywhere outside that folder.
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
// `rename_all` renames the variants (`spawnBinary`); `rename_all_fields` the
// fields inside them (`binaryName`, `envTemplates`), as the documented format
// and the bundled manifests spell them. The snake_case aliases keep any
// manifest written to match the old (accidental) field names loading.
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
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
        #[serde(alias = "binary_name")]
        binary_name: String,
        #[serde(default, alias = "env_templates")]
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
    /// Folder the manifest was loaded from; [`LaunchMode::SpawnBinary`]
    /// binaries must live here. Empty for a manifest not loaded from disk,
    /// which therefore can't spawn a binary.
    #[serde(skip)]
    pub dir: PathBuf,
}

impl EngineManifest {
    /// Load a single manifest from a JSON file.
    pub fn load(path: &Path) -> Result<Self, String> {
        let json = std::fs::read_to_string(path)
            .map_err(|e| format!("read manifest {}: {e}", path.display()))?;
        let mut manifest: Self = serde_json::from_str(&json)
            .map_err(|e| format!("parse manifest {}: {e}", path.display()))?;
        manifest.dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(manifest)
    }

    /// Absolute path of `binary_name` inside this manifest's folder.
    ///
    /// Refuses (with the reason) a name that is empty, contains a path
    /// separator, `..`, a NUL or a `:` (Windows drive / stream), a missing
    /// file, a directory, and a file whose canonical path is outside the
    /// manifest's folder. On Windows a name without an extension also matches
    /// `<name>.exe`.
    pub fn resolve_binary(&self, binary_name: &str) -> Result<PathBuf, String> {
        let bad = binary_name.is_empty()
            || binary_name == "."
            || binary_name == ".."
            || binary_name.contains(['/', '\\', ':', '\0']);
        if bad {
            return Err(format!(
                "plugin '{}': binaryName '{binary_name}' must be a file name in the plugin's folder",
                self.family
            ));
        }
        if self.dir.as_os_str().is_empty() {
            return Err(format!(
                "plugin '{}': manifest has no folder to resolve binaryName in",
                self.family
            ));
        }
        let dir = self.dir.canonicalize().map_err(|e| {
            format!(
                "plugin '{}': folder {}: {e}",
                self.family,
                self.dir.display()
            )
        })?;

        let mut candidates = vec![dir.join(binary_name)];
        if cfg!(windows) && Path::new(binary_name).extension().is_none() {
            candidates.push(dir.join(format!("{binary_name}.exe")));
        }
        for candidate in candidates {
            let Ok(real) = candidate.canonicalize() else {
                continue;
            };
            if !real.starts_with(&dir) {
                return Err(format!(
                    "plugin '{}': binaryName '{binary_name}' resolves outside the plugin's folder",
                    self.family
                ));
            }
            if real.is_file() {
                return Ok(real);
            }
        }
        Err(format!(
            "plugin '{}': binary '{binary_name}' not found in {}",
            self.family,
            dir.display()
        ))
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
                        tracing::info!("loaded manifest for '{}'", m.family);
                        manifests.push(m);
                    }
                    Err(e) => tracing::warn!("{e}"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh `<tmp>/ot-core-plugin-<pid>-<n>/plugins/demo/` folder.
    fn plugin_dir() -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "ot-core-plugin-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let dir = root.join("plugins").join("demo");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_manifest(dir: &Path, binary: &str) -> EngineManifest {
        let json = format!(
            r#"{{"family":"demo","label":"Demo","launchMode":{{"type":"spawnBinary","binaryName":"{binary}"}}}}"#
        );
        let path = dir.join("manifest.json");
        std::fs::write(&path, json).unwrap();
        EngineManifest::load(&path).unwrap()
    }

    #[test]
    fn load_records_the_manifest_folder() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "demo-host");
        assert_eq!(m.dir, dir);
        let root = dir.parent().unwrap().parent().unwrap();
        let scanned = EngineManifest::scan_plugins(root);
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].dir, dir);
    }

    #[test]
    fn resolves_a_binary_in_the_plugin_folder() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "demo-host");
        std::fs::write(dir.join("demo-host"), b"").unwrap();
        let resolved = m.resolve_binary("demo-host").unwrap();
        assert!(resolved.is_absolute());
        assert_eq!(resolved, dir.canonicalize().unwrap().join("demo-host"));
    }

    #[test]
    fn refuses_names_that_are_paths() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "x");
        for name in [
            "",
            ".",
            "..",
            "../demo-host",
            "sub/demo-host",
            "sub\\demo-host",
            "/bin/sh",
            "C:\\Windows\\System32\\cmd.exe",
            "C:cmd.exe",
            "demo-host:stream",
            "demo\0host",
        ] {
            let err = m.resolve_binary(name).expect_err(name);
            assert!(err.contains("must be a file name"), "{name}: {err}");
        }
    }

    #[test]
    fn never_falls_back_to_path() {
        // `sh` exists on PATH on every Unix CI runner; it must not be found.
        let dir = plugin_dir();
        let m = write_manifest(&dir, "sh");
        let err = m.resolve_binary("sh").unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn refuses_a_directory() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "bin");
        std::fs::create_dir(dir.join("bin")).unwrap();
        assert!(m.resolve_binary("bin").unwrap_err().contains("not found"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlink_out_of_the_plugin_folder() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "escape");
        std::os::unix::fs::symlink("/bin/sh", dir.join("escape")).unwrap();
        let err = m.resolve_binary("escape").unwrap_err();
        assert!(err.contains("outside the plugin's folder"), "{err}");
    }

    #[cfg(windows)]
    #[test]
    fn windows_adds_exe() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "demo-host");
        std::fs::write(dir.join("demo-host.exe"), b"").unwrap();
        let resolved = m.resolve_binary("demo-host").unwrap();
        assert!(resolved.ends_with("demo-host.exe"));
    }

    #[test]
    fn parses_the_documented_camel_case_format_and_the_old_snake_case() {
        for json in [
            r#"{"family":"cef","label":"CEF","launchMode":{"type":"spawnBinary","binaryName":"cef-host","envTemplates":[{"key":"CEF_URL","value":"{{url}}"}]}}"#,
            r#"{"family":"cef","label":"CEF","launchMode":{"type":"spawnBinary","binary_name":"cef-host","env_templates":[{"key":"CEF_URL","value":"{{url}}"}]}}"#,
        ] {
            let m: EngineManifest = serde_json::from_str(json).unwrap();
            match m.launch_mode {
                LaunchMode::SpawnBinary {
                    binary_name,
                    env_templates,
                } => {
                    assert_eq!(binary_name, "cef-host");
                    assert_eq!(env_templates.len(), 1);
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn a_manifest_not_loaded_from_disk_cannot_spawn() {
        let m: EngineManifest = serde_json::from_str(
            r#"{"family":"demo","label":"Demo","launchMode":{"type":"inProcess"}}"#,
        )
        .unwrap();
        assert!(m
            .resolve_binary("demo-host")
            .unwrap_err()
            .contains("no folder"));
    }
}
