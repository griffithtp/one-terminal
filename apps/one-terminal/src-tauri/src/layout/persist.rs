//! Layout persistence — serialise/deserialise the full layout state to disk.
//!
//! Format: `<data_dir>/terminals/<id>/dashboards.json` (`TerminalPersist`,
//! per-window session state) plus one shared `<data_dir>/dashboards.json`
//! (`PersistedDashboardRegistry`, Issue 15-B) holding every dashboard's
//! content.
//!
//! Migration: if the old `<data_dir>/layout.json` exists it is converted
//! in-place to a single-dashboard `TerminalPersist`, written to the new path,
//! and the old file is deleted. Separately, Issue 15-B's
//! `load_or_migrate_registry` handles the one-time upgrade from N isolated
//! per-terminal dashboard lists (each `TerminalPersist.dashboards`, pre-15-B)
//! to the one shared `dashboards.json`.

use std::collections::HashMap;
use std::path::Path;

use indexmap::IndexMap;
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
    /// header (Generic Web Widget panels only). Defaults to hidden — the
    /// user opts in via the tab context menu.
    #[serde(default)]
    pub show_address_bar: bool,
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

/// Per-window session state written to `terminals/<id>/dashboards.json`.
///
/// Until Issue 15-B this also carried `dashboards: Vec<PersistedDashboard>`
/// — dashboard *content* now lives exclusively in the shared
/// [`PersistedDashboardRegistry`] (`<data_dir>/dashboards.json`), so this
/// type only holds what's genuinely per-Terminal-window: which dashboard
/// (by name) this window was last showing, its auto-save preference, and
/// its OS window geometry.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPersist {
    /// User-visible Terminal name (e.g. "Terminal 2").
    #[serde(default)]
    pub name: String,
    pub active_dashboard: String,
    #[serde(default = "default_auto_save")]
    pub auto_save: bool,
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
            window: PersistedWindowConfig::default(),
        }
    }
}

fn default_auto_save() -> bool {
    true
}

// ── Shared dashboard registry (Issue 15-B) ────────────────────────────────────

/// Every dashboard's content, shared process-wide across every Terminal
/// window. Written to `<data_dir>/dashboards.json`. Order matches the
/// in-memory `DashboardRegistry`'s `IndexMap` insertion order — the
/// switcher/Manage-drawer display order.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedDashboardRegistry {
    pub dashboards: Vec<PersistedDashboard>,
}

/// Legacy (pre-15-B) shape of `TerminalPersist`, which bundled each
/// terminal's own dashboards. Used only by `load_or_migrate_registry` to
/// read a per-terminal file's dashboards before the shared registry existed
/// — new code should never construct this.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyTerminalPersist {
    #[serde(default)]
    name: String,
    active_dashboard: String,
    #[serde(default = "default_auto_save")]
    auto_save: bool,
    #[serde(default)]
    dashboards: Vec<PersistedDashboard>,
    #[serde(default)]
    window: PersistedWindowConfig,
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

/// Update only the session fields (`active_dashboard`, `auto_save`),
/// preserving `name` and `window` from the existing file.
///
/// Used by `LayoutTree::schedule_save` so layout auto-saves don't overwrite the
/// terminal name or window position that were written by other commands.
/// Callers that also changed dashboard *content* must separately call
/// `save_registry` — this function only ever touches the per-terminal file.
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
    save_terminal_for(terminal_id, &existing, data_dir)
}

// ── Shared registry: load / save / migrate ────────────────────────────────────

fn registry_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("dashboards.json")
}

/// Load `<data_dir>/dashboards.json`. Returns `None` if it doesn't exist or
/// can't be parsed — callers use this to distinguish "already on the new
/// format" from "needs `load_or_migrate_registry`".
pub fn load_registry(data_dir: &Path) -> Option<PersistedDashboardRegistry> {
    let bytes = std::fs::read(registry_path(data_dir)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// Write `registry` to `<data_dir>/dashboards.json` atomically (write to a
/// `.tmp` file, then rename over the destination).
pub fn save_registry(data_dir: &Path, registry: &PersistedDashboardRegistry) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(registry).map_err(|e| e.to_string())?;
    let tmp = data_dir.join("dashboards.json.tmp");
    let dest = registry_path(data_dir);
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Load the shared registry, running the one-time migration first if it
/// doesn't exist yet but at least one pre-15-B per-terminal dashboard list
/// does. Must be called once, before any `LayoutTree::init` resolves a
/// session against the registry (see `TerminalManager::load_dashboards_registry`).
///
/// Migration order matters for crash-safety: the merged shared file is
/// written first and only once that succeeds are the per-terminal files
/// rewritten (dashboards dropped, active_dashboard repointed at the merged
/// name). If the process dies between those two steps, the next launch
/// finds `dashboards.json` already present and simply loads it — it will
/// not re-migrate or double-merge; at worst a few per-terminal files still
/// carry their now-redundant legacy `dashboards` array, which the trimmed
/// `TerminalPersist` deserializer just ignores.
pub fn load_or_migrate_registry(data_dir: &Path) -> PersistedDashboardRegistry {
    if let Some(existing) = load_registry(data_dir) {
        return existing;
    }

    let terminals_dir = data_dir.join("terminals");
    let Ok(entries) = std::fs::read_dir(&terminals_dir) else {
        // Fresh install — nothing to migrate, nothing to log.
        return PersistedDashboardRegistry::default();
    };

    let mut source_dirs: Vec<(String, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            let file = path.join("dashboards.json");
            if file.exists() {
                let name = path.file_name()?.to_str()?.to_string();
                Some((name, file))
            } else {
                None
            }
        })
        .collect();
    source_dirs.sort();

    if source_dirs.is_empty() {
        // `terminals/` exists but has nothing loadable — also a no-op.
        return PersistedDashboardRegistry::default();
    }

    let mut merged: IndexMap<String, PersistedDashboard> = IndexMap::new();
    // (dir_name, legacy source, renamed active_dashboard if it collided)
    let mut rewrites: Vec<(String, LegacyTerminalPersist, Option<String>)> = Vec::new();
    let mut total_dashboards = 0usize;
    let mut renamed_count = 0usize;

    for (dir_name, file) in &source_dirs {
        let Ok(bytes) = std::fs::read(file) else {
            continue;
        };
        let Ok(legacy) = serde_json::from_slice::<LegacyTerminalPersist>(&bytes) else {
            continue;
        };

        let terminal_label = if legacy.name.trim().is_empty() {
            format!("Terminal {dir_name}")
        } else {
            legacy.name.clone()
        };

        let mut active_rename: Option<String> = None;

        for mut dashboard in legacy.dashboards.clone() {
            total_dashboards += 1;
            let original_name = dashboard.name.clone();
            let mut final_name = original_name.clone();
            if merged.contains_key(&final_name) {
                let mut n = 2;
                final_name = format!("{original_name} (from {terminal_label})");
                while merged.contains_key(&final_name) {
                    final_name = format!("{original_name} (from {terminal_label} {n})");
                    n += 1;
                }
                renamed_count += 1;
                dashboard.name = final_name.clone();
            }
            if original_name == legacy.active_dashboard && final_name != original_name {
                active_rename = Some(final_name.clone());
            }
            merged.insert(final_name, dashboard);
        }

        rewrites.push((dir_name.clone(), legacy, active_rename));
    }

    let registry = PersistedDashboardRegistry {
        dashboards: merged.into_values().collect(),
    };

    // Write the shared file FIRST — only once it's confirmed on disk do we
    // touch any per-terminal file (see crash-safety note above).
    if let Err(e) = save_registry(data_dir, &registry) {
        eprintln!("[layout] migration: failed to write shared dashboards.json: {e}");
        return registry;
    }

    for (dir_name, legacy, active_rename) in &rewrites {
        let trimmed = TerminalPersist {
            name: legacy.name.clone(),
            active_dashboard: active_rename
                .clone()
                .unwrap_or_else(|| legacy.active_dashboard.clone()),
            auto_save: legacy.auto_save,
            window: legacy.window.clone(),
        };
        let terminal_id = if dir_name == "main" {
            "terminal-main".to_string()
        } else {
            format!("terminal-{dir_name}")
        };
        if let Err(e) = save_terminal_for(&terminal_id, &trimmed, data_dir) {
            eprintln!("[layout] migration: failed to trim {dir_name}/dashboards.json: {e}");
        }
    }

    eprintln!(
        "[layout] migrated {} terminal(s) into the shared dashboard registry: \
         {} dashboard(s) found, {} written ({} renamed to avoid name collisions)",
        source_dirs.len(),
        total_dashboards,
        registry.dashboards.len(),
        renamed_count,
    );

    registry
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

    // Migration: convert the old layout.json to a single dashboard, written
    // directly into the shared registry (Issue 15-B — TerminalPersist no
    // longer carries dashboard content at all). This predates even the
    // per-terminal `dashboards.json` era, so it's vanishingly rare; still
    // handled so nobody upgrading straight from that far back loses their
    // one saved layout.
    let old_path = data_dir.join("layout.json");
    if let Ok(bytes) = std::fs::read(&old_path) {
        if let Ok(old) = serde_json::from_slice::<PersistedLayout>(&bytes) {
            let dashboard = PersistedDashboard::from_layout("Default".to_string(), old);
            let mut registry = load_registry(data_dir).unwrap_or_default();
            registry.dashboards.push(dashboard);
            let persist = TerminalPersist {
                active_dashboard: "Default".to_string(),
                auto_save: true,
                ..Default::default()
            };
            if save_registry(data_dir, &registry).is_ok()
                && save_terminal(&persist, data_dir).is_ok()
            {
                let _ = std::fs::remove_file(&old_path);
                eprintln!("[layout] migrated layout.json → dashboards.json");
            }
            return Some(persist);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Fresh, isolated scratch directory per test — avoids needing a
    /// `tempfile` dev-dependency for what's otherwise a couple of file reads.
    fn test_data_dir() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("ot-persist-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a pre-15-B per-terminal `dashboards.json` fixture — the legacy
    /// shape, with `dashboards` embedded, that `load_or_migrate_registry`
    /// must be able to read even though the current `TerminalPersist` no
    /// longer has that field.
    fn write_legacy_terminal(
        data_dir: &Path,
        dir_name: &str,
        name: &str,
        dashboard_names: &[&str],
    ) {
        let dir = data_dir.join("terminals").join(dir_name);
        std::fs::create_dir_all(&dir).unwrap();
        let dashboards_json: Vec<String> = dashboard_names
            .iter()
            .map(|dname| format!(r#"{{"name":"{dname}","tree":null,"meta":{{}}}}"#))
            .collect();
        let active = dashboard_names.first().copied().unwrap_or("Default");
        let json = format!(
            r#"{{"name":"{name}","activeDashboard":"{active}","autoSave":true,"dashboards":[{}],"window":{{"x":0.0,"y":0.0,"width":0.0,"height":0.0}}}}"#,
            dashboards_json.join(","),
        );
        std::fs::write(dir.join("dashboards.json"), json).unwrap();
    }

    #[test]
    fn fresh_install_is_a_no_op() {
        let data_dir = test_data_dir();
        let registry = load_or_migrate_registry(&data_dir);
        assert!(registry.dashboards.is_empty());
        assert!(
            !registry_path(&data_dir).exists(),
            "a fresh install must not write dashboards.json"
        );
    }

    #[test]
    fn single_terminal_migrates_verbatim() {
        let data_dir = test_data_dir();
        write_legacy_terminal(&data_dir, "main", "", &["Trading", "Research"]);

        let registry = load_or_migrate_registry(&data_dir);
        let names: Vec<&str> = registry
            .dashboards
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert_eq!(
            names,
            vec!["Trading", "Research"],
            "a single terminal's dashboards must migrate verbatim, no renames"
        );

        assert!(registry_path(&data_dir).exists());
        let trimmed = load_terminal_for("terminal-main", &data_dir).unwrap();
        assert_eq!(trimmed.active_dashboard, "Trading");
    }

    #[test]
    fn colliding_names_across_terminals_are_disambiguated_not_dropped() {
        let data_dir = test_data_dir();
        write_legacy_terminal(&data_dir, "main", "Terminal 1", &["Main"]);
        write_legacy_terminal(&data_dir, "2", "Terminal 2", &["Main"]);

        let registry = load_or_migrate_registry(&data_dir);
        assert_eq!(
            registry.dashboards.len(),
            2,
            "both same-named dashboards must survive under distinct names"
        );

        let names: Vec<&str> = registry
            .dashboards
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert!(names.contains(&"Main"));
        assert!(
            names.iter().any(|n| n.starts_with("Main (from")),
            "the collision must be disambiguated, e.g. 'Main (from Terminal 2)', got {names:?}"
        );

        let ids: std::collections::HashSet<&str> =
            registry.dashboards.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids.len(), 2, "each survivor must have distinct identity");

        // terminal-2's own file should now point at the renamed copy.
        let terminal_2 = load_terminal_for("terminal-2", &data_dir).unwrap();
        assert_ne!(terminal_2.active_dashboard, "Trading"); // sanity: not empty/garbage
        assert!(terminal_2.active_dashboard.starts_with("Main"));
    }

    #[test]
    fn rerunning_after_migration_does_not_duplicate() {
        let data_dir = test_data_dir();
        write_legacy_terminal(&data_dir, "main", "", &["Trading"]);

        let first = load_or_migrate_registry(&data_dir);
        assert_eq!(first.dashboards.len(), 1);

        let second = load_or_migrate_registry(&data_dir);
        assert_eq!(
            second.dashboards.len(),
            1,
            "re-running after a completed migration must not re-migrate or duplicate"
        );
    }

    #[test]
    fn save_and_load_registry_round_trip() {
        let data_dir = test_data_dir();
        let registry = PersistedDashboardRegistry {
            dashboards: vec![PersistedDashboard::empty("Trading".to_string())],
        };
        save_registry(&data_dir, &registry).unwrap();
        let loaded = load_registry(&data_dir).unwrap();
        assert_eq!(loaded.dashboards.len(), 1);
        assert_eq!(loaded.dashboards[0].name, "Trading");
        assert_eq!(loaded.dashboards[0].id, registry.dashboards[0].id);
    }
}
