//! Drag bridge — IPC command that translates chrome-side pointer coordinates
//! into a tree swap and a real-time reflow.
//!
//! Flow: chrome panel header pointermove → `wm_drag_move(panel_id, window_x,
//! window_y)` → hit-test the layout tree → swap the two leaves → reflow.

use tauri::{AppHandle, State};

use super::node::{Direction, LayoutNode};
use super::reflow::TAB_STRIP_HEIGHT;
use super::store::LayoutTree;
use super::HEADER_HEIGHT;

/// Hit-test the N-ary tree at the given window-space coordinates. Returns the
/// label of the leaf whose rect contains the point, if any.
fn hit_test(node: &LayoutNode, px: f64, py: f64, x: f64, y: f64, w: f64, h: f64) -> Option<String> {
    if px < x || px >= x + w || py < y || py >= y + h {
        return None;
    }
    match node {
        LayoutNode::Leaf { label, .. } => Some(label.clone()),
        LayoutNode::Splitter {
            direction,
            children,
            ..
        } => {
            let total: f64 = children.iter().map(|c| c.weight().max(0.0)).sum();
            if total <= 0.0 {
                return None;
            }
            let mut off = 0.0;
            for c in children {
                let frac = c.weight().max(0.0) / total;
                let (cx, cy, cw, ch) = match direction {
                    Direction::Horizontal => (x + off, y, w * frac, h),
                    Direction::Vertical => (x, y + off, w, h * frac),
                };
                if let Some(l) = hit_test(c, px, py, cx, cy, cw, ch) {
                    return Some(l);
                }
                off += match direction {
                    Direction::Horizontal => w * frac,
                    Direction::Vertical => h * frac,
                };
            }
            None
        }
        LayoutNode::Stack {
            active, children, ..
        } => {
            if children.is_empty() {
                return None;
            }
            let idx = (*active).min(children.len() - 1);
            let cy = y + TAB_STRIP_HEIGHT;
            let ch = (h - TAB_STRIP_HEIGHT).max(0.0);
            hit_test(&children[idx], px, py, x, cy, w, ch)
        }
    }
}

/// Pointer-move while a chrome-drawn panel header is being dragged. Swaps the
/// dragging panel with whatever panel sits under the cursor, then reflows in
/// real-time. Coordinates are window-local — the chrome webview fills the
/// window, so its pointer events already speak window space.
#[tauri::command]
pub fn wm_drag_move(
    panel_id: String,
    window_x: f64,
    window_y: f64,
    tree: State<'_, LayoutTree>,
    app: AppHandle,
) -> Result<(), String> {
    let (w, h) = tree.size();
    let content_x = 0.0;
    let content_y = HEADER_HEIGHT;
    let content_w = w;
    let content_h = (h - HEADER_HEIGHT).max(0.0);

    let target = tree.with_root(|root| {
        hit_test(
            root, window_x, window_y, content_x, content_y, content_w, content_h,
        )
    });
    let Some(Some(target)) = target else {
        return Ok(());
    };
    if target == panel_id {
        return Ok(());
    }

    if !tree.swap_leaves(&panel_id, &target) {
        return Ok(());
    }
    tree.reflow(&app);
    tree.emit_host(&app);
    Ok(())
}
