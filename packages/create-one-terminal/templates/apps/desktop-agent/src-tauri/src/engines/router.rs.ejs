//! Pure launch-planning logic. Takes an [`AppRecord`] + a resolved
//! [`InstalledEngine`] and decides *how* to open the app.
//!
//! The router is deliberately side-effect free: download/extraction lives in
//! [`EngineRuntimeStore`](super::runtime), and the actual process spawn /
//! [`WebviewWindowBuilder`](tauri::WebviewWindowBuilder) call lives in
//! `cda_launch_app`. That split keeps the decision table easy to reason about
//! and unit-testable without Tauri state.
//!
//! Pass the `manifests` slice from [`EngineRuntimeStore::plugins`] to enable
//! plugin-declared engines. An empty slice is safe when only built-in engines
//! are in use.

use std::path::PathBuf;

use ot_core::plugin::{EngineManifest, LaunchMode};

use crate::app_dir::AppRecord;

use super::{current_os, is_system_version, EngineBinding, EngineFamily, InstalledEngine};

/// Where the app is being launched from.
#[derive(Clone, Debug)]
pub enum LaunchContext {
    /// Stand-alone window, driven by the CDA app launcher.
    Standalone,
    /// As a tab inside an existing window-manager session. (Phase 4 uses this
    /// to delegate to a sibling WM host when engines mismatch.)
    #[allow(dead_code)]
    Tab { wm_session_id: String },
}

#[derive(Clone, Debug)]
pub enum LaunchPlan {
    /// Open a `WebviewWindow` in the current process. Fastest path; used when
    /// the app has no binding, asks for the system engine, or the binding
    /// matches what this process is already running.
    InProcessWebview { url: String, title: String },

    /// Spawn the `tauri-webview-host` sidecar with a pinned WebView2 runtime.
    /// `runtime_folder == None` means "use system WebView2" (same as in-
    /// process, but in a separate host so the current process's engine isn't
    /// forced to change).
    SpawnTauriHost {
        runtime_folder: Option<PathBuf>,
        url: String,
        title: String,
    },

    /// Spawn the bundled Electron host pointed at the URL. The actual binary
    /// and shell-folder lookup happens in `ot_core::electron_host`; we just
    /// carry the binding so `execute_plan` can resolve them.
    SpawnElectronHost {
        binding: EngineBinding,
        url: String,
        title: String,
    },

    /// Spawn an arbitrary binary declared by a plugin manifest, with fully
    /// resolved env vars (placeholders already substituted).
    SpawnBinary {
        binary_name: String,
        /// `(key, value)` pairs — already template-expanded.
        env_vars: Vec<(String, String)>,
    },
}

/// Default engine binding for `appd` on the current OS — the first entry in
/// the directory's supported list. Callers that present a user picker should
/// pass the explicit choice to `plan_launch` instead.
pub fn resolve_binding(appd: &AppRecord) -> Option<EngineBinding> {
    appd.default_binding_for(current_os()).cloned()
}

/// Build the launch plan.
///
/// - `installed` — result of [`EngineRuntimeStore::ensure`]; `None` when the
///   app has no engine binding for this OS.
/// - `manifests` — plugin manifests from [`EngineRuntimeStore::plugins`].
///   Pass `&[]` when only built-in engines are used.
pub fn plan_launch(
    appd: &AppRecord,
    _context: &LaunchContext,
    installed: Option<&InstalledEngine>,
    manifests: &[EngineManifest],
) -> Result<LaunchPlan, String> {
    let url = appd
        .details
        .as_ref()
        .and_then(|d| d.url.clone())
        .ok_or_else(|| format!("App {} has no launch URL", appd.app_id))?;
    let title = appd.title.clone().unwrap_or_else(|| appd.name.clone());

    // No engine binding for this OS → keep today's behavior (in-process).
    let Some(installed) = installed else {
        return Ok(LaunchPlan::InProcessWebview { url, title });
    };

    match &installed.family {
        // WebView2: same engine as Tauri on Windows. Run in-process when the
        // app accepts the system runtime; otherwise spawn the sidecar host so
        // the pinned fixed-runtime folder can be applied via env var.
        EngineFamily::Webview2 => {
            if installed.system || is_system_version(&installed.version) {
                Ok(LaunchPlan::InProcessWebview { url, title })
            } else {
                Ok(LaunchPlan::SpawnTauriHost {
                    runtime_folder: installed.path.clone(),
                    url,
                    title,
                })
            }
        }
        // WKWebView is always the macOS system engine — nothing to pin.
        EngineFamily::Wkwebview => Ok(LaunchPlan::InProcessWebview { url, title }),
        // Electron = Chromium host, separate process.
        EngineFamily::Electron => {
            // Defensive: resolve_runtime should have failed earlier if the
            // catalog version isn't installed, but keep the check so a
            // missing path doesn't paper over a broken install upstream.
            // Dev overrides (OT_ELECTRON_BIN / OT_ELECTRON_HOST_OVERRIDE)
            // bypass this path: callers using them won't have an installed
            // catalog runtime, and `execute_plan` honors them directly.
            if !installed.system && installed.path.is_none() {
                return Err("Electron binding missing runtime path".to_string());
            }
            Ok(LaunchPlan::SpawnElectronHost {
                binding: EngineBinding {
                    family: installed.family.clone(),
                    version: installed.version.clone(),
                },
                url,
                title,
            })
        }
        // Plugin / manifest-declared engine — look up the launch mode.
        EngineFamily::Custom(name) => {
            let manifest = manifests
                .iter()
                .find(|m| m.family == *name)
                .ok_or_else(|| {
                    format!(
                        "No plugin manifest found for engine '{name}'. \
                         Place a manifest.json under <engine-cache>/plugins/{name}/."
                    )
                })?;
            plan_from_manifest(manifest, installed, url, title, &appd.app_id)
        }
    }
}

/// Translate a plugin manifest's `LaunchMode` into a concrete `LaunchPlan`.
fn plan_from_manifest(
    manifest: &EngineManifest,
    installed: &InstalledEngine,
    url: String,
    title: String,
    app_id: &str,
) -> Result<LaunchPlan, String> {
    let runtime_path = installed
        .path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    match &manifest.launch_mode {
        LaunchMode::InProcess => Ok(LaunchPlan::InProcessWebview { url, title }),

        LaunchMode::SpawnTauriHost => Ok(LaunchPlan::SpawnTauriHost {
            runtime_folder: installed.path.clone(),
            url,
            title,
        }),

        LaunchMode::SpawnElectronHost => Ok(LaunchPlan::SpawnElectronHost {
            binding: EngineBinding {
                family: installed.family.clone(),
                version: installed.version.clone(),
            },
            url,
            title,
        }),

        LaunchMode::SpawnBinary {
            binary_name,
            env_templates,
        } => {
            let env_vars = env_templates
                .iter()
                .map(|t| {
                    let v = EngineManifest::expand_template(
                        &t.value,
                        &url,
                        &title,
                        app_id,
                        &runtime_path,
                    );
                    (t.key.clone(), v)
                })
                .collect();
            Ok(LaunchPlan::SpawnBinary {
                binary_name: binary_name.clone(),
                env_vars,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_dir::{AppDetails, AppRecord};
    use std::collections::HashMap;

    fn app(url: &str, bindings: Option<HashMap<OsKey, Vec<EngineBinding>>>) -> AppRecord {
        AppRecord {
            app_id: "t".into(),
            name: "T".into(),
            app_type: "web".into(),
            title: None,
            description: None,
            version: None,
            details: Some(AppDetails {
                url: Some(url.into()),
            }),
            categories: vec![],
            interop: None,
            engine_bindings: bindings,
        }
    }

    use super::super::OsKey;

    #[test]
    fn no_binding_is_in_process() {
        let appd = app("http://x", None);
        let plan = plan_launch(&appd, &LaunchContext::Standalone, None, &[]).unwrap();
        assert!(matches!(plan, LaunchPlan::InProcessWebview { .. }));
    }

    #[test]
    fn system_webview2_is_in_process() {
        let appd = app("http://x", None);
        let inst = InstalledEngine {
            family: EngineFamily::Webview2,
            version: "system".into(),
            path: None,
            system: true,
        };
        let plan = plan_launch(&appd, &LaunchContext::Standalone, Some(&inst), &[]).unwrap();
        assert!(matches!(plan, LaunchPlan::InProcessWebview { .. }));
    }

    #[test]
    fn pinned_webview2_spawns_host() {
        let appd = app("http://x", None);
        let inst = InstalledEngine {
            family: EngineFamily::Webview2,
            version: "124.0.2478.97".into(),
            path: Some(PathBuf::from("/tmp/wv2")),
            system: false,
        };
        let plan = plan_launch(&appd, &LaunchContext::Standalone, Some(&inst), &[]).unwrap();
        match plan {
            LaunchPlan::SpawnTauriHost { runtime_folder, .. } => {
                assert_eq!(runtime_folder, Some(PathBuf::from("/tmp/wv2")));
            }
            _ => panic!("expected SpawnTauriHost"),
        }
    }

    #[test]
    fn electron_requires_path() {
        let appd = app("http://x", None);
        let inst_no_path = InstalledEngine {
            family: EngineFamily::Electron,
            version: "29.3.0".into(),
            path: None,
            system: false,
        };
        let plan = plan_launch(&appd, &LaunchContext::Standalone, Some(&inst_no_path), &[]);
        assert!(plan.is_err());
    }

    #[test]
    fn custom_engine_without_manifest_errors() {
        let appd = app("http://x", None);
        let inst = InstalledEngine {
            family: EngineFamily::Custom("cef".into()),
            version: "124.0.0".into(),
            path: Some(PathBuf::from("/tmp/cef")),
            system: false,
        };
        let plan = plan_launch(&appd, &LaunchContext::Standalone, Some(&inst), &[]);
        assert!(plan.is_err());
    }
}
