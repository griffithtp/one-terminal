//! Widget catalog source for the Terminal launcher.
//!
//! Two backends:
//!   * `Appd`  — HTTP fetch from the App Directory at `cfg.app_directory_url`.
//!   * `Local` — read `widgets.config.json` from the workspace and translate its entries into AppRecord shape.
//!
//! Both backends return the same JSON shape (`{ applications: [...] }`) so the
//! frontend treats them identically.

use crate::config::{TerminalConfig, WidgetSourceKind};
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
pub async fn wm_list_apps(cfg: State<'_, TerminalConfig>) -> Result<AppDirectoryResponse, String> {
    match cfg.widget_source {
        WidgetSourceKind::Appd => fetch_appd(&cfg.app_directory_url).await,
        WidgetSourceKind::Local => read_local(&cfg.local_widgets_path),
    }
}

async fn fetch_appd(url: &str) -> Result<AppDirectoryResponse, String> {
    let res = reqwest::get(url).await.map_err(|e| e.to_string())?;
    let body: Value = res.json().await.map_err(|e| e.to_string())?;
    let applications = body
        .get("applications")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(AppDirectoryResponse { applications })
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
    let title = w.title.clone().unwrap_or_else(|| w.app_id.clone());
    let name = w.name.unwrap_or_else(|| w.app_id.clone());
    serde_json::json!({
        "appId": w.app_id,
        "name": name,
        "title": title,
        "description": w.description,
        "details": { "url": w.url },
        "categories": w.categories,
        "icons": w.icon.map(|i| vec![serde_json::json!({ "src": i })]),
    })
}
