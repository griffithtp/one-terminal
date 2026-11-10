//! Layout persistence — serialise/deserialise the full layout state to disk.
//!
//! Format: `<data_dir>/terminals/<id>/dashboards.json` (`TerminalPersist`,
//! per-window session state: open tabs, active dashboard, geometry) plus
//! the shared registry, one `<data_dir>/dashboards/<dashboard id>.json`
//! per dashboard, which every `one-terminal` process reads and writes.
//!
//! Migration: if the old `<data_dir>/layout.json` exists it is converted
//! in-place to a single-dashboard `TerminalPersist`, written to the new path,
//! and the old file is deleted. Separately, Issue 15-B's
//! `load_or_migrate_registry` handles the one-time upgrade from N isolated
//! per-terminal dashboard lists (each `TerminalPersist.dashboards`, pre-15-B)
//! to the shared registry, and `load_registry` splits the interim single
//! shared `<data_dir>/dashboards.json` into per-dashboard files.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::dashboard::PersistedDashboard;
use super::docking::DropZone;
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_channel_id"
    )]
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

// ── FDC3 user channel ids ─────────────────────────────────────────────────────

/// The 8 FDC3 user channel ids the Desktop Agent seeds by default
/// (`AgentConfig::default_channels` in desktop-agent). Mirrors
/// `FDC3_CHANNELS` in `src/components/fdc3Channels.ts`. Also the allowlist
/// for `wm_set_panel_fdc3_channel`, which interpolates the id into a JS
/// snippet evaluated in the target panel's webview.
pub const USER_CHANNEL_IDS: [&str; 8] = [
    "Red", "Orange", "Yellow", "Green", "Teal", "Blue", "Purple", "Pink",
];

/// Map a legacy `fdc3.channel.N` id (persisted before the Terminal's ids
/// were aligned with the Desktop Agent's) to the Nth user channel. Any other
/// id is returned unchanged.
pub fn normalize_channel_id(id: &str) -> String {
    id.strip_prefix("fdc3.channel.")
        .and_then(|n| n.parse::<usize>().ok())
        .and_then(|n| n.checked_sub(1))
        .and_then(|i| USER_CHANNEL_IDS.get(i))
        .map_or_else(|| id.to_string(), |s| s.to_string())
}

/// Serde `deserialize_with` for persisted channel fields — applies
/// [`normalize_channel_id`] so saved layouts migrate transparently on load.
pub fn deserialize_channel_id<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = Option::<String>::deserialize(deserializer)?;
    Ok(raw.map(|id| normalize_channel_id(&id)))
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
    /// Panels popped out of the tree into their own window. Their
    /// metadata stays in `meta`; they're simply absent from `tree`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub floating: Vec<FloatingPanel>,
}

// ── Floating (popped-out) panels ──────────────────────────────────────────────

/// A panel detached from the tiling tree into its own popout window. The
/// popout window's label is runtime-only and isn't persisted — a fresh one is
/// minted whenever the popout is (re)opened.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FloatingPanel {
    /// Webview label of the panel — unchanged by detaching.
    pub label: String,
    /// Last-known popout window geometry. `width == 0.0` = not yet placed.
    #[serde(default)]
    pub bounds: PersistedWindowConfig,
    /// Where the panel goes back to when docked.
    #[serde(default)]
    pub dock_hint: DockHint,
    /// Glue group: popouts sharing an id were snapped edge to
    /// edge and move together. `None` = not glued. A group always has at
    /// least two members — one left alone is dissolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

/// Where a floating panel returns to on dock, tried in order: back into the
/// Stack with `stack_id` at `index`; else next to `sibling` per `zone`; else
/// as a new tab in the active Stack.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DockHint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(default)]
    pub index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sibling: Option<String>,
    #[serde(default)]
    pub zone: DropZone,
}

// ── Window config ─────────────────────────────────────────────────────────────

/// Saved OS window position and size. Values are logical pixels.
/// Width/height of `0.0` signals "not yet saved" — callers fall back to default
/// geometry and OS-chosen positioning in that case.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
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
/// — dashboard *content* now lives exclusively in the shared registry
/// (`<data_dir>/dashboards/<id>.json`), so this type only holds what's
/// genuinely per-Terminal-window: which dashboards this window has open as
/// tabs, which one (by name) it was last showing, its auto-save preference,
/// and its OS window geometry.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPersist {
    /// User-visible Terminal name (e.g. "Terminal 2").
    #[serde(default)]
    pub name: String,
    pub active_dashboard: String,
    /// Stable ids of the dashboards open as tabs in this window, in tab
    /// order. `None` only for a file written before this field existed —
    /// back then "open" was a single registry-wide flag
    /// (`PersistedDashboard::closed`), so such a window opens every
    /// dashboard not flagged closed (see `DashboardSession::from_persist`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_dashboards: Option<Vec<String>>,
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
            open_dashboards: Some(Vec::new()),
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

/// `terminals/` subdirectory this process's `terminal-main` window persists
/// to, when it isn't the default `main` — see [`set_main_terminal_dir`].
static MAIN_TERMINAL_DIR: OnceLock<String> = OnceLock::new();

/// Persist this process's `terminal-main` window under `terminals/<dir>/`
/// instead of `terminals/main/`. Every `one-terminal` process names its
/// first window `terminal-main`, so a process opened as a *new* Terminal
/// (`OT_NEW_TERMINAL`) calls this with a unique `dir` — otherwise it would
/// share, and overwrite, the session file of whichever process restored
/// `terminals/main/`. The next restore picks `<dir>` up as an ordinary
/// saved Terminal (`list_saved_terminal_ids`). Must be called before
/// anything is loaded or saved; later calls are ignored.
pub fn set_main_terminal_dir(dir: String) {
    let _ = MAIN_TERMINAL_DIR.set(dir);
}

/// Derive the `terminals/` subdirectory name from a terminal window label.
///
/// `"terminal-main"` → `"main"` (or the [`set_main_terminal_dir`] override),
/// `"terminal-2"` → `"2"`, etc.
fn terminal_dir_name(terminal_id: &str) -> &str {
    if terminal_id == "terminal-main" {
        if let Some(dir) = MAIN_TERMINAL_DIR.get() {
            return dir;
        }
    }
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
    let json = serde_json::to_vec(persist).map_err(|e| e.to_string())?;
    write_atomic(&dir.join("dashboards.json"), &json)
}

/// Write `bytes` to `dest` via a temp file + rename, so a reader never sees
/// a half-written file. The temp name carries this process's pid: several
/// `one-terminal` processes share the data dir, and two writing the same
/// destination must not also share a temp file.
fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<(), String> {
    let file_name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("bad path {}", dest.display()))?;
    let tmp = dest.with_file_name(format!("{file_name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

/// Backwards-compatible alias — saves the primary terminal.
pub fn save_terminal(persist: &TerminalPersist, data_dir: &Path) -> Result<(), String> {
    save_terminal_for("terminal-main", persist, data_dir)
}

/// Update only the session fields (`active_dashboard`, `open_dashboards`,
/// `auto_save`), preserving `name` and `window` from the existing file.
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
            // A `set_main_terminal_dir` directory is a random id, not a name
            // worth showing; restore names such a Terminal itself.
            name: if terminal_id == "terminal-main" && MAIN_TERMINAL_DIR.get().is_some() {
                String::new()
            } else {
                terminal_dir_name(terminal_id).to_string()
            },
            ..Default::default()
        });
    existing.active_dashboard = persist.active_dashboard.clone();
    existing.open_dashboards = persist.open_dashboards.clone();
    existing.auto_save = persist.auto_save;
    save_terminal_for(terminal_id, &existing, data_dir)
}

// ── Shared registry: one file per dashboard ──────────────────────────────────
//
// Every dashboard is stored in its own `<data_dir>/dashboards/<id>.json`.
// Several independent `one-terminal` processes share this directory, each
// with its own in-memory copy of the registry, so a save must only ever
// touch what *this* process changed: rewriting a single shared file from a
// stale in-memory copy silently discarded every dashboard another process
// had created since (the lost-update bug the old `dashboards.json` had).
//
// `KNOWN` remembers, per file, the content this process last wrote or read.
// `save_registry` writes a dashboard only when its content differs from
// that, and deletes only files it knows about whose dashboard is gone from
// its registry (i.e. deleted here). `scan_registry_changes` uses the same
// record to pick up what other processes wrote since.

fn legacy_registry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("dashboards.json")
}

fn dashboards_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("dashboards")
}

fn dashboard_path(dir: &Path, id: &str) -> PathBuf {
    // Ids are UUIDs; anything else is defanged so it can't escape `dir`.
    let safe: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{safe}.json"))
}

/// What this process last saw of one dashboard file.
#[derive(Clone, Debug)]
struct FileStamp {
    id: String,
    hash: u64,
    modified: Option<SystemTime>,
    len: u64,
}

fn known_files() -> &'static Mutex<HashMap<PathBuf, FileStamp>> {
    static KNOWN: OnceLock<Mutex<HashMap<PathBuf, FileStamp>>> = OnceLock::new();
    KNOWN.get_or_init(|| Mutex::new(HashMap::new()))
}

fn content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Serialise with object keys sorted. `PersistedDashboard::meta` is a
/// `HashMap`, whose iteration order differs per process — without this,
/// a dashboard read from another process's file would never compare equal
/// to its own bytes, and every save would rewrite it from a possibly stale
/// copy.
fn canonical_bytes(dashboard: &PersistedDashboard) -> Result<Vec<u8>, String> {
    let value = serde_json::to_value(dashboard).map_err(|e| e.to_string())?;
    serde_json::to_vec(&value).map_err(|e| e.to_string())
}

fn stamp(path: &Path, id: &str, hash: u64) -> FileStamp {
    let meta = std::fs::metadata(path).ok();
    FileStamp {
        id: id.to_string(),
        hash,
        modified: meta.as_ref().and_then(|m| m.modified().ok()),
        len: meta.map(|m| m.len()).unwrap_or(0),
    }
}

/// Read and parse one dashboard file, returning it with its content hash.
fn read_dashboard(path: &Path) -> Option<(PersistedDashboard, u64)> {
    let bytes = std::fs::read(path).ok()?;
    let dashboard: PersistedDashboard = serde_json::from_slice(&bytes).ok()?;
    // Hash our canonical form rather than the raw bytes, so a file written
    // by an older build (or by hand) is still recognised as unchanged.
    let hash = content_hash(&canonical_bytes(&dashboard).ok()?);
    Some((dashboard, hash))
}

fn dashboard_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect()
}

/// Load every dashboard from `<data_dir>/dashboards/`, sorted by name.
/// Returns `None` if no registry exists in any format — callers use this
/// to distinguish "already on the new format" from "needs
/// `load_or_migrate_registry`".
///
/// A pre-existing single-file `<data_dir>/dashboards.json` is split into
/// per-dashboard files first (kept as `dashboards.json.migrated`).
pub fn load_registry(data_dir: &Path) -> Option<PersistedDashboardRegistry> {
    let dir = dashboards_dir(data_dir);
    if !dir.is_dir() {
        return migrate_single_file_registry(data_dir);
    }
    let mut known = known_files().lock().unwrap();
    let mut dashboards = Vec::new();
    for path in dashboard_files(&dir) {
        let Some((dashboard, hash)) = read_dashboard(&path) else {
            eprintln!(
                "[layout] skipping unreadable dashboard file {}",
                path.display()
            );
            continue;
        };
        known.insert(path.clone(), stamp(&path, &dashboard.id, hash));
        dashboards.push(dashboard);
    }
    dashboards.sort_by_key(|d| d.name.to_lowercase());
    Some(PersistedDashboardRegistry { dashboards })
}

/// One-time upgrade from the single shared `dashboards.json` to one file
/// per dashboard. Order matters for crash-safety: terminal files gain their
/// `openDashboards` first (from the old file's order — the per-file format
/// has none), then the dashboard files are written, and only then is the
/// old file moved aside. A crash part-way just re-runs this next launch.
fn migrate_single_file_registry(data_dir: &Path) -> Option<PersistedDashboardRegistry> {
    let legacy_path = legacy_registry_path(data_dir);
    let bytes = std::fs::read(&legacy_path).ok()?;
    let registry: PersistedDashboardRegistry = serde_json::from_slice(&bytes).ok()?;
    backfill_open_dashboards(data_dir, &registry);
    if let Err(e) = save_registry(data_dir, &registry) {
        eprintln!("[layout] migration: failed to split dashboards.json: {e}");
        return Some(registry);
    }
    let _ = std::fs::rename(&legacy_path, data_dir.join("dashboards.json.migrated"));
    eprintln!(
        "[layout] migrated dashboards.json → dashboards/ ({} dashboard(s))",
        registry.dashboards.len()
    );
    Some(registry)
}

/// Give every saved Terminal written before per-window open lists existed
/// an explicit `openDashboards`: every dashboard not flagged `closed`, in
/// `registry` order — exactly the tabs it showed before, since "closed" was
/// then one flag shared by all windows.
fn backfill_open_dashboards(data_dir: &Path, registry: &PersistedDashboardRegistry) {
    let open: Vec<String> = registry
        .dashboards
        .iter()
        .filter(|d| !d.closed)
        .map(|d| d.id.clone())
        .collect();
    for path in dashboard_files_in_terminals(data_dir) {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(mut persist) = serde_json::from_slice::<TerminalPersist>(&bytes) else {
            continue;
        };
        if persist.open_dashboards.is_some() {
            continue;
        }
        persist.open_dashboards = Some(open.clone());
        if let Ok(json) = serde_json::to_vec(&persist) {
            if let Err(e) = write_atomic(&path, &json) {
                eprintln!("[layout] migration: {}: {e}", path.display());
            }
        }
    }
}

fn dashboard_files_in_terminals(data_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(data_dir.join("terminals")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path().join("dashboards.json"))
        .filter(|p| p.is_file())
        .collect()
}

/// Write `registry` to `<data_dir>/dashboards/`: each dashboard whose
/// content changed since this process last wrote or read it, and a delete
/// for each file this process knows about whose dashboard is no longer in
/// `registry`. Files this process has never seen (dashboards created by
/// another process since the last scan) are left alone.
pub fn save_registry(data_dir: &Path, registry: &PersistedDashboardRegistry) -> Result<(), String> {
    let dir = dashboards_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut known = known_files().lock().unwrap();
    let mut live: HashSet<PathBuf> = HashSet::with_capacity(registry.dashboards.len());
    let mut first_error = None;

    for dashboard in &registry.dashboards {
        let path = dashboard_path(&dir, &dashboard.id);
        live.insert(path.clone());
        let bytes = canonical_bytes(dashboard)?;
        let hash = content_hash(&bytes);
        if known.get(&path).is_some_and(|s| s.hash == hash) {
            continue;
        }
        match write_atomic(&path, &bytes) {
            Ok(()) => {
                known.insert(path.clone(), stamp(&path, &dashboard.id, hash));
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }

    let deleted: Vec<PathBuf> = known
        .keys()
        .filter(|p| p.parent() == Some(dir.as_path()) && !live.contains(*p))
        .cloned()
        .collect();
    for path in deleted {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                first_error.get_or_insert(e.to_string());
                continue;
            }
        }
        known.remove(&path);
    }

    first_error.map_or(Ok(()), Err)
}

/// A change another process made to the shared registry on disk.
#[derive(Debug)]
pub enum RegistryChange {
    /// Created or modified elsewhere — carries the dashboard as now on disk.
    Upserted(Box<PersistedDashboard>),
    /// Deleted elsewhere — carries the dashboard's id.
    Removed(String),
}

/// Diff `<data_dir>/dashboards/` against what this process last wrote or
/// read, and record the current files as seen. Unchanged files cost one
/// `stat` each (no read), so this is cheap enough to run on window focus.
pub fn scan_registry_changes(data_dir: &Path) -> Vec<RegistryChange> {
    let dir = dashboards_dir(data_dir);
    let mut known = known_files().lock().unwrap();
    let mut changes = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for path in dashboard_files(&dir) {
        seen.insert(path.clone());
        let meta = std::fs::metadata(&path).ok();
        let modified = meta.as_ref().and_then(|m| m.modified().ok());
        let len = meta.map(|m| m.len()).unwrap_or(0);
        if let Some(s) = known.get(&path) {
            if modified.is_some() && s.modified == modified && s.len == len {
                continue;
            }
        }
        let Some((dashboard, hash)) = read_dashboard(&path) else {
            continue;
        };
        let unchanged = known.get(&path).is_some_and(|s| s.hash == hash);
        known.insert(path.clone(), stamp(&path, &dashboard.id, hash));
        if !unchanged {
            changes.push(RegistryChange::Upserted(Box::new(dashboard)));
        }
    }

    let removed: Vec<PathBuf> = known
        .keys()
        .filter(|p| p.parent() == Some(dir.as_path()) && !seen.contains(*p))
        .cloned()
        .collect();
    for path in removed {
        if let Some(s) = known.remove(&path) {
            changes.push(RegistryChange::Removed(s.id));
        }
    }

    changes
}

/// Load the shared registry, running the one-time migration first if it
/// doesn't exist yet but at least one pre-15-B per-terminal dashboard list
/// does. Must be called once, before any `LayoutTree::init` resolves a
/// session against the registry (see `TerminalManager::load_dashboards_registry`).
///
/// Migration order matters for crash-safety: the merged shared registry is
/// written first and only once that succeeds are the per-terminal files
/// rewritten (dashboards dropped, active_dashboard repointed at the merged
/// name). If the process dies between those two steps, the next launch
/// finds the registry already present and simply loads it — it will
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

    let mut source_dirs: Vec<(String, PathBuf)> = entries
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

    // Write the shared registry FIRST — only once it's confirmed on disk do
    // we touch any per-terminal file (see crash-safety note above).
    if let Err(e) = save_registry(data_dir, &registry) {
        eprintln!("[layout] migration: failed to write shared dashboard registry: {e}");
        return registry;
    }

    // Each pre-15-B Terminal showed exactly its own dashboards as tabs.
    let open_ids = |legacy: &LegacyTerminalPersist| -> Vec<String> {
        let own: HashSet<&str> = legacy.dashboards.iter().map(|d| d.id.as_str()).collect();
        registry
            .dashboards
            .iter()
            .filter(|d| own.contains(d.id.as_str()) && !d.closed)
            .map(|d| d.id.clone())
            .collect()
    };

    for (dir_name, legacy, active_rename) in &rewrites {
        let trimmed = TerminalPersist {
            name: legacy.name.clone(),
            active_dashboard: active_rename
                .clone()
                .unwrap_or_else(|| legacy.active_dashboard.clone()),
            open_dashboards: Some(open_ids(legacy)),
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
    let main = terminal_dir_name("terminal-main");
    let mut ids: Vec<String> = entries
        .flatten()
        .filter_map(|e| {
            let path = e.path();
            if path.is_dir() && path.join("dashboards.json").exists() {
                let name = path.file_name()?.to_str()?;
                if name != main {
                    return Some(format!("terminal-{name}"));
                }
            }
            None
        })
        .collect();
    ids.sort();
    ids
}

/// When restoring and there's no `terminals/main/` session, make the first
/// other saved session this process's `terminal-main` window (via
/// [`set_main_terminal_dir`]) instead of opening an extra blank one next
/// to it. Terminals opened from the Desktop Agent each save under their
/// own id, never `main`, so this is the usual case. Must run before
/// anything loads or saves `terminal-main`.
pub fn adopt_saved_terminal_as_main(data_dir: &Path) {
    if load_terminal_for("terminal-main", data_dir).is_some() {
        return;
    }
    if let Some(id) = list_saved_terminal_ids(data_dir).first() {
        set_main_terminal_dir(terminal_dir_name(id).to_string());
    }
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
            let open_dashboards = Some(vec![dashboard.id.clone()]);
            let mut registry = load_registry(data_dir).unwrap_or_default();
            registry.dashboards.push(dashboard);
            let persist = TerminalPersist {
                active_dashboard: "Default".to_string(),
                open_dashboards,
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
    fn normalize_channel_id_maps_legacy_ids() {
        assert_eq!(normalize_channel_id("fdc3.channel.1"), "Red");
        assert_eq!(normalize_channel_id("fdc3.channel.4"), "Green");
        assert_eq!(normalize_channel_id("fdc3.channel.8"), "Pink");
        // Out of range / malformed legacy ids and current ids pass through.
        assert_eq!(normalize_channel_id("fdc3.channel.0"), "fdc3.channel.0");
        assert_eq!(normalize_channel_id("fdc3.channel.9"), "fdc3.channel.9");
        assert_eq!(normalize_channel_id("Blue"), "Blue");
    }

    #[test]
    fn persisted_leaf_meta_migrates_legacy_channel() {
        let m: PersistedLeafMeta = serde_json::from_str(
            r#"{"appId":"a","url":"u","title":"t","fdc3Channel":"fdc3.channel.6"}"#,
        )
        .unwrap();
        assert_eq!(m.fdc3_channel.as_deref(), Some("Blue"));
        let m: PersistedLeafMeta =
            serde_json::from_str(r#"{"appId":"a","url":"u","title":"t"}"#).unwrap();
        assert_eq!(m.fdc3_channel, None);
    }

    #[test]
    fn fresh_install_is_a_no_op() {
        let data_dir = test_data_dir();
        let registry = load_or_migrate_registry(&data_dir);
        assert!(registry.dashboards.is_empty());
        assert!(
            !dashboards_dir(&data_dir).exists(),
            "a fresh install must not write a dashboard registry"
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

        assert!(dashboards_dir(&data_dir).is_dir());
        let trimmed = load_terminal_for("terminal-main", &data_dir).unwrap();
        assert_eq!(trimmed.active_dashboard, "Trading");
        assert_eq!(
            trimmed.open_dashboards.map(|o| o.len()),
            Some(2),
            "a migrated Terminal keeps its own dashboards open"
        );
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

    /// Drop this process's record of `data_dir`'s files, as if a different
    /// process were about to touch them.
    fn forget_known(data_dir: &Path) {
        let dir = dashboards_dir(data_dir);
        known_files()
            .lock()
            .unwrap()
            .retain(|p, _| p.parent() != Some(dir.as_path()));
    }

    fn dashboard_file_count(data_dir: &Path) -> usize {
        dashboard_files(&dashboards_dir(data_dir)).len()
    }

    #[test]
    fn two_processes_saving_never_drop_each_others_dashboards() {
        // Stand-in for two `one-terminal` processes: each has its own
        // in-memory registry loaded at startup, and each saves its whole
        // registry. The second save used to overwrite the shared file and
        // erase the first process's new dashboard.
        let data_dir = test_data_dir();
        let a = PersistedDashboardRegistry {
            dashboards: vec![PersistedDashboard::empty("From A".to_string())],
        };
        let b = PersistedDashboardRegistry {
            dashboards: vec![PersistedDashboard::empty("From B".to_string())],
        };
        save_registry(&data_dir, &a).unwrap();
        // Process B never saw A's file — its cache has no record of it.
        forget_known(&data_dir);
        save_registry(&data_dir, &b).unwrap();

        let loaded = load_registry(&data_dir).unwrap();
        let names: Vec<&str> = loaded.dashboards.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["From A", "From B"]);
    }

    #[test]
    fn save_deletes_only_dashboards_removed_from_the_registry() {
        let data_dir = test_data_dir();
        let keep = PersistedDashboard::empty("Keep".to_string());
        let drop = PersistedDashboard::empty("Drop".to_string());
        let mut registry = PersistedDashboardRegistry {
            dashboards: vec![keep, drop],
        };
        save_registry(&data_dir, &registry).unwrap();
        assert_eq!(dashboard_file_count(&data_dir), 2);

        registry.dashboards.retain(|d| d.name == "Keep");
        save_registry(&data_dir, &registry).unwrap();
        assert_eq!(dashboard_file_count(&data_dir), 1);
    }

    #[test]
    fn scan_reports_dashboards_written_and_deleted_elsewhere() {
        let data_dir = test_data_dir();
        let mine = PersistedDashboard::empty("Mine".to_string());
        save_registry(
            &data_dir,
            &PersistedDashboardRegistry {
                dashboards: vec![mine.clone()],
            },
        )
        .unwrap();
        assert!(
            scan_registry_changes(&data_dir).is_empty(),
            "own writes are not changes"
        );

        // Another process adds a dashboard and deletes ours.
        let theirs = PersistedDashboard::empty("Theirs".to_string());
        let dir = dashboards_dir(&data_dir);
        std::fs::write(
            dashboard_path(&dir, &theirs.id),
            serde_json::to_vec(&theirs).unwrap(),
        )
        .unwrap();
        std::fs::remove_file(dashboard_path(&dir, &mine.id)).unwrap();

        let changes = scan_registry_changes(&data_dir);
        assert_eq!(changes.len(), 2);
        assert!(changes
            .iter()
            .any(|c| matches!(c, RegistryChange::Upserted(d) if d.id == theirs.id)));
        assert!(changes
            .iter()
            .any(|c| matches!(c, RegistryChange::Removed(id) if *id == mine.id)));
        assert!(
            scan_registry_changes(&data_dir).is_empty(),
            "changes are reported once"
        );
    }

    #[test]
    fn single_file_registry_is_split_and_open_tabs_backfilled() {
        let data_dir = test_data_dir();
        let open = PersistedDashboard::empty("Open".to_string());
        let mut closed = PersistedDashboard::empty("Closed".to_string());
        closed.closed = true;
        let legacy = PersistedDashboardRegistry {
            dashboards: vec![open.clone(), closed],
        };
        std::fs::write(
            legacy_registry_path(&data_dir),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        let dir = data_dir.join("terminals").join("main");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("dashboards.json"),
            r#"{"name":"","activeDashboard":"Open","autoSave":true}"#,
        )
        .unwrap();

        let registry = load_or_migrate_registry(&data_dir);
        assert_eq!(registry.dashboards.len(), 2);
        assert_eq!(dashboard_file_count(&data_dir), 2);
        assert!(!legacy_registry_path(&data_dir).exists());

        let main = load_terminal_for("terminal-main", &data_dir).unwrap();
        assert_eq!(main.open_dashboards, Some(vec![open.id]));
    }
}
