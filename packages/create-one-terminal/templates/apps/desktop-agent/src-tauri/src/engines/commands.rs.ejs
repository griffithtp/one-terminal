//! Tauri commands exposing the engine catalog and runtime cache to the UI.

use tauri::{AppHandle, State};

use super::{
    EngineBinding, EngineCatalog, EngineCatalogClient, EngineRuntimeStore, InstalledEngine,
};

/// Return the currently cached engine catalog.
#[tauri::command]
pub async fn engine_catalog(
    catalog: State<'_, EngineCatalogClient>,
) -> Result<EngineCatalog, String> {
    Ok(catalog.snapshot().await)
}

/// Trigger a fresh fetch from the App Directory engine catalog.
#[tauri::command]
pub async fn engine_refresh_catalog(
    catalog: State<'_, EngineCatalogClient>,
) -> Result<usize, String> {
    catalog.refresh().await
}

/// List the engine runtimes already installed in the on-disk cache.
#[tauri::command]
pub async fn engine_list_installed(
    store: State<'_, EngineRuntimeStore>,
) -> Result<Vec<InstalledEngine>, String> {
    Ok(store.list_installed())
}

/// Ensure the given binding is installed locally, downloading it on demand.
/// For `version == "system"` this is a no-op and returns `path: None`.
#[tauri::command]
pub async fn engine_ensure(
    binding: EngineBinding,
    store: State<'_, EngineRuntimeStore>,
    catalog: State<'_, EngineCatalogClient>,
    app: AppHandle,
) -> Result<InstalledEngine, String> {
    let snapshot = catalog.snapshot().await;
    store.ensure(&binding, &snapshot, &app).await
}
