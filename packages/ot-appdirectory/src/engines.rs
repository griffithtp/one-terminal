//! Engine catalog served at `GET /v2/engines`. Ported from
//! `apps/app-directory/src/engines.ts`.

use ot_core::engine::{EngineFamily, OsKey};

use crate::types::{EngineBinding, EngineCatalog, EngineDownload, EngineEntry};

pub fn catalog() -> EngineCatalog {
    EngineCatalog {
        windows: Some(vec![
            EngineEntry {
                family: EngineFamily::Webview2,
                version: "system".into(),
                label: "WebView2 (System Evergreen)".into(),
                download: None,
            },
            EngineEntry {
                family: EngineFamily::Webview2,
                version: "124.0.2478.97".into(),
                label: "WebView2 124 (Fixed Runtime)".into(),
                download: Some(EngineDownload {
                    url: "https://engines.internal/webview2/124.0.2478.97/webview2-x64.zip".into(),
                    sha256: "0".repeat(64),
                    size_bytes: 180_000_000,
                }),
            },
            EngineEntry {
                family: EngineFamily::Electron,
                version: "29.3.0".into(),
                label: "Electron 29 (Chromium 122)".into(),
                download: Some(EngineDownload {
                    url: "https://engines.internal/electron/29.3.0/electron-win32-x64.zip".into(),
                    sha256: "0".repeat(64),
                    size_bytes: 220_000_000,
                }),
            },
        ]),
        macos: Some(vec![
            EngineEntry {
                family: EngineFamily::Wkwebview,
                version: "system".into(),
                label: "WKWebView (System WebKit)".into(),
                download: None,
            },
            EngineEntry {
                family: EngineFamily::Electron,
                version: "29.3.0".into(),
                label: "Electron 29 (Chromium 122)".into(),
                download: Some(EngineDownload {
                    url: "https://engines.internal/electron/29.3.0/electron-darwin-arm64.zip"
                        .into(),
                    sha256: "0".repeat(64),
                    size_bytes: 220_000_000,
                }),
            },
        ]),
        linux: None,
    }
}

fn entries_for(cat: &EngineCatalog, os: OsKey) -> &[EngineEntry] {
    match os {
        OsKey::Windows => cat.windows.as_deref(),
        OsKey::Macos => cat.macos.as_deref(),
        OsKey::Linux => cat.linux.as_deref(),
    }
    .unwrap_or(&[])
}

/// True if `(family, version)` is present in the catalog for `os`.
pub fn is_engine_binding_valid(cat: &EngineCatalog, os: OsKey, binding: &EngineBinding) -> bool {
    entries_for(cat, os)
        .iter()
        .any(|e| e.family == binding.family && e.version == binding.version)
}
