//! Tiling layout engine.
//!
//! The layout is an N-ary tree of `Leaf`, `Splitter`, and `Stack` nodes —
//! see [`node::LayoutNode`] and the in-memory source of truth at
//! [`store::LayoutTree`]. This module exposes only the shared primitives
//! (constants, split-direction enum, snapshot shape) that the command layer
//! and frontend contracts depend on.

pub mod commands;
pub mod dashboard;
pub mod docking;
pub mod drag;
pub mod host;
pub mod node;
pub mod persist;
pub mod reflow;
pub mod store;

use serde::{Deserialize, Serialize};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Height of the global chrome toolbar at the top of the window. Layout rects
/// start at `y = HEADER_HEIGHT` so panels don't overlap the toolbar.
pub const HEADER_HEIGHT: f64 = 40.0;
/// Height of the chrome-drawn per-panel header (title / drag / close),
/// reserved at the top of every `PanelBounds` rect.
pub const PANEL_HEADER_HEIGHT: f64 = 28.0;

// ── Split direction ───────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SplitDir {
    Horizontal,
    Vertical,
}

// ── Snapshot types (serialised to the chrome frontend on `wm:layout`) ─────────
//
// The snapshot is the legacy projection used by per-panel chrome headers and
// header chips. Stack members are deliberately omitted from `panels` — they
// get their headers from the tab strip emitted via `wm:host-layout`.

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelBounds {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub title: String,
    pub app_id: String,
    pub url: String,
    /// Engine this panel was opened for (if any). The frontend can surface
    /// this as a small badge on the tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine_binding: Option<ot_core::engine::EngineBinding>,
    /// User-set display name override. `None` means show `title`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Webview zoom multiplier; `1.0` is 100 %.
    #[serde(default = "default_zoom")]
    pub zoom_factor: f64,
    /// FDC3 user channel this panel is joined to. `None` = no channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fdc3_channel: Option<String>,
}

fn default_zoom() -> f64 {
    1.0
}

/// Retained for frontend type compatibility. Always emitted empty by the
/// N-ary layout — resize handles are surfaced as `SplitterHandle`s on
/// `wm:host-layout` instead.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DividerBounds {
    pub split_id: String,
    pub dir: SplitDir,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSnapshot {
    pub panels: Vec<PanelBounds>,
    pub dividers: Vec<DividerBounds>,
    pub window_width: f64,
    pub window_height: f64,
}

impl LayoutSnapshot {
    /// Zero-panel placeholder returned when a command does not produce a
    /// local layout change (e.g. an open that was delegated to a sibling
    /// process). The caller's dimensions are unknown at that point, so the
    /// window size is left at 0.
    pub fn empty() -> Self {
        Self {
            panels: Vec::new(),
            dividers: Vec::new(),
            window_width: 0.0,
            window_height: 0.0,
        }
    }
}
