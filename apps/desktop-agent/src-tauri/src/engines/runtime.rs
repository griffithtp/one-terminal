//! On-disk cache of browser-engine runtime folders.
//!
//! Layout:
//!     <root>/<family>/<version>/
//!         .installed     ← sentinel; its presence means this engine is usable.
//!         ...            ← the unzipped contents of the download archive.
//!
//! `ensure()` is idempotent — it only downloads when the sentinel is absent —
//! and serialises concurrent installs of the same `(family, version)` so two
//! launch requests can't race on the filesystem.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

use super::{
    current_os, is_system_version, EngineBinding, EngineCatalog, EngineEntry, EngineFamily,
    InstalledEngine,
};
use ot_core::engine::{binding_path, is_installed as binding_is_installed, INSTALL_SENTINEL};

#[derive(Clone)]
pub struct EngineRuntimeStore {
    root: PathBuf,
    locks: Arc<DashMap<EngineBinding, Arc<Mutex<()>>>>,
    /// Plugin manifests loaded from `<root>/plugins/*/manifest.json` at init.
    plugins: Arc<Vec<ot_core::plugin::EngineManifest>>,
}

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

impl EngineRuntimeStore {
    /// Create the store, scanning `<root>/plugins/` for manifests.
    pub fn new(root: PathBuf) -> Self {
        let plugins = ot_core::plugin::EngineManifest::scan_plugins(&root);
        if !plugins.is_empty() {
            println!("[cda] {} engine plugin(s) loaded", plugins.len());
        }
        Self {
            root,
            locks: Arc::new(DashMap::new()),
            plugins: Arc::new(plugins),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Plugin manifests loaded at init — pass to [`plan_launch`] to enable
    /// custom engine families.
    pub fn plugins(&self) -> &[ot_core::plugin::EngineManifest] {
        &self.plugins
    }

    fn path_for(&self, binding: &EngineBinding) -> PathBuf {
        binding_path(&self.root, binding)
    }

    fn is_installed(dir: &Path) -> bool {
        binding_is_installed(dir)
    }

    /// Scan the cache directory and return every engine that has a sentinel.
    ///
    /// Scans all three built-in family directories plus any plugin families
    /// declared in loaded manifests.
    pub fn list_installed(&self) -> Vec<InstalledEngine> {
        let mut families: Vec<EngineFamily> = vec![
            EngineFamily::Webview2,
            EngineFamily::Wkwebview,
            EngineFamily::Electron,
        ];
        for m in self.plugins.iter() {
            families.push(EngineFamily::Custom(m.family.clone()));
        }

        let mut out = Vec::new();
        for family in &families {
            let family_dir = self.root.join(family.as_dir());
            let Ok(entries) = fs::read_dir(&family_dir) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() || !Self::is_installed(&path) {
                    continue;
                }
                let version = match path.file_name().and_then(|s| s.to_str()) {
                    Some(v) => v.to_string(),
                    None => continue,
                };
                out.push(InstalledEngine {
                    family: family.clone(),
                    version,
                    path: Some(path),
                    system: false,
                });
            }
        }
        out
    }

    /// Resolve a binding to an installed runtime, downloading it if needed.
    ///
    /// For `version == "system"` this is a no-op and returns `path: None` —
    /// the caller should fall back to the OS-default runtime.
    pub async fn ensure(
        &self,
        binding: &EngineBinding,
        catalog: &EngineCatalog,
        app: &AppHandle,
    ) -> Result<InstalledEngine, String> {
        // Electron is resolved via the electron-host launcher, not the catalog.
        if binding.family == EngineFamily::Electron {
            let shell = ot_core::electron_host::locate_electron_shell()?;
            let bin =
                ot_core::electron_host::locate_electron_binary(binding, &self.root, &shell)?;
            return Ok(InstalledEngine {
                family: binding.family.clone(),
                version: binding.version.clone(),
                path: Some(bin),
                system: false,
            });
        }

        if is_system_version(&binding.version) {
            return Ok(InstalledEngine {
                family: binding.family.clone(),
                version: binding.version.clone(),
                path: None,
                system: true,
            });
        }

        // Debug-only dev override: if OT_ENGINE_RUNTIME_OVERRIDE points at an
        // existing dir, short-circuit download and use it as the runtime.
        if let Some(dev_path) = dev_runtime_override() {
            eprintln!(
                "[cda] OT_ENGINE_RUNTIME_OVERRIDE={} → skipping download for {}@{}",
                dev_path.display(),
                binding.family.as_dir(),
                binding.version,
            );
            return Ok(InstalledEngine {
                family: binding.family.clone(),
                version: binding.version.clone(),
                path: Some(dev_path),
                system: false,
            });
        }

        let dir = self.path_for(binding);
        if Self::is_installed(&dir) {
            return Ok(InstalledEngine {
                family: binding.family.clone(),
                version: binding.version.clone(),
                path: Some(dir),
                system: false,
            });
        }

        // Serialise concurrent installs of the same binding.
        let lock = self
            .locks
            .entry(binding.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;

        // Re-check after acquiring the lock — another task may have finished.
        if Self::is_installed(&dir) {
            return Ok(InstalledEngine {
                family: binding.family.clone(),
                version: binding.version.clone(),
                path: Some(dir),
                system: false,
            });
        }

        let entry = catalog
            .find(current_os(), binding)
            .ok_or_else(|| {
                format!(
                    "Engine {}@{} not in catalog for this OS",
                    binding.family.as_dir(),
                    binding.version
                )
            })?;
        let download = entry.download.as_ref().ok_or_else(|| {
            format!(
                "Engine {}@{} has no download in catalog",
                binding.family.as_dir(),
                binding.version
            )
        })?;

        self.install_from_download(binding, entry, download, &dir, app)
            .await
            .map_err(|e| {
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
                // Leave a partial install cleaned up; next call will retry.
                let _ = fs::remove_dir_all(&dir);
                e
            })?;

        Ok(InstalledEngine {
            family: binding.family.clone(),
            version: binding.version.clone(),
            path: Some(dir),
            system: false,
        })
    }

    async fn install_from_download(
        &self,
        binding: &EngineBinding,
        _entry: &EngineEntry,
        download: &super::EngineDownload,
        dir: &Path,
        app: &AppHandle,
    ) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;

        app.emit(
            "engine:download:start",
            DownloadEvent {
                family: binding.family.clone(),
                version: &binding.version,
                total: Some(download.size_bytes),
                downloaded: Some(0),
                path: None,
                message: None,
            },
        )
        .ok();

        // Stream the archive into a sibling temp file, hashing as we go.
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
            let mut file =
                fs::File::create(&tmp_path).map_err(|e| format!("create temp: {e}"))?;
            let mut downloaded: u64 = 0;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let bytes = chunk.map_err(|e| format!("stream: {e}"))?;
                hasher.update(&bytes);
                file.write_all(&bytes).map_err(|e| format!("write: {e}"))?;
                downloaded += bytes.len() as u64;
                app.emit(
                    "engine:download:progress",
                    DownloadEvent {
                        family: binding.family.clone(),
                        version: &binding.version,
                        total,
                        downloaded: Some(downloaded),
                        path: None,
                        message: None,
                    },
                )
                .ok();
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
        fs::remove_file(&tmp_path).ok();

        fs::write(dir.join(INSTALL_SENTINEL), b"ok\n")
            .map_err(|e| format!("sentinel: {e}"))?;

        app.emit(
            "engine:download:complete",
            DownloadEvent {
                family: binding.family.clone(),
                version: &binding.version,
                total: Some(download.size_bytes),
                downloaded: Some(download.size_bytes),
                path: Some(dir.display().to_string()),
                message: None,
            },
        )
        .ok();

        Ok(())
    }
}

/// Debug-only: return `OT_ENGINE_RUNTIME_OVERRIDE` if it's set to an existing
/// directory. Lets developers skip the download/sha256/unzip path entirely.
#[cfg(debug_assertions)]
fn dev_runtime_override() -> Option<PathBuf> {
    let raw = std::env::var("OT_ENGINE_RUNTIME_OVERRIDE").ok()?;
    let path = PathBuf::from(raw);
    if path.is_dir() {
        Some(path)
    } else {
        eprintln!(
            "[cda] OT_ENGINE_RUNTIME_OVERRIDE={} is not a directory; ignoring",
            path.display()
        );
        None
    }
}

#[cfg(not(debug_assertions))]
fn dev_runtime_override() -> Option<PathBuf> {
    None
}

/// Blocking extraction — the archive is already on disk, zip crate is sync.
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
