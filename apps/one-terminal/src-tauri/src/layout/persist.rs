//! Layout persistence — serialise/deserialise the full layout tree to disk.
//!
//! File: `<data_dir>/layout.json`.  Writes are atomic (tmp → rename).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::node::LayoutNode;

/// Serialisable mirror of `LeafMeta` — all fields needed to restore a panel.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedLeafMeta {
    pub app_id: String,
    pub url: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_binding: Option<ot_core::engine::EngineBinding>,
    /// User-set display name; `None` means "show app-provided title".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Webview zoom multiplier. Default `1.0`.
    #[serde(default = "default_zoom")]
    pub zoom_factor: f64,
}

fn default_zoom() -> f64 {
    1.0
}

/// Full layout state written to `layout.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedLayout {
    /// Root of the layout tree. `None` = empty layout.
    pub tree: Option<LayoutNode>,
    /// Per-leaf metadata keyed by webview label.
    pub meta: HashMap<String, PersistedLeafMeta>,
    /// Label of the active panel, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_panel: Option<String>,
    /// Stable id of the maximized Stack, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximized_stack_id: Option<String>,
}

/// Atomically write `layout` to `<data_dir>/layout.json`.
pub fn save(layout: &PersistedLayout, data_dir: &Path) -> Result<(), String> {
    let json = serde_json::to_string(layout).map_err(|e| e.to_string())?;
    let tmp = data_dir.join("layout.json.tmp");
    let dest = data_dir.join("layout.json");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Load `<data_dir>/layout.json`, returning `None` if the file is absent or
/// unparseable (treated as a fresh start rather than a hard error).
pub fn load(data_dir: &Path) -> Option<PersistedLayout> {
    let bytes = std::fs::read(data_dir.join("layout.json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}
