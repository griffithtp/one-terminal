//! Engine availability & install for the window-manager.
//!
//! When the user picks an engine in the launch picker that doesn't match the
//! WM's pinned engine, we need to (a) confirm the runtime exists locally and
//! (b) download it on demand if it doesn't. CDA owns the canonical install
//! flow, but we replicate the slim subset here so the WM can launch external
//! windows without round-tripping through CDA.
//!
//! The on-disk layout (cache root, sentinel file, per-binding folder) is
//! identical to CDA's so a runtime installed by either side is reusable by
//! the other.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};

use ot_core::engine::{
    binding_path, current_os, is_installed as binding_is_installed, is_system_version, EngineBinding,
    EngineFamily, OsKey, INSTALL_SENTINEL,
};


// ── Catalog types (mirror app-directory's `/v2/engines` response) ─────────────

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDownload {
    pub url: String,
    pub sha256: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EngineEntry {
    pub family: EngineFamily,
    pub version: String,
    pub label: String,
    #[serde(default)]
    pub download: Option<EngineDownload>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct EngineCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub windows: Option<Vec<EngineEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos: Option<Vec<EngineEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linux: Option<Vec<EngineEntry>>,
}

impl EngineCatalog {
    fn entries_for(&self, os: OsKey) -> &[EngineEntry] {
        let bucket = match os {
            OsKey::Windows => self.windows.as_ref(),
            OsKey::Macos => self.macos.as_ref(),
            OsKey::Linux => self.linux.as_ref(),
        };
        bucket.map(|v| v.as_slice()).unwrap_or(&[])
    }

    fn find(&self, os: OsKey, binding: &EngineBinding) -> Option<&EngineEntry> {
        self.entries_for(os)
            .iter()
            .find(|e| e.family == binding.family && e.version == binding.version)
    }
}

#[derive(Deserialize)]
struct EngineCatalogResponse {
    engines: EngineCatalog,
}

async fn fetch_catalog(catalog_url: &str) -> Result<EngineCatalog, String> {
    let res = reqwest::get(catalog_url)
        .await
        .map_err(|e| format!("engine catalog fetch: {e}"))?;
    if !res.status().is_success() {
        return Err(format!("engine catalog HTTP {}", res.status()));
    }
    let body: EngineCatalogResponse = res
        .json()
        .await
        .map_err(|e| format!("engine catalog parse: {e}"))?;
    Ok(body.engines)
}

// ── Status ────────────────────────────────────────────────────────────────────

/// Result of `wm_engine_status`. Tells the frontend whether the engine is
/// usable right now, needs a download first, or simply isn't supported.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum EngineStatus {
    /// Engine is ready to launch immediately — system runtime or already
    /// installed locally.
    Ready {
        family: EngineFamily,
        version: String,
        /// Path on disk for fixed runtimes. Absent for system engines.
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
    },
    /// Engine isn't installed but the catalog has a download we can fetch.
    NeedsDownload {
        family: EngineFamily,
        version: String,
        label: String,
        size_bytes: u64,
    },
    /// Engine isn't in the catalog for this OS, or has no download — we
    /// can't help the user proceed without manual install.
    Unsupported {
        family: EngineFamily,
        version: String,
        message: String,
    },
}

pub async fn engine_status(binding: &EngineBinding, catalog_url: &str) -> EngineStatus {
    // Electron is an out-of-process host, not a downloadable webview runtime.
    // Readiness is determined by whether the electron-host shell + binary are
    // locatable, not by a cache sentinel or catalog entry.
    if binding.family == EngineFamily::Electron {
        return electron_status(binding);
    }

    if is_system_version(&binding.version) {
        return EngineStatus::Ready {
            family: binding.family.clone(),
            version: binding.version.clone(),
            path: None,
        };
    }

    let cache_root = ot_core::engine::default_cache_root();
    let dir = binding_path(&cache_root, binding);
    if binding_is_installed(&dir) {
        return EngineStatus::Ready {
            family: binding.family.clone(),
            version: binding.version.clone(),
            path: Some(dir),
        };
    }

    match fetch_catalog(catalog_url).await {
        Ok(catalog) => match catalog.find(current_os(), binding) {
            Some(entry) => match entry.download.as_ref() {
                Some(dl) => EngineStatus::NeedsDownload {
                    family: binding.family.clone(),
                    version: binding.version.clone(),
                    label: entry.label.clone(),
                    size_bytes: dl.size_bytes,
                },
                None => EngineStatus::Unsupported {
                    family: binding.family.clone(),
                    version: binding.version.clone(),
                    message: format!(
                        "{}@{} is not downloadable from this catalog",
                        binding.family.as_dir(),
                        binding.version
                    ),
                },
            },
            None => EngineStatus::Unsupported {
                family: binding.family.clone(),
                version: binding.version.clone(),
                message: format!(
                    "{}@{} is not in the engine catalog for this OS",
                    binding.family.as_dir(),
                    binding.version
                ),
            },
        },
        Err(e) => EngineStatus::Unsupported {
            family: binding.family.clone(),
            version: binding.version.clone(),
            message: e,
        },
    }
}

fn electron_status(binding: &EngineBinding) -> EngineStatus {
    let shell = match ot_core::electron_host::locate_electron_shell() {
        Ok(p) => p,
        Err(e) => {
            return EngineStatus::Unsupported {
                family: binding.family.clone(),
                version: binding.version.clone(),
                message: e,
            }
        }
    };
    let cache_root = ot_core::engine::default_cache_root();
    match ot_core::electron_host::locate_electron_binary(binding, &cache_root, &shell) {
        Ok(bin) => EngineStatus::Ready {
            family: binding.family.clone(),
            version: binding.version.clone(),
            path: Some(bin),
        },
        Err(e) => EngineStatus::Unsupported {
            family: binding.family.clone(),
            version: binding.version.clone(),
            message: e,
        },
    }
}

// ── Install ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadEvent<'a> {
    family: EngineFamily,
    version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    total: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    downloaded: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

/// Download + verify + extract the runtime for `binding` and write the
/// install sentinel. Idempotent: a second call is a no-op once the sentinel
/// exists. Emits `engine:download:start|progress|complete|error` so the
/// frontend can render a progress bar.
pub async fn engine_install(binding: &EngineBinding, app: &AppHandle, catalog_url: &str) -> Result<PathBuf, String> {
    if binding.family == EngineFamily::Electron {
        return Err(
            "Electron uses a local npm binary, not the download catalog. \
             Run `npm run setup:electron-host` to install it."
                .to_string(),
        );
    }

    if is_system_version(&binding.version) {
        // System engines don't have a folder; the caller should have already
        // resolved them as Ready in status. Returning Err keeps the flow
        // honest if a frontend bug routes them here.
        return Err("system engines do not need installation".to_string());
    }

    let cache_root = ot_core::engine::default_cache_root();
    let dir = binding_path(&cache_root, binding);
    if binding_is_installed(&dir) {
        return Ok(dir);
    }

    let catalog = fetch_catalog(catalog_url).await?;
    let entry = catalog
        .find(current_os(), binding)
        .ok_or_else(|| {
            format!(
                "{}@{} is not in the engine catalog",
                binding.family.as_dir(),
                binding.version
            )
        })?;
    let download = entry.download.as_ref().ok_or_else(|| {
        format!(
            "{}@{} has no download in the catalog",
            binding.family.as_dir(),
            binding.version
        )
    })?;

    fs::create_dir_all(&dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;

    let _ = app.emit(
        "engine:download:start",
        DownloadEvent {
            family: binding.family.clone(),
            version: &binding.version,
            total: Some(download.size_bytes),
            downloaded: Some(0),
            path: None,
            message: None,
        },
    );

    let result = install_inner(binding, download, &dir, app).await;

    match &result {
        Ok(_) => {
            let _ = app.emit(
                "engine:download:complete",
                DownloadEvent {
                    family: binding.family.clone(),
                    version: &binding.version,
                    total: Some(download.size_bytes),
                    downloaded: Some(download.size_bytes),
                    path: Some(dir.display().to_string()),
                    message: None,
                },
            );
        }
        Err(e) => {
            // Wipe the partial install so the next attempt starts clean.
            let _ = fs::remove_dir_all(&dir);
            let _ = app.emit(
                "engine:download:error",
                DownloadEvent {
                    family: binding.family.clone(),
                    version: &binding.version,
                    total: None,
                    downloaded: None,
                    path: None,
                    message: Some(e.clone()),
                },
            );
        }
    }

    result.map(|_| dir)
}

async fn install_inner(
    binding: &EngineBinding,
    download: &EngineDownload,
    dir: &Path,
    app: &AppHandle,
) -> Result<(), String> {
    let tmp_path = dir.with_extension("partial.zip");
    let mut hasher = Sha256::new();

    {
        let response = reqwest::get(&download.url)
            .await
            .map_err(|e| format!("download: {e}"))?;
        if !response.status().is_success() {
            return Err(format!("download HTTP {}", response.status()));
        }
        let total = response.content_length().or(Some(download.size_bytes));
        let mut file = fs::File::create(&tmp_path).map_err(|e| format!("create temp: {e}"))?;
        let mut downloaded: u64 = 0;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| format!("stream: {e}"))?;
            hasher.update(&bytes);
            file.write_all(&bytes).map_err(|e| format!("write: {e}"))?;
            downloaded += bytes.len() as u64;
            let _ = app.emit(
                "engine:download:progress",
                DownloadEvent {
                    family: binding.family.clone(),
                    version: &binding.version,
                    total,
                    downloaded: Some(downloaded),
                    path: None,
                    message: None,
                },
            );
        }
    }

    let actual = hex::encode(hasher.finalize());
    if !actual.eq_ignore_ascii_case(&download.sha256) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!(
            "sha256 mismatch: expected {}, got {}",
            download.sha256, actual
        ));
    }

    extract_zip(&tmp_path, dir).map_err(|e| format!("extract: {e}"))?;
    let _ = fs::remove_file(&tmp_path);

    fs::write(dir.join(INSTALL_SENTINEL), b"ok\n")
        .map_err(|e| format!("sentinel: {e}"))?;
    Ok(())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let rel = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let target = dest.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&target).map_err(|e| e.to_string())?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        out.write_all(&buf).map_err(|e| e.to_string())?;
    }
    Ok(())
}

