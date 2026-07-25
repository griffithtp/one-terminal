//! Layout persistence — serialise/deserialise the full layout state to disk.
//!
//! Format: `<data_dir>/terminals/<id>/dashboards.json` (`TerminalPersist`).
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
    /// FDC3 user channel this panel is joined to. `None` = no channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fdc3_channel: Option<String>,
    /// When `true`, switching away from this panel's Dashboard parks its
    /// webview off-screen instead of closing it, so it keeps running and
    /// reappears instantly when the Dashboard becomes active again.
    #[serde(default)]
    pub keep_alive: bool,
    /// Whether the read-only address-bar row is shown below the title
    /// header (Generic Web Widget panels only). Default `true`.
    #[serde(default = "default_show_address_bar")]
    pub show_address_bar: bool,
}

fn default_zoom() -> f64 {
    1.0
}

fn default_show_address_bar() -> bool {
    true
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

// ── Window config ─────────────────────────────────────────────────────────────

/// Saved OS window position and size. Values are logical pixels.
/// Width/height of `0.0` signals "not yet saved" — callers fall back to default
/// geometry and OS-chosen positioning in that case.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWindowConfig {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Monitor name hint; compared against available monitors on restore to
    /// detect disconnected displays and fall back to the primary monitor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor: Option<String>,
}

// ── Current top-level type ────────────────────────────────────────────────────

/// Full terminal persistence state written to `terminals/<id>/dashboards.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPersist {
    /// User-visible Terminal name (e.g. "Terminal 2").
    #[serde(default)]
    pub name: String,
    pub active_dashboard: String,
    #[serde(default = "default_auto_save")]
    pub auto_save: bool,
    pub dashboards: Vec<PersistedDashboard>,
    /// Saved OS window position and size; zeroed = use OS default.
    #[serde(default)]
    pub window: PersistedWindowConfig,
}

impl Default for TerminalPersist {
    fn default() -> Self {
        Self {
            name: String::new(),
            active_dashboard: "Default".to_string(),
            auto_save: true,
            dashboards: vec![],
            window: PersistedWindowConfig::default(),
        }
    }
}

fn default_auto_save() -> bool {
    true
}

// ── Path helpers ──────────────────────────────────────────────────────────────

/// Derive the `terminals/` subdirectory name from a terminal window label.
///
/// `"terminal-main"` → `"main"`, `"terminal-2"` → `"2"`, etc.
fn terminal_dir_name(terminal_id: &str) -> &str {
    terminal_id.strip_prefix("terminal-").unwrap_or(terminal_id)
}

fn terminal_dashboards_path(terminal_id: &str, data_dir: &Path) -> std::path::PathBuf {
    data_dir
        .join("terminals")
        .join(terminal_dir_name(terminal_id))
        .join("dashboards.json")
}

// ── Save ──────────────────────────────────────────────────────────────────────

/// Write `persist` to `<data_dir>/terminals/<id>/dashboards.json` atomically.
pub fn save_terminal_for(
    terminal_id: &str,
    persist: &TerminalPersist,
    data_dir: &Path,
) -> Result<(), String> {
    let dir = data_dir
        .join("terminals")
        .join(terminal_dir_name(terminal_id));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(persist).map_err(|e| e.to_string())?;
    let tmp = dir.join("dashboards.json.tmp");
    let dest = dir.join("dashboards.json");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Backwards-compatible alias — saves the primary terminal.
pub fn save_terminal(persist: &TerminalPersist, data_dir: &Path) -> Result<(), String> {
    save_terminal_for("terminal-main", persist, data_dir)
}

/// Update only the dashboard fields (active_dashboard, auto_save, dashboards),
/// preserving `name` and `window` from the existing file.
///
/// Used by `LayoutTree::schedule_save` so layout auto-saves don't overwrite the
/// terminal name or window position that were written by other commands.
pub fn save_terminal_dashboards(
    terminal_id: &str,
    persist: &TerminalPersist,
    data_dir: &Path,
) -> Result<(), String> {
    let mut existing =
        load_terminal_for(terminal_id, data_dir).unwrap_or_else(|| TerminalPersist {
            name: terminal_dir_name(terminal_id).to_string(),
            ..Default::default()
        });
    existing.active_dashboard = persist.active_dashboard.clone();
    existing.auto_save = persist.auto_save;
    existing.dashboards = persist.dashboards.clone();
    save_terminal_for(terminal_id, &existing, data_dir)
}

/// Update only the `window` field, preserving all other fields.
pub fn update_window_config(
    terminal_id: &str,
    window: &PersistedWindowConfig,
    data_dir: &Path,
) -> Result<(), String> {
    let mut existing = load_terminal_for(terminal_id, data_dir).unwrap_or_default();
    existing.window = window.clone();
    save_terminal_for(terminal_id, &existing, data_dir)
}

// ── List ──────────────────────────────────────────────────────────────────────

/// Return window labels for all saved non-main terminals by scanning
/// `<data_dir>/terminals/`. Used at startup to restore previously open Terminals.
pub fn list_saved_terminal_ids(data_dir: &Path) -> Vec<String> {
    let terminals_dir = data_dir.join("terminals");
    let Ok(entries) = std::fs::read_dir(&terminals_dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.is_dir() && path.join("dashboards.json").exists() {
                let name = path.file_name()?.to_str()?;
                if name != "main" {
                    return Some(format!("terminal-{name}"));
                }
            }
            None
        })
        .collect();
    ids.sort();
    ids
}

// ── Delete ────────────────────────────────────────────────────────────────────

/// Remove the persisted state directory for a terminal so it is not restored
/// on next startup. Silently succeeds if the directory does not exist.
pub fn delete_terminal_for(terminal_id: &str, data_dir: &Path) -> Result<(), String> {
    let dir = data_dir
        .join("terminals")
        .join(terminal_dir_name(terminal_id));
    match std::fs::remove_dir_all(&dir) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

// ── Load ──────────────────────────────────────────────────────────────────────

/// Load from `<data_dir>/terminals/<id>/dashboards.json`.
/// Returns `None` if the file does not exist or cannot be parsed.
pub fn load_terminal_for(terminal_id: &str, data_dir: &Path) -> Option<TerminalPersist> {
    let path = terminal_dashboards_path(terminal_id, data_dir);
    let bytes = std::fs::read(&path).ok()?;
    serde_json::from_slice::<TerminalPersist>(&bytes).ok()
}

/// Load from `<data_dir>/terminals/main/dashboards.json`, migrating from the
/// legacy `<data_dir>/layout.json` if the new file does not exist yet.
///
/// Returns `None` only on a fresh install with no persisted state at all.
pub fn load_terminal(data_dir: &Path) -> Option<TerminalPersist> {
    // Happy path: new format already present.
    if let Some(p) = load_terminal_for("terminal-main", data_dir) {
        return Some(p);
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
                ..Default::default()
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
