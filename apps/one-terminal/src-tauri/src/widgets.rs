//! Widget catalog source for the Terminal launcher.
//!
//! The catalog is the **union** of two sources:
//!   * `appd`  — HTTP fetch from the App Directory (URL from the optional
//!     `appDirectoryUrl` argument, else `cfg.app_directory_url`). Skipped when
//!     the effective URL is empty; a fetch error is logged and treated as empty
//!     so a bad/unreachable endpoint never hides the local widgets.
//!   * `local` — read `widgets.config.json` from the workspace and translate
//!     its entries into AppRecord shape. Skipped when the file is missing.
//!
//! Each record is tagged with a `source` (`"appd"` | `"local"`) and a
//! source-namespaced `catalogId` (`"appd:<appId>"` / `"local:<appId>"`) so the
//! frontend can disambiguate collisions without mutating the FDC3 `appId`.
//! (`cfg.widget_source` is retained for back-compat deserialization but no
//! longer gates which sources are read.)

use crate::config::TerminalConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tauri::{Manager, State};

/// Entry in widgets.config.json. Subset of FDC3 2.2 AppRecord so it can be
/// promoted to a full App Directory record later without schema churn.
#[derive(Debug, Deserialize)]
struct LocalWidget {
    #[serde(rename = "appId")]
    app_id: String,
    title: Option<String>,
    name: Option<String>,
    url: String,
    #[serde(default)]
    categories: Vec<String>,
    icon: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LocalWidgetsFile {
    #[serde(default)]
    widgets: Vec<LocalWidget>,
}

#[derive(Debug, Serialize)]
pub struct AppDirectoryResponse {
    pub applications: Vec<Value>,
}

#[tauri::command]
pub async fn wm_list_apps(
    app: tauri::AppHandle,
    cfg: State<'_, TerminalConfig>,
    app_directory_url: Option<String>,
) -> Result<AppDirectoryResponse, String> {
    // Effective App Directory URL: caller-supplied override (user setting) wins
    // over the config default; an empty/blank value disables the appd source.
    let effective_url = app_directory_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| cfg.app_directory_url.trim());

    // appd source — non-fatal: an unreachable endpoint must not hide local widgets.
    let mut applications: Vec<Value> = if effective_url.is_empty() {
        Vec::new()
    } else {
        match fetch_appd(effective_url).await {
            Ok(apps) => apps,
            Err(e) => {
                eprintln!("[widgets] app directory fetch failed ({effective_url}): {e}");
                Vec::new()
            }
        }
    };

    // local source — read_local already tolerates a missing file. In a packaged
    // build the file ships as a Tauri resource, so hand the resolver the app's
    // resource dir; in dev it falls back to the `resources/` and upward-walk
    // conventions.
    let resource_dir = app.path().resource_dir().ok();
    applications.extend(read_local(&cfg.local_widgets_path, resource_dir.as_deref())?.applications);

    Ok(AppDirectoryResponse { applications })
}

async fn fetch_appd(url: &str) -> Result<Vec<Value>, String> {
    let res = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let body: Value = res.json().await.map_err(|e| e.to_string())?;
    let applications = body
        .get("applications")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|mut app| {
            tag_source(&mut app, "appd");
            app
        })
        .collect();
    Ok(applications)
}

/// Stamp `source` and a source-namespaced `catalogId` onto an AppRecord value.
/// The FDC3 `appId` is left untouched — it remains the panel/registry identity.
fn tag_source(app: &mut Value, source: &str) {
    let Some(obj) = app.as_object_mut() else {
        return;
    };
    let app_id = obj
        .get("appId")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    obj.insert("source".into(), Value::String(source.into()));
    obj.insert(
        "catalogId".into(),
        Value::String(format!("{source}:{app_id}")),
    );
}

fn read_local(path: &str, resource_dir: Option<&Path>) -> Result<AppDirectoryResponse, String> {
    let resolved = resolve_path(path, resource_dir);
    let text = match &resolved {
        Some(p) => std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        None => {
            // Missing file is not fatal — return empty catalog so the launcher
            // renders with no widgets rather than erroring out.
            eprintln!("[widgets] {path} not found; returning empty catalog");
            return Ok(AppDirectoryResponse {
                applications: vec![],
            });
        }
    };
    let parsed: LocalWidgetsFile =
        serde_json::from_str(&text).map_err(|e| format!("parse error: {e}"))?;

    let applications = parsed
        .widgets
        .into_iter()
        .map(local_widget_to_value)
        .collect();
    Ok(AppDirectoryResponse { applications })
}

/// Resolve `path` (usually the default `"widgets.config.json"`) to an existing
/// file. Resolution order:
///   1. Absolute path as-is.
///   2. Packaged build — the Tauri resource dir (both `<res>/path` and
///      `<res>/resources/path`, since bundling can preserve the leading dir).
///   3. Dev — a `resources/` subdir next to cwd / the binary (mirrors how
///      `config.rs` finds `terminal.config.json`).
///   4. Dev / scaffold fallback — walk upward from cwd and the binary, matching
///      `path` at each ancestor (finds a project-root `widgets.config.json`).
fn resolve_path(path: &str, resource_dir: Option<&Path>) -> Option<PathBuf> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() {
        return candidate.is_file().then_some(candidate);
    }

    let bases: Vec<PathBuf> = {
        let mut v = Vec::new();
        if let Ok(cwd) = std::env::current_dir() {
            v.push(cwd);
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                v.push(parent.to_path_buf());
            }
        }
        v
    };

    // 2 + 3: explicit candidates (resource dir, then `resources/` conventions).
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(rd) = resource_dir {
        candidates.push(rd.join(path));
        candidates.push(rd.join("resources").join(path));
    }
    for base in &bases {
        candidates.push(base.join("resources").join(path));
        candidates.push(base.join(path));
    }
    if let Some(hit) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(hit);
    }

    // 4: upward walk from each base.
    for base in &bases {
        let mut here = Some(base.as_path());
        while let Some(dir) = here {
            let candidate = dir.join(path);
            if candidate.is_file() {
                return Some(candidate);
            }
            here = dir.parent();
        }
    }
    None
}

/// `widgets.config.json` uses this sentinel instead of a literal URL for the
/// bundled `sample-widget` demo, since the registered `sample-widget://` URI
/// scheme's Origin differs by platform (see `register_uri_scheme_protocol`'s
/// docs): `<scheme>://localhost/` on macOS/iOS/Linux vs
/// `http://<scheme>.localhost/` on Windows/Android. Resolving it here keeps
/// the JSON config platform-agnostic.
const BUNDLED_SAMPLE_WIDGET_URL: &str = "bundled:sample-widget";

fn resolve_widget_url(url: String) -> String {
    if url != BUNDLED_SAMPLE_WIDGET_URL {
        return url;
    }
    if cfg!(any(target_os = "windows", target_os = "android")) {
        "http://sample-widget.localhost/".into()
    } else {
        "sample-widget://localhost/".into()
    }
}

fn local_widget_to_value(w: LocalWidget) -> Value {
    let app_id = w.app_id.clone();
    let title = w.title.clone().unwrap_or_else(|| app_id.clone());
    let name = w.name.unwrap_or_else(|| app_id.clone());
    let url = resolve_widget_url(w.url);
    serde_json::json!({
        "appId": app_id,
        "name": name,
        "title": title,
        "description": w.description,
        "details": { "url": url },
        "categories": w.categories,
        "icons": w.icon.map(|i| vec![serde_json::json!({ "src": i })]),
        "source": "local",
        "catalogId": format!("local:{}", app_id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_widget_is_tagged_with_source_and_catalog_id() {
        let w = LocalWidget {
            app_id: "my-widget".into(),
            title: Some("My Widget".into()),
            name: None,
            url: "https://example.test".into(),
            categories: vec![],
            icon: None,
            description: None,
        };
        let v = local_widget_to_value(w);
        assert_eq!(v["appId"], "my-widget");
        assert_eq!(v["source"], "local");
        assert_eq!(v["catalogId"], "local:my-widget");
    }

    #[test]
    fn bundled_sample_widget_sentinel_resolves_to_registered_scheme() {
        let resolved = resolve_widget_url(BUNDLED_SAMPLE_WIDGET_URL.into());
        assert_ne!(resolved, BUNDLED_SAMPLE_WIDGET_URL);
        assert!(resolved.contains("sample-widget"));
    }

    #[test]
    fn non_sentinel_urls_pass_through_unchanged() {
        assert_eq!(
            resolve_widget_url("https://example.test".into()),
            "https://example.test"
        );
    }

    #[test]
    fn tag_source_namespaces_catalog_id_without_touching_app_id() {
        let mut v = serde_json::json!({ "appId": "sample-chart", "name": "Sample" });
        tag_source(&mut v, "appd");
        // FDC3 appId stays intact; only catalogId carries the source prefix.
        assert_eq!(v["appId"], "sample-chart");
        assert_eq!(v["source"], "appd");
        assert_eq!(v["catalogId"], "appd:sample-chart");
    }
}
