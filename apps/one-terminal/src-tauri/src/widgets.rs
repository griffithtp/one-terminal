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
use std::path::PathBuf;
use tauri::State;

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

    // local source — read_local already tolerates a missing file.
    applications.extend(read_local(&cfg.local_widgets_path)?.applications);

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

fn read_local(path: &str) -> Result<AppDirectoryResponse, String> {
    let resolved = resolve_path(path);
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

/// Search upward from the binary and from cwd for the widgets.config.json file.
fn resolve_path(path: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(path);
    if candidate.is_absolute() && candidate.is_file() {
        return Some(candidate);
    }

    let mut search_roots: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        search_roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            search_roots.push(parent.to_path_buf());
        }
    }

    for root in search_roots {
        let mut here = Some(root.as_path());
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

fn local_widget_to_value(w: LocalWidget) -> Value {
    let app_id = w.app_id.clone();
    let title = w.title.clone().unwrap_or_else(|| app_id.clone());
    let name = w.name.unwrap_or_else(|| app_id.clone());
    serde_json::json!({
        "appId": app_id,
        "name": name,
        "title": title,
        "description": w.description,
        "details": { "url": w.url },
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
    fn tag_source_namespaces_catalog_id_without_touching_app_id() {
        let mut v = serde_json::json!({ "appId": "sample-chart", "name": "Sample" });
        tag_source(&mut v, "appd");
        // FDC3 appId stays intact; only catalogId carries the source prefix.
        assert_eq!(v["appId"], "sample-chart");
        assert_eq!(v["source"], "appd");
        assert_eq!(v["catalogId"], "appd:sample-chart");
    }
}
