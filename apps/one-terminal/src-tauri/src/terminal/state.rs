//! Per-terminal runtime state and shared overlay types.

use std::sync::{Arc, Mutex, RwLock};

use tokio::sync::oneshot;

use crate::layout::persist::PersistedWindowConfig;
use crate::layout::store::LayoutTree;
use crate::webview_pool::WebviewPool;

// ── Overlay webview state ─────────────────────────────────────────────────────

/// Mutable state for the overlay webview (context menus, command palette,
/// overflow dropdowns).
pub struct OverlayInner {
    pub is_ready: bool,
    /// `true` when a content panel was added after the overlay was last
    /// inserted as the topmost child webview — `wm_ctx_menu_open` will
    /// recreate it before showing the next menu.
    pub stale: bool,
    /// Senders waiting for the overlay to call `wm_overlay_ready`. All are
    /// drained and notified at once when readiness is signalled.
    pub wakers: Vec<oneshot::Sender<()>>,
}

impl Default for OverlayInner {
    fn default() -> Self {
        Self {
            is_ready: false,
            stale: false,
            wakers: Vec::new(),
        }
    }
}

pub type OverlayState = Arc<Mutex<OverlayInner>>;

// ── TerminalState ─────────────────────────────────────────────────────────────

/// All runtime state owned by one Terminal OS window.
///
/// Stored as `Arc<TerminalState>` inside [`TerminalManager`]. Layout commands
/// retrieve it by window label so each Terminal operates on its own isolated
/// tree, pool, and overlay.
pub struct TerminalState {
    /// Window label — globally unique; also the prefix for all child webview
    /// labels (`<id>-chrome`, `<id>-overlay`, `<id>-panel-*`, `<id>-pool-*`).
    pub id: String,
    /// User-visible name shown in the title bar and Terminal switcher.
    pub name: RwLock<String>,
    /// Tiling layout tree for this Terminal's active Dashboard.
    pub layout_tree: LayoutTree,
    /// Overlay webview readiness / staleness tracker.
    pub overlay: OverlayState,
    /// Pre-warmed blank webviews ready for immediate navigation.
    pub pool: WebviewPool,
    /// Currently selected FDC3 context channel (`None` = no channel / global).
    pub fdc3_channel: Arc<RwLock<Option<String>>>,
    /// Last-known OS window position and size (logical pixels). Updated by the
    /// window move/resize listener; persisted with 500 ms debounce.
    /// Commands that need the current geometry (e.g. `wm_reset_terminal_position`)
    /// read this directly instead of hitting disk.
    #[allow(dead_code)]
    pub window_config: Arc<RwLock<PersistedWindowConfig>>,
}

// ── TerminalInfo ──────────────────────────────────────────────────────────────

/// Minimal Terminal descriptor — returned by management commands and emitted
/// in `wm:terminals` event payloads.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    pub name: String,
}
