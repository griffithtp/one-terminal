//! Engine identity for this window-manager process.
//!
//! Each WM host is pinned to a single browser engine at startup (because the
//! underlying webview runtime — WebView2 fixed-runtime on Windows, WKWebView
//! on macOS — is selected per-process, not per-webview). The identity is
//! derived from environment variables set by the spawner, or defaults to the
//! OS-native system engine when running standalone.

use std::path::PathBuf;

use ot_core::engine::{
    binding_path, default_cache_root, is_installed, is_system_version, EngineBinding,
    EngineFamily,
};

/// The engine this WM process is running under, plus where its fixed runtime
/// lives on disk (if any). `path == None` means "system engine — nothing
/// special is bundled".
#[derive(Clone, Debug)]
pub struct WmHostIdentity {
    pub binding: EngineBinding,
    pub runtime_path: Option<PathBuf>,
    pub cache_root: PathBuf,
}

impl WmHostIdentity {
    /// Build the identity from environment variables. Falls back to the
    /// OS-native system engine if `OT_ENGINE_FAMILY` / `OT_ENGINE_VERSION`
    /// are unset, which is the normal case for a standalone launch.
    pub fn from_env() -> Self {
        let cache_root = default_cache_root();

        let family = std::env::var("OT_ENGINE_FAMILY")
            .ok()
            .map(|f| EngineFamily::parse(&f));
        let version = std::env::var("OT_ENGINE_VERSION").ok();

        let binding = match (family, version) {
            (Some(family), Some(version)) => EngineBinding { family, version },
            _ => EngineBinding::os_default(),
        };

        let runtime_path = if is_system_version(&binding.version) {
            None
        } else if let Some(dev_path) = dev_runtime_override() {
            eprintln!(
                "[wm] using OT_ENGINE_RUNTIME_OVERRIDE={} for {}@{}",
                dev_path.display(),
                binding.family.as_dir(),
                binding.version,
            );
            Some(dev_path)
        } else {
            let dir = binding_path(&cache_root, &binding);
            if is_installed(&dir) {
                Some(dir)
            } else {
                // Spawner promised a runtime but it's missing. We leave path
                // as None and the webview falls back to the system engine —
                // safer than crashing, but worth a loud log so it's noticed.
                eprintln!(
                    "[wm] engine {}@{} missing at {} — falling back to system",
                    binding.family.as_dir(),
                    binding.version,
                    dir.display(),
                );
                None
            }
        };

        Self {
            binding,
            runtime_path,
            cache_root,
        }
    }

    /// True when an incoming `engine_binding` matches what this process can
    /// run locally. A `None` (no preference) always matches.
    pub fn matches(&self, other: Option<&EngineBinding>) -> bool {
        match other {
            None => true,
            Some(b) => b == &self.binding,
        }
    }
}

/// Debug-only: return `OT_ENGINE_RUNTIME_OVERRIDE` if set and pointing at an
/// existing directory. Lets developers aim a WM host at a locally-unzipped
/// WebView2 folder without going through the catalog or install sentinel.
#[cfg(debug_assertions)]
fn dev_runtime_override() -> Option<PathBuf> {
    let raw = std::env::var("OT_ENGINE_RUNTIME_OVERRIDE").ok()?;
    let path = PathBuf::from(raw);
    if path.is_dir() {
        Some(path)
    } else {
        eprintln!(
            "[wm] OT_ENGINE_RUNTIME_OVERRIDE={} is not a directory; ignoring",
            path.display()
        );
        None
    }
}

#[cfg(not(debug_assertions))]
fn dev_runtime_override() -> Option<PathBuf> {
    None
}
