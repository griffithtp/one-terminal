//! Layout persistence — serialise/deserialise the full layout state to disk.
//!
//! Current format: `<data_dir>/terminals/main/dashboards.json` (`TerminalPersist`).
//!
//! Migration: if the old `<data_dir>/layout.json` exists it is converted
//! in-place to a single-dashboard `TerminalPersist`, written to the new path,
//! and the old file is deleted.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::dashboard::PersistedDashboard;
use super::node::LayoutNode;

// ── Leaf-level metadata ───────────────────────────────────────────────────────

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

// ── Legacy top-level type (pre-dashboard era) ─────────────────────────────────

/// Layout state as written to `layout.json` before dashboards were introduced.
/// Retained for migration; all new code uses `TerminalPersist`.
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
    /// Stable id of the maximised Stack, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximized_stack_id: Option<String>,
}

// ── Current top-level type ────────────────────────────────────────────────────

/// Full terminal persistence state written to `terminals/main/dashboards.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPersist {
    pub active_dashboard: String,
    #[serde(default = "default_auto_save")]
    pub auto_save: bool,
    pub dashboards: Vec<PersistedDashboard>,
}

fn default_auto_save() -> bool {
    true
}

// ── Save ──────────────────────────────────────────────────────────────────────

/// Atomically write `persist` to `<data_dir>/terminals/main/dashboards.json`.
pub fn save_terminal(persist: &TerminalPersist, data_dir: &Path) -> Result<(), String> {
    let dir = data_dir.join("terminals").join("main");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(persist).map_err(|e| e.to_string())?;
    let tmp = dir.join("dashboards.json.tmp");
    let dest = dir.join("dashboards.json");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Load (with migration) ─────────────────────────────────────────────────────

/// Load from `<data_dir>/terminals/main/dashboards.json`, migrating from the
/// legacy `<data_dir>/layout.json` if the new file does not exist yet.
///
/// Returns `None` only on a fresh install with no persisted state at all.
pub fn load_terminal(data_dir: &Path) -> Option<TerminalPersist> {
    let new_path = data_dir
        .join("terminals")
        .join("main")
        .join("dashboards.json");

    // Happy path: new format already present.
    if let Ok(bytes) = std::fs::read(&new_path) {
        if let Ok(p) = serde_json::from_slice::<TerminalPersist>(&bytes) {
            return Some(p);
        }
    }

    // Migration: convert the old layout.json to a single-dashboard TerminalPersist.
    let old_path = data_dir.join("layout.json");
    if let Ok(bytes) = std::fs::read(&old_path) {
        if let Ok(old) = serde_json::from_slice::<PersistedLayout>(&bytes) {
            let dashboard = PersistedDashboard::from_layout("Default".to_string(), old);
            let persist = TerminalPersist {
                active_dashboard: "Default".to_string(),
                auto_save: true,
                dashboards: vec![dashboard],
            };
            if save_terminal(&persist, data_dir).is_ok() {
                let _ = std::fs::remove_file(&old_path);
                eprintln!("[layout] migrated layout.json → terminals/main/dashboards.json");
            }
            return Some(persist);
        }
    }

    None
}
