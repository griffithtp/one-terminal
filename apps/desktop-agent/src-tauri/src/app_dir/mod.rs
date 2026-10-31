use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::engines::{EngineBinding, OsKey};

/// The FDC3 version this agent implements, as `(major, minor)`.
const AGENT_FDC3_VERSION: (u32, u32) = (3, 0);

// ── AppD subset types (mirrors the FDC3 3.0 App Directory response) ───────────────

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

/// Slim App Directory record — a subset of the FDC3 3.0 AppD.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRecord {
    pub app_id: String,
    #[serde(rename = "type", default)]
    pub app_type: String,
    pub title: Option<String>,
    /// `name` was removed in FDC3 3.0, but a remote directory may still send it.
    #[serde(default, rename = "name", skip_serializing)]
    pub legacy_name: Option<String>,
    pub description: Option<String>,
    pub tooltip: Option<String>,
    pub version: Option<String>,
    /// Range of FDC3 versions the app supports, e.g. `>=2.2`. Absent means any.
    pub fdc3_version: Option<String>,
    pub icons: Option<serde_json::Value>,
    pub screenshots: Option<serde_json::Value>,
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
    /// The title to show for this app, falling back to a legacy name, then its id.
    pub fn display_title(&self) -> &str {
        self.title
            .as_deref()
            .or(self.legacy_name.as_deref())
            .unwrap_or(&self.app_id)
    }

    fn supports_agent_fdc3_version(&self) -> bool {
        self.fdc3_version
            .as_deref()
            .is_none_or(|range| range_includes(range, AGENT_FDC3_VERSION))
    }

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

/// Whether an AppD `fdc3Version` range such as `>=2.2`, `2.2 || 3.0` or
/// `2.0 - 3.0` includes `version`. A range that doesn't parse excludes the app.
fn range_includes(range: &str, version: (u32, u32)) -> bool {
    fn parse(v: &str) -> Option<(u32, u32)> {
        let (major, minor) = v.trim().split_once('.')?;
        Some((major.parse().ok()?, minor.parse().ok()?))
    }
    fn comparator(part: &str, version: (u32, u32)) -> Option<bool> {
        for op in [">=", "<=", ">", "<", "=", "~", "^"] {
            if let Some(rest) = part.strip_prefix(op) {
                let v = parse(rest)?;
                return Some(match op {
                    ">=" => version >= v,
                    "<=" => version <= v,
                    ">" => version > v,
                    "<" => version < v,
                    "~" => version == v,
                    "^" => version.0 == v.0 && version >= v,
                    _ => version == v,
                });
            }
        }
        Some(version == parse(part)?)
    }
    range.split("||").any(|alternative| {
        let alternative = alternative.trim();
        if let Some((low, high)) = alternative.split_once(" - ") {
            return matches!((parse(low), parse(high)), (Some(l), Some(h)) if version >= l && version <= h);
        }
        // Attach an operator separated from its version by a space (">= 2.2").
        let compact = alternative
            .replace(">= ", ">=")
            .replace("<= ", "<=")
            .replace("> ", ">")
            .replace("< ", "<");
        !compact.is_empty()
            && compact
                .split_whitespace()
                .all(|part| comparator(part, version) == Some(true))
    })
}

/// Response envelope from `GET /v2/apps`.
#[derive(Debug, Deserialize)]
struct AppsListResponse {
    applications: Vec<AppRecord>,
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

    /// Records usable by this agent over FDC3: those whose `fdc3Version` range,
    /// if any, includes the version it implements.
    pub async fn list_fdc3_compatible(&self) -> Vec<AppRecord> {
        self.inner
            .read()
            .await
            .iter()
            .filter(|a| a.supports_agent_fdc3_version())
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::range_includes;

    #[test]
    fn fdc3_version_ranges() {
        let v3 = (3, 0);
        for included in [
            "3.0",
            "=3.0",
            ">=2.2",
            ">= 2.2",
            "<=3.0",
            "2.2 || 3.0",
            "2.0 - 3.0",
            "^3.0",
            "~3.0",
            ">=2.0 <4.0",
        ] {
            assert!(
                range_includes(included, v3),
                "{included} should include 3.0"
            );
        }
        for excluded in [
            "2.2",
            "<3.0",
            ">3.0",
            "^2.0",
            "2.0 - 2.2",
            ">=2.0 <3.0",
            "nonsense",
            "",
        ] {
            assert!(
                !range_includes(excluded, v3),
                "{excluded} should exclude 3.0"
            );
        }
    }
}
