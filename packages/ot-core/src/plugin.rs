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
//! (`<engine-cache>/plugins/<family>/cef-host`). It is never looked up on
//! `PATH`: [`EngineManifest::resolve_binary`] refuses names with path
//! separators, `..` or a drive prefix, and a file that resolves (through a
//! symlink, say) to anywhere outside that folder. On Windows it must be a
//! real `.exe` (`cef-host` finds `cef-host.exe`): `.bat` / `.cmd` would run
//! through `cmd.exe`, where values passed on from `envTemplates` can inject
//! commands.
//!
//! `envTemplates` values carry App Directory data (the URL and title of the
//! app being launched): plugin binaries must treat them as untrusted input.
//!
//! # Template variables
//!
//! The following placeholders are substituted in `envTemplates.value` at launch:
//! - `{{url}}` — the URL to load
//! - `{{title}}` — the window title
//! - `{{app_id}}` — the FDC3 app ID
//! - `{{runtime_path}}` — absolute path to the installed runtime folder, if any

use std::env::consts::EXE_SUFFIX;
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

        // Windows: `<name>.exe` first, so a stray extensionless file of the
        // same name can't shadow it.
        let mut candidates = Vec::with_capacity(2);
        if requires_exe() && !has_exe_suffix(binary_name) {
            candidates.push(dir.join(format!("{binary_name}{EXE_SUFFIX}")));
        }
        candidates.push(dir.join(binary_name));
        for candidate in candidates {
            let Ok(real) = candidate.canonicalize() else {
                continue;
            };
            // Containment is checked on the canonical (on Windows: `\\?\`
            // verbatim) forms of both paths, which compare consistently.
            if !real.starts_with(&dir) {
                return Err(format!(
                    "plugin '{}': binaryName '{binary_name}' resolves outside the plugin's folder",
                    self.family
                ));
            }
            if real.is_dir() {
                return Err(format!(
                    "plugin '{}': binaryName '{binary_name}' is a directory, not an executable \
                     (on macOS, point it at a plain executable, not an .app bundle)",
                    self.family
                ));
            }
            // Windows runs `.bat` / `.cmd` through `cmd.exe`, where values a
            // plugin passes on from `envTemplates` (an App Directory title, a
            // URL) can inject commands; `.ps1`, `.lnk` and others don't run
            // as programs at all. Only a real `.exe` is launched. Checked on
            // the canonical file, so a link named `x.exe` to a `.cmd` fails.
            if requires_exe() && !has_exe_suffix(&real.to_string_lossy()) {
                return Err(format!(
                    "plugin '{}': binaryName '{binary_name}' must be an .exe on Windows",
                    self.family
                ));
            }
            if real.is_file() {
                return Ok(simplify_verbatim(real));
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
    ///
    /// One pass over `template`: an inserted value is never scanned again, so
    /// a title or URL that itself contains `{{runtime_path}}` stays literal.
    /// Unknown placeholders are left as written.
    pub fn expand_template(
        template: &str,
        url: &str,
        title: &str,
        app_id: &str,
        runtime_path: &str,
    ) -> String {
        let mut out = String::with_capacity(template.len());
        let mut rest = template;
        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            let value = after.find("}}").and_then(|end| {
                let v = match &after[..end] {
                    "url" => url,
                    "title" => title,
                    "app_id" => app_id,
                    "runtime_path" => runtime_path,
                    _ => return None,
                };
                Some((v, end))
            });
            match value {
                Some((v, end)) => {
                    out.push_str(v);
                    rest = &after[end + 2..];
                }
                None => {
                    out.push_str("{{");
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

/// Path where plugin manifests live inside `root`.
pub fn plugins_dir(root: &Path) -> PathBuf {
    root.join("plugins")
}

/// Plugin binaries must be `.exe` files on Windows (see `resolve_binary`).
const fn requires_exe() -> bool {
    !EXE_SUFFIX.is_empty()
}

fn has_exe_suffix(name: &str) -> bool {
    name.len() >= EXE_SUFFIX.len()
        && name[name.len() - EXE_SUFFIX.len()..].eq_ignore_ascii_case(EXE_SUFFIX)
}

/// Strip Windows' `\\?\` verbatim prefix from a canonical path when the
/// plain form means the same thing (`\\?\C:\…` → `C:\…`, under 260
/// characters). Child processes see this path as their program path and
/// working directory, and Chromium/CEF hosts and `cmd.exe` mishandle verbatim
/// ones. UNC (`\\?\UNC\…`) and long paths are kept as they are.
fn simplify_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(strip_verbatim_drive) {
        Some(plain) => PathBuf::from(plain),
        None => path,
    }
}

/// `r"\\?\C:\x"` → `Some(r"C:\x")`; `None` for anything else.
fn strip_verbatim_drive(s: &str) -> Option<&str> {
    let rest = s.strip_prefix(r"\\?\")?;
    let b = rest.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\';
    (drive && rest.len() < 260).then_some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A fresh `<tmp>/ot-core-plugin-<pid>-<n>/plugins/demo/` folder,
    /// removed (with its root) when dropped.
    struct PluginDir {
        root: PathBuf,
        dir: PathBuf,
    }

    impl std::ops::Deref for PluginDir {
        type Target = Path;
        fn deref(&self) -> &Path {
            &self.dir
        }
    }

    impl Drop for PluginDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn plugin_dir() -> PluginDir {
        static N: AtomicU32 = AtomicU32::new(0);
        let root = std::env::temp_dir().join(format!(
            "ot-core-plugin-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let dir = root.join("plugins").join("demo");
        std::fs::create_dir_all(&dir).unwrap();
        PluginDir { root, dir }
    }

    /// The file name a test plugin binary has on this OS.
    fn exe(name: &str) -> String {
        format!("{name}{EXE_SUFFIX}")
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
        assert_eq!(m.dir.as_path(), &*dir);
        let root = dir.parent().unwrap().parent().unwrap();
        let scanned = EngineManifest::scan_plugins(root);
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].dir.as_path(), &*dir);
    }

    #[test]
    fn resolves_a_binary_in_the_plugin_folder() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "demo-host");
        std::fs::write(dir.join(exe("demo-host")), b"").unwrap();
        let resolved = m.resolve_binary("demo-host").unwrap();
        assert!(resolved.is_absolute());
        assert_eq!(
            resolved,
            simplify_verbatim(dir.canonicalize().unwrap().join(exe("demo-host")))
        );
        // Never the `\\?\` form on Windows.
        assert!(!resolved.to_string_lossy().starts_with(r"\\?\"));
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
        let err = m.resolve_binary("bin").unwrap_err();
        assert!(err.contains("is a directory"), "{err}");
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

    #[cfg(windows)]
    #[test]
    fn windows_prefers_exe_over_an_extensionless_file() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "demo-host");
        std::fs::write(dir.join("demo-host"), b"").unwrap();
        std::fs::write(dir.join("demo-host.exe"), b"").unwrap();
        assert!(m
            .resolve_binary("demo-host")
            .unwrap()
            .ends_with("demo-host.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_refuses_scripts_and_other_non_exe_files() {
        let dir = plugin_dir();
        let m = write_manifest(&dir, "x");
        for name in [
            "host.cmd", "host.bat", "host.CMD", "host.ps1", "host.com", "host",
        ] {
            std::fs::write(dir.join(name), b"").unwrap();
            let err = m.resolve_binary(name).expect_err(name);
            assert!(err.contains("must be an .exe"), "{name}: {err}");
        }
        // Case-insensitive `.exe` is fine.
        std::fs::write(dir.join("HOST2.EXE"), b"").unwrap();
        assert!(m.resolve_binary("HOST2.EXE").is_ok());
    }

    #[test]
    fn verbatim_drive_paths_are_simplified_and_others_kept() {
        assert_eq!(
            strip_verbatim_drive(r"\\?\C:\x\host.exe"),
            Some(r"C:\x\host.exe")
        );
        assert_eq!(strip_verbatim_drive(r"\\?\UNC\srv\share\host.exe"), None);
        assert_eq!(strip_verbatim_drive(r"C:\x\host.exe"), None);
        assert_eq!(strip_verbatim_drive("/usr/bin/x"), None);
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(strip_verbatim_drive(&long), None);
    }

    #[test]
    fn exe_suffix_check_ignores_case() {
        if requires_exe() {
            assert!(has_exe_suffix("a.exe") && has_exe_suffix("a.EXE"));
            assert!(!has_exe_suffix("a.cmd") && !has_exe_suffix("exe"));
        } else {
            // No suffix on macOS / Linux: every name "has" it.
            assert!(has_exe_suffix("anything"));
        }
    }

    #[test]
    fn templates_expand_in_one_pass() {
        let out = EngineManifest::expand_template(
            "u={{url}} t={{title}} a={{app_id}} r={{runtime_path}} x={{other}} {{",
            "https://e.com/?q={{runtime_path}}",
            "{{app_id}}",
            "app",
            "SECRET",
        );
        assert_eq!(
            out,
            "u=https://e.com/?q={{runtime_path}} t={{app_id}} a=app r=SECRET x={{other}} {{"
        );
    }

    #[test]
    fn launch_mode_round_trips_in_camel_case() {
        let m: EngineManifest = serde_json::from_str(
            r#"{"family":"cef","label":"CEF","launchMode":{"type":"spawnBinary","binaryName":"cef-host","envTemplates":[{"key":"K","value":"V"}]}}"#,
        )
        .unwrap();
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["launchMode"]["binaryName"], "cef-host");
        assert_eq!(json["launchMode"]["envTemplates"][0]["key"], "K");
        assert!(json.get("dir").is_none());
        let back: EngineManifest = serde_json::from_value(json).unwrap();
        assert!(matches!(back.launch_mode, LaunchMode::SpawnBinary { .. }));
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
