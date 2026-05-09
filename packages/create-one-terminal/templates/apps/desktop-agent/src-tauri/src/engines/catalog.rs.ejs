//! In-memory cache of the engine catalog served by the App Directory
//! (`GET /v2/engines`). Refreshed at startup and on demand; failures are
//! non-fatal so CDA can still run when the App Directory is offline.

use std::sync::Arc;
use tokio::sync::RwLock;

use super::EngineCatalog;

/// Response envelope from `GET /v2/engines`.
#[derive(Debug, serde::Deserialize)]
struct EngineCatalogResponse {
    engines: EngineCatalog,
}

#[derive(Clone)]
pub struct EngineCatalogClient {
    base_url: String,
    inner: Arc<RwLock<EngineCatalog>>,
}

impl EngineCatalogClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            inner: Arc::new(RwLock::new(EngineCatalog::default())),
        }
    }

    /// Fetch fresh data from `GET /v2/engines` and replace the cache. Returns
    /// the total number of entries across all OSes, or an error string.
    pub async fn refresh(&self) -> Result<usize, String> {
        let url = format!("{}/v2/engines", self.base_url.trim_end_matches('/'));
        let response = reqwest::get(&url)
            .await
            .map_err(|e| format!("Engine catalog fetch failed: {e}"))?;

        if !response.status().is_success() {
            return Err(format!(
                "Engine catalog returned HTTP {}",
                response.status()
            ));
        }

        let data: EngineCatalogResponse = response
            .json()
            .await
            .map_err(|e| format!("Engine catalog JSON parse error: {e}"))?;

        let count = data.engines.windows.as_ref().map(|v| v.len()).unwrap_or(0)
            + data.engines.macos.as_ref().map(|v| v.len()).unwrap_or(0)
            + data.engines.linux.as_ref().map(|v| v.len()).unwrap_or(0);

        *self.inner.write().await = data.engines;
        println!("[cda] Engine catalog refreshed: {count} entry/entries");
        Ok(count)
    }

    pub async fn snapshot(&self) -> EngineCatalog {
        self.inner.read().await.clone()
    }
}
