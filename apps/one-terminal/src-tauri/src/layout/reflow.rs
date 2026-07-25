//! Reflow — recursively stitch webviews to the rectangles the tree assigns.
//!
//! - `Leaf`     → `set_position` + `set_size` on the webview for this rect.
//! - `Splitter` → split the rect along `direction` by each child's weight share.
//! - `Stack`    → the active child fills the rect below the tab strip; every
//!   other child is parked offscreen so it stays loaded but hidden.

use std::collections::HashMap;

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager};

use super::node::{Direction, LayoutNode};
use super::PANEL_HEADER_HEIGHT;

/// Space reserved at the top of a Stack for the tab strip (drawn by the host
/// shell / chrome). The content of the active tab starts at `y + TAB_STRIP`.
pub const TAB_STRIP_HEIGHT: f64 = 28.0;

/// Gap between sibling children of a Splitter — the host shell paints a
/// draggable splitter handle here. Panels are shrunk so their rects don't
/// overlap this gap.
pub const SPLITTER_THICKNESS: f64 = 4.0;

/// Coordinates where inactive-tab webviews are parked — far outside any
/// reasonable window, so they remain loaded but contribute no visuals.
const OFFSCREEN_X: f64 = -20000.0;
const OFFSCREEN_Y: f64 = -20000.0;

pub fn reflow_layout(
    node: &LayoutNode,
    app: &AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    address_bar_extra: &HashMap<String, f64>,
) {
    reflow_inner(node, app, x, y, width, height, false, address_bar_extra);
}

/// Internal reflow walker.
///
/// `in_stack` tracks whether the parent is a `Stack`; if so the parent's tab
/// strip serves as the panel header and we skip the extra `PANEL_HEADER_HEIGHT`
/// reservation. Outside a Stack every Leaf reserves space for a chrome-drawn
/// per-panel header, matching the binary-tree `apply()` pass in `lib.rs`.
///
/// `address_bar_extra` maps a leaf's label to any additional height it
/// reserves below the title header for its own address-bar row (currently
/// only Generic Web Widget panels with `show_address_bar` set; `0.0`
/// otherwise). Precomputed by the caller (`LayoutTree::reflow`) since only
/// it has access to `LeafMeta`. Only applied outside a Stack — matching the
/// scope of the chrome-drawn header itself.
fn reflow_inner(
    node: &LayoutNode,
    app: &AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    in_stack: bool,
    address_bar_extra: &HashMap<String, f64>,
) {
    match node {
        LayoutNode::Leaf { label, .. } => {
            let (py, ph) = if in_stack {
                (y, height)
            } else {
                let extra = address_bar_extra.get(label).copied().unwrap_or(0.0);
                let reserved = PANEL_HEADER_HEIGHT + extra;
                (y + reserved, (height - reserved).max(0.0))
            };
            place(app, label, x, py, width, ph);
        }

        LayoutNode::Splitter {
            direction,
            children,
            ..
        } => {
            let n = children.len();
            if n == 0 {
                return;
            }
            let total: f64 = children.iter().map(|c| c.weight().max(0.0)).sum();
            if total <= 0.0 {
                return;
            }

            let axis_total = match direction {
                Direction::Horizontal => width,
                Direction::Vertical => height,
            };
            let gaps = SPLITTER_THICKNESS * n.saturating_sub(1) as f64;
            let content = (axis_total - gaps).max(0.0);

            let mut offset = 0.0;
            for (i, child) in children.iter().enumerate() {
                let frac = child.weight().max(0.0) / total;
                let axis_share = content * frac;
                match direction {
                    Direction::Horizontal => {
                        reflow_inner(
                            child,
                            app,
                            x + offset,
                            y,
                            axis_share,
                            height,
                            false,
                            address_bar_extra,
                        );
                    }
                    Direction::Vertical => {
                        reflow_inner(
                            child,
                            app,
                            x,
                            y + offset,
                            width,
                            axis_share,
                            false,
                            address_bar_extra,
                        );
                    }
                }
                offset += axis_share;
                if i + 1 < n {
                    offset += SPLITTER_THICKNESS;
                }
            }
        }

        LayoutNode::Stack {
            active, children, ..
        } => {
            if children.is_empty() {
                return;
            }
            let active_idx = (*active).min(children.len() - 1);
            let content_y = y + TAB_STRIP_HEIGHT;
            let content_h = (height - TAB_STRIP_HEIGHT).max(0.0);

            for (i, child) in children.iter().enumerate() {
                if i == active_idx {
                    reflow_inner(
                        child,
                        app,
                        x,
                        content_y,
                        width,
                        content_h,
                        true,
                        address_bar_extra,
                    );
                } else {
                    park_offscreen(child, app);
                }
            }
        }
    }
}

fn place(app: &AppHandle, label: &str, x: f64, y: f64, w: f64, h: f64) {
    let Some(wv) = app.get_webview(label) else {
        return;
    };
    let _ = wv.set_position(LogicalPosition::new(x, y));
    let _ = wv.set_size(LogicalSize::new(w.max(1.0), h.max(1.0)));
}

/// Walk a subtree and park every Leaf offscreen so inactive tabs stay loaded.
pub fn park_offscreen(node: &LayoutNode, app: &AppHandle) {
    match node {
        LayoutNode::Leaf { label, .. } => {
            place(app, label, OFFSCREEN_X, OFFSCREEN_Y, 1.0, 1.0);
        }
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
            for c in children {
                park_offscreen(c, app);
            }
        }
    }
}

/// Park a single webview label offscreen by its label directly (no tree
/// walk) — used when a panel is kept alive across a Dashboard switch and is
/// no longer present in the live tree at all.
pub fn park_label_offscreen(app: &AppHandle, label: &str) {
    place(app, label, OFFSCREEN_X, OFFSCREEN_Y, 1.0, 1.0);
}
