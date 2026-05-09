//! Engine catalog and runtime cache.
//!
//! The App Directory tells us *which* browser engine and version each app
//! wants. This module owns (a) the in-memory catalog of engine builds the
//! launcher knows how to install, and (b) the on-disk cache of actually-
//! installed runtime folders under `<app_data>/engines/<family>/<version>/`.

pub mod catalog;
pub mod commands;
pub mod router;
pub mod runtime;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub use catalog::EngineCatalogClient;
pub use ot_core::plugin::EngineManifest;
pub use runtime::EngineRuntimeStore;

// Re-export shared types so existing `engines::OsKey`, `engines::EngineBinding`
// call sites keep working.
pub use ot_core::engine::{current_os, is_system_version, EngineBinding, EngineFamily, OsKey};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDownload {
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineEntry {
    pub family: EngineFamily,
    pub version: String,
    pub label: String,
    #[serde(default)]
    pub download: Option<EngineDownload>,
}

/// Full catalog keyed by OS. Mirrors the server-side
/// `EngineCatalog = Partial<Record<OsKey, EngineEntry[]>>`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EngineCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<Vec<EngineEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<Vec<EngineEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<Vec<EngineEntry>>,
}

impl EngineCatalog {
    pub fn entries_for(&self, os: OsKey) -> &[EngineEntry] {
        let bucket = match os {
            OsKey::Windows => self.windows.as_ref(),
            OsKey::Macos => self.macos.as_ref(),
            OsKey::Linux => self.linux.as_ref(),
        };
        bucket.map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn find(&self, os: OsKey, binding: &EngineBinding) -> Option<&EngineEntry> {
        self.entries_for(os)
            .iter()
            .find(|e| e.family == binding.family && e.version == binding.version)
    }
}

/// The result of `engine_ensure` / `list_installed`. `path == None` means the
/// engine is the OS-default (system WebView2/WKWebView) — nothing to install.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledEngine {
    pub family: EngineFamily,
    pub version: String,
    pub path: Option<PathBuf>,
    pub system: bool,
}
