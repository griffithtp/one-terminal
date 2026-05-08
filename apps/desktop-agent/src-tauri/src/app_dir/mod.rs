use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::engines::{EngineBinding, OsKey};

// ── AppD subset types (mirrors FDC3 2.2 App Directory response) ───────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDetails {
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IntentDef {
    pub display_name: Option<String>,
    #[serde(default)]
    pub contexts: Vec<String>,
    pub result_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppIntents {
    /// intent name → intent definition (what this app can handle)
    pub listens_for: Option<HashMap<String, IntentDef>>,
    /// intent name → context types (what this app raises)
    pub raises: Option<HashMap<String, Vec<String>>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppInterop {
    pub intents: Option<AppIntents>,
}

/// Slim App Directory record — a subset of FDC3 2.2 AppD.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRecord {
    pub app_id: String,
    pub name: String,
    #[serde(rename = "type", default)]
    pub app_type: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub version: Option<String>,
    pub details: Option<AppDetails>,
    #[serde(default)]
    pub categories: Vec<String>,
    pub interop: Option<AppInterop>,
    /// Per-OS list of supported browser engines. The caller picks one at
    /// launch time; an empty / missing list ⇒ use launcher default.
    #[serde(default)]
    pub engine_bindings: Option<HashMap<OsKey, Vec<EngineBinding>>>,
}

impl AppRecord {
    /// All supported engines for `os`, in directory-declared order. Returns
    /// an empty slice if the app has no entry for this OS.
    pub fn bindings_for(&self, os: OsKey) -> &[EngineBinding] {
        self.engine_bindings
            .as_ref()
            .and_then(|m| m.get(&os))
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// First supported engine for `os`, if any. Used by launch paths that
    /// don't present a user picker (e.g. auto-launching from the CDA tray).
    pub fn default_binding_for(&self, os: OsKey) -> Option<&EngineBinding> {
        self.bindings_for(os).first()
    }
}

/// Response envelope from `GET /v2/apps`.
#[derive(Debug, Deserialize)]
struct AppsListResponse {
    applications: Vec<AppRecord>,
}

// ── Tauri event: emitted when tier-4 intent resolution finds App Dir candidates

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CdaIntentNeedsResolutionEvent {
    pub intent: String,
    pub context: serde_json::Value,
    pub source_instance_id: String,
    pub request_id: String,
    /// App Directory apps that declare `listensFor` this intent.
    pub candidates: Vec<AppRecord>,
}

// ── AppDirectoryCache ─────────────────────────────────────────────────────────

/// Thread-safe in-memory cache of App Directory records.
/// Wrapped in `Arc` so it can be cheaply cloned across Tauri state and tasks.
#[derive(Clone)]
pub struct AppDirectoryCache {
    base_url: String,
    inner: Arc<RwLock<Vec<AppRecord>>>,
}

impl AppDirectoryCache {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            inner: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Fetch fresh data from `GET /v2/apps` and replace the in-memory cache.
    ///
    /// Returns the number of records loaded, or an error string.
    /// A `reqwest::Error` (e.g. connection refused) is treated as a non-fatal
    /// startup warning so the CDA continues to run without App Directory data.
    pub async fn refresh(&self) -> Result<usize, String> {
        let url = format!("{}/v2/apps", self.base_url.trim_end_matches('/'));
        let response = reqwest::get(&url)
            .await
            .map_err(|e| format!("App Directory fetch failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!("App Directory returned HTTP {}", response.status()));
        }

        let data: AppsListResponse = response
            .json()
            .await
            .map_err(|e| format!("App Directory JSON parse error: {e}"))?;

        let count = data.applications.len();
        let mut guard = self.inner.write().await;
        *guard = data.applications;
        println!("[cda] App Directory cache refreshed: {count} record(s)");
        Ok(count)
    }

    /// Return a snapshot of all cached App Directory records.
    pub async fn list_all(&self) -> Vec<AppRecord> {
        self.inner.read().await.clone()
    }

    /// Return all cached apps that declare `listensFor` the given intent name.
    pub async fn handlers_for_intent(&self, intent: &str) -> Vec<AppRecord> {
        let guard = self.inner.read().await;
        guard
            .iter()
            .filter(|app| {
                app.interop
                    .as_ref()
                    .and_then(|i| i.intents.as_ref())
                    .and_then(|intents| intents.listens_for.as_ref())
                    .map(|lf| lf.contains_key(intent))
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}
