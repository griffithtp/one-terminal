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

/// Path of the installed runtime folder for `(family, version)` inside `root`.
pub fn binding_path(root: &Path, binding: &EngineBinding) -> PathBuf {
    root.join(binding.family.as_dir()).join(&binding.version)
}

/// Sentinel file marker written after a successful install.
pub const INSTALL_SENTINEL: &str = ".installed";

/// True if `dir` holds a complete, usable engine install.
pub fn is_installed(dir: &Path) -> bool {
    dir.join(INSTALL_SENTINEL).is_file()
}
