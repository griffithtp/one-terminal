//! Pure engine-binding types shared by CDA, the window-manager, and any
//! future host app. Everything here is tauri-free so a plain library crate
//! can depend on it without the whole Tauri stack.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── OS + family enums ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OsKey {
    Windows,
    Macos,
    Linux,
}

impl OsKey {
    pub fn as_dir(self) -> &'static str {
        match self {
            OsKey::Windows => "windows",
            OsKey::Macos => "macos",
            OsKey::Linux => "linux",
        }
    }
}

/// Browser engine family. The three built-in variants are recognised by name;
/// any other string deserializes into `Custom(String)` so new engines declared
/// in plugin manifests or the App Directory don't require a code change.
///
/// `Copy` is intentionally NOT derived: `Custom` holds an owned `String`.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum EngineFamily {
    Webview2,
    Wkwebview,
    Electron,
    /// A manifest-declared engine identified by its family name string.
    Custom(String),
}

impl EngineFamily {
    /// Canonical directory/path component for this family (lowercased name).
    pub fn as_str(&self) -> &str {
        match self {
            EngineFamily::Webview2 => "webview2",
            EngineFamily::Wkwebview => "wkwebview",
            EngineFamily::Electron => "electron",
            EngineFamily::Custom(s) => s.as_str(),
        }
    }

    /// Alias for [`as_str`] — kept for call-site compatibility.
    pub fn as_dir(&self) -> &str {
        self.as_str()
    }

    /// Parse a family name. Always succeeds — unknown names become `Custom`.
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "webview2" => EngineFamily::Webview2,
            "wkwebview" => EngineFamily::Wkwebview,
            "electron" => EngineFamily::Electron,
            _ => EngineFamily::Custom(s.to_string()),
        }
    }

    /// True for the three built-in families.
    pub fn is_builtin(&self) -> bool {
        !matches!(self, EngineFamily::Custom(_))
    }
}

impl std::fmt::Display for EngineFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for EngineFamily {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EngineFamily {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Ok(EngineFamily::parse(&s))
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct EngineBinding {
    pub family: EngineFamily,
    pub version: String,
}

impl EngineBinding {
    /// The default engine for the host OS — the OS-native webview at its
    /// system version. Used when env vars don't pin a specific runtime.
    pub fn os_default() -> Self {
        let family = if cfg!(target_os = "windows") {
            EngineFamily::Webview2
        } else {
            // Linux's WebKitGTK is close enough to "wkwebview" for the
            // binding-equality check; true Linux support is out-of-scope.
            EngineFamily::Wkwebview
        };
        Self {
            family,
            version: "system".into(),
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub fn current_os() -> OsKey {
    if cfg!(target_os = "windows") {
        OsKey::Windows
    } else if cfg!(target_os = "macos") {
        OsKey::Macos
    } else {
        OsKey::Linux
    }
}

pub fn is_system_version(version: &str) -> bool {
    version.eq_ignore_ascii_case("system")
}

/// Shared engine cache root — both the desktop agent and Terminal resolve to the same directory
/// so an engine installed by the standalone launcher is usable by a tab
/// opened later in the Terminal. Respects `OT_ENGINE_CACHE_ROOT` for
/// overrides (dev mode, tests).
pub fn default_cache_root() -> PathBuf {
    if let Ok(custom) = std::env::var("OT_ENGINE_CACHE_ROOT") {
        return PathBuf::from(custom);
    }

    let product = "one-terminal";

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(product)
                .join("engines");
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join(product).join("engines");
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join(product).join("engines");
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join(".local")
                .join("share")
                .join(product)
                .join("engines");
        }
    }

    // Last resort: working directory. Don't silently fail.
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".ot-engine-cache")
}

/// Folders directly under the engine cache root that the runtime store must
/// never install into: `plugins/` holds engine plugin manifests and binaries
/// (`ot_core::plugin`), which run with the user's rights.
pub const RESERVED_CACHE_DIRS: &[&str] = &["plugins"];

/// Whether `s` is safe as one folder name under the engine cache: ASCII
/// letters, digits, `.`, `_`, `-`; not `.` / `..`; not a Windows device name
/// (`CON`, `NUL`, `COM1`, …, with or without an extension).
fn is_safe_component(s: &str) -> bool {
    if s.is_empty() || s == "." || s == ".." || s.len() > 64 {
        return false;
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return false;
    }
    let stem = s.split('.').next().unwrap_or(s).to_ascii_uppercase();
    let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit());
    !device
}

/// Refuse a binding whose family or version can't be used as a folder name
/// under the engine cache (a path separator, `..`, …) or would land in a
/// reserved folder such as `plugins/`. Engine bindings come from the App
/// Directory and the engine catalog, so installers check this before
/// writing anything.
pub fn check_binding(binding: &EngineBinding) -> Result<(), String> {
    let family = binding.family.as_dir();
    if !is_safe_component(family)
        || RESERVED_CACHE_DIRS
            .iter()
            .any(|r| r.eq_ignore_ascii_case(family))
    {
        return Err(format!(
            "engine family '{family}' is not a valid engine name"
        ));
    }
    if !is_safe_component(&binding.version) {
        return Err(format!(
            "engine version '{}' is not a valid version",
            binding.version
        ));
    }
    Ok(())
}

/// `s` if it is a safe folder name, else a harmless stand-in (`_` plus `s`
/// with unsafe characters replaced), so a bad binding can never point
/// outside its own folder under the cache.
fn cache_component(s: &str, is_family: bool) -> std::borrow::Cow<'_, str> {
    let reserved = is_family
        && RESERVED_CACHE_DIRS
            .iter()
            .any(|r| r.eq_ignore_ascii_case(s));
    if is_safe_component(s) && !reserved {
        return std::borrow::Cow::Borrowed(s);
    }
    let cleaned: String = s
        .chars()
        .take(64)
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect();
    std::borrow::Cow::Owned(format!("_{cleaned}"))
}

/// Path of the installed runtime folder for `(family, version)` inside `root`.
///
/// Always `root/<one folder>/<one folder>`: a family or version that fails
/// [`check_binding`] is mapped to a `_`-prefixed stand-in rather than joined
/// as given, so it can't climb out of `root` or reach `plugins/`. Installers
/// still call [`check_binding`] first to refuse such bindings outright.
pub fn binding_path(root: &Path, binding: &EngineBinding) -> PathBuf {
    root.join(cache_component(binding.family.as_dir(), true).as_ref())
        .join(cache_component(&binding.version, false).as_ref())
}

/// Sentinel file marker written after a successful install.
pub const INSTALL_SENTINEL: &str = ".installed";

/// True if `dir` holds a complete, usable engine install.
pub fn is_installed(dir: &Path) -> bool {
    dir.join(INSTALL_SENTINEL).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(family: &str, version: &str) -> EngineBinding {
        EngineBinding {
            family: EngineFamily::parse(family),
            version: version.into(),
        }
    }

    #[test]
    fn ordinary_bindings_pass_and_map_one_to_one() {
        let root = Path::new("/cache");
        for (f, v) in [
            ("webview2", "130.0.2849.80"),
            ("electron", "33.2.1"),
            ("cef", "1_2-3"),
        ] {
            assert!(check_binding(&b(f, v)).is_ok(), "{f}/{v}");
            assert_eq!(binding_path(root, &b(f, v)), root.join(f).join(v));
        }
    }

    #[test]
    fn unsafe_or_reserved_bindings_are_refused() {
        for (f, v) in [
            ("plugins", "1"),
            ("PLUGINS", "1"),
            ("cef", "../x"),
            ("cef", ".."),
            ("cef", "."),
            ("cef", ""),
            ("cef", "a/b"),
            ("cef", "a\\b"),
            ("cef", "C:x"),
            ("..", "1"),
            ("a/b", "1"),
            ("con", "1"),
            ("cef", "NUL.txt"),
            ("cef", "com1"),
            ("cef", "1 2"),
        ] {
            assert!(check_binding(&b(f, v)).is_err(), "{f}/{v}");
        }
    }

    #[test]
    fn unsafe_bindings_still_map_inside_their_own_folder() {
        let root = Path::new("/cache");
        for (f, v) in [
            ("plugins", "evil"),
            ("cef", "../../x"),
            ("..", ".."),
            ("a/b", "c\\d"),
        ] {
            let p = binding_path(root, &b(f, v));
            let rel = p.strip_prefix(root).expect("inside root");
            let parts: Vec<_> = rel.components().collect();
            assert_eq!(parts.len(), 2, "{f}/{v} -> {}", p.display());
            for c in &parts {
                assert!(
                    matches!(c, std::path::Component::Normal(_)),
                    "{f}/{v} -> {}",
                    p.display()
                );
            }
            assert_ne!(
                parts[0].as_os_str().to_string_lossy().to_ascii_lowercase(),
                "plugins"
            );
        }
    }
}
