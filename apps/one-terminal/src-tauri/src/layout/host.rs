//! Projection of the layout tree into the rects the host-shell chrome paints:
//! tab strips (one per `Stack`) and resizer bars (one per gap inside a
//! `Splitter`). Emitted to the chrome as `wm:host-layout` so the TS side can
//! render them as transparent overlays that line up perfectly with the holes
//! left by the reflow pass.

use std::collections::HashMap;

use serde::Serialize;

use super::node::{Direction, LayoutNode};
use super::reflow::{SPLITTER_THICKNESS, TAB_STRIP_HEIGHT};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostLayout {
    pub window_width: f64,
    pub window_height: f64,
    pub stacks: Vec<StackHeader>,
    pub splitters: Vec<SplitterHandle>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackHeader {
    /// Indices from the root to this Stack node — used later for targeted
    /// mutations (active-tab change, drop-here, etc.).
    pub path: Vec<usize>,
    /// Full stack rect (tab strip + content area) in window-space pixels.
    /// Drop-zone hit-testing in the chrome uses this rect.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Height of the tab strip portion at the top of the stack rect; the
    /// content area starts at `y + tab_strip_height`.
    pub tab_strip_height: f64,
    pub active: usize,
    pub tabs: Vec<StackTab>,
    /// `true` when this stack is the currently-maximized one; the frontend
    /// swaps the maximize button for a restore button based on this flag.
    pub maximized: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTab {
    /// Webview label — stable identity for rename/close commands.
    pub label: String,
    /// App-provided title (from the App Directory record or `wm_open` arg).
    pub title: String,
    /// FDC3 App Directory identifier — lets the tab strip render the same
    /// custom header content as the single-panel overlay (e.g. LIVE badge).
    pub app_id: String,
    /// User-set display name override. `None` means show `title`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Webview zoom multiplier; `1.0` is 100 %.
    pub zoom_factor: f64,
    /// FDC3 user channel this tab is joined to. `None` = no channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fdc3_channel: Option<String>,
    /// `true` when this tab is flagged to keep running in the background
    /// across Dashboard switches instead of being destroyed/recreated.
    #[serde(default)]
    pub keep_alive: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitterHandle {
    /// Path to the Splitter node.
    pub path: Vec<usize>,
    /// Handle sits between `children[child_index]` and `children[child_index + 1]`.
    pub child_index: usize,
    pub direction: Direction,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[allow(clippy::too_many_arguments)]
pub fn compute_host_layout(
    root: Option<&LayoutNode>,
    maximized_path: Option<&[usize]>,
    origin_x: f64,
    origin_y: f64,
    content_w: f64,
    content_h: f64,
    titles: &HashMap<String, String>,
    app_ids: &HashMap<String, String>,
    display_names: &HashMap<String, Option<String>>,
    zoom_factors: &HashMap<String, f64>,
    fdc3_channels: &HashMap<String, Option<String>>,
    keep_alives: &HashMap<String, bool>,
) -> HostLayout {
    let mut stacks = Vec::new();
    let mut splitters = Vec::new();
    if let Some(root) = root {
        if let Some(path) = maximized_path {
            // Caller validated `path` resolves to a Stack. Emit only that
            // stack at the full content rect and skip splitters — no other
            // strips/handles should appear while maximized.
            if let Some(LayoutNode::Stack {
                active, children, ..
            }) = resolve(root, path)
            {
                let tabs = stack_tabs(
                    children,
                    titles,
                    app_ids,
                    display_names,
                    zoom_factors,
                    fdc3_channels,
                    keep_alives,
                );
                stacks.push(StackHeader {
                    path: path.to_vec(),
                    x: origin_x,
                    y: origin_y,
                    width: content_w,
                    height: content_h,
                    tab_strip_height: TAB_STRIP_HEIGHT,
                    active: *active,
                    tabs,
                    maximized: true,
                });
            }
        } else {
            walk(
                root,
                &mut Vec::new(),
                origin_x,
                origin_y,
                content_w,
                content_h,
                &mut stacks,
                &mut splitters,
                titles,
                app_ids,
                display_names,
                zoom_factors,
                fdc3_channels,
                keep_alives,
            );
        }
    }
    HostLayout {
        window_width: origin_x + content_w,
        window_height: origin_y + content_h,
        stacks,
        splitters,
    }
}

fn resolve<'a>(root: &'a LayoutNode, path: &[usize]) -> Option<&'a LayoutNode> {
    let mut node = root;
    for &i in path {
        match node {
            LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
                node = children.get(i)?;
            }
            LayoutNode::Leaf { .. } => return None,
        }
    }
    Some(node)
}

fn stack_tabs(
    children: &[LayoutNode],
    titles: &HashMap<String, String>,
    app_ids: &HashMap<String, String>,
    display_names: &HashMap<String, Option<String>>,
    zoom_factors: &HashMap<String, f64>,
    fdc3_channels: &HashMap<String, Option<String>>,
    keep_alives: &HashMap<String, bool>,
) -> Vec<StackTab> {
    children
        .iter()
        .filter_map(|c| match c {
            LayoutNode::Leaf { label, .. } => {
                let title = titles
                    .get(label)
                    .cloned()
                    .filter(|t| !t.is_empty())
                    .unwrap_or_else(|| label.clone());
                let app_id = app_ids.get(label).cloned().unwrap_or_default();
                let display_name = display_names.get(label).and_then(|v| v.clone());
                let zoom_factor = zoom_factors.get(label).copied().unwrap_or(1.0);
                let fdc3_channel = fdc3_channels.get(label).and_then(|v| v.clone());
                let keep_alive = keep_alives.get(label).copied().unwrap_or(false);
                Some(StackTab {
                    label: label.clone(),
                    title,
                    app_id,
                    display_name,
                    zoom_factor,
                    fdc3_channel,
                    keep_alive,
                })
            }
            _ => None,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn walk(
    node: &LayoutNode,
    path: &mut Vec<usize>,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    stacks: &mut Vec<StackHeader>,
    splitters: &mut Vec<SplitterHandle>,
    titles: &HashMap<String, String>,
    app_ids: &HashMap<String, String>,
    display_names: &HashMap<String, Option<String>>,
    zoom_factors: &HashMap<String, f64>,
    fdc3_channels: &HashMap<String, Option<String>>,
    keep_alives: &HashMap<String, bool>,
) {
    match node {
        LayoutNode::Leaf { .. } => {}

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
                Direction::Horizontal => w,
                Direction::Vertical => h,
            };
            let gaps = SPLITTER_THICKNESS * n.saturating_sub(1) as f64;
            let content = (axis_total - gaps).max(0.0);

            let mut offset = 0.0;
            for (i, child) in children.iter().enumerate() {
                let frac = child.weight().max(0.0) / total;
                let axis_share = content * frac;
                let (cx, cy, cw, ch) = match direction {
                    Direction::Horizontal => (x + offset, y, axis_share, h),
                    Direction::Vertical => (x, y + offset, w, axis_share),
                };
                path.push(i);
                walk(
                    child,
                    path,
                    cx,
                    cy,
                    cw,
                    ch,
                    stacks,
                    splitters,
                    titles,
                    app_ids,
                    display_names,
                    zoom_factors,
                    fdc3_channels,
                    keep_alives,
                );
                path.pop();
                offset += axis_share;

                if i + 1 < n {
                    let (hx, hy, hw, hh) = match direction {
                        Direction::Horizontal => (x + offset, y, SPLITTER_THICKNESS, h),
                        Direction::Vertical => (x, y + offset, w, SPLITTER_THICKNESS),
                    };
                    splitters.push(SplitterHandle {
                        path: path.clone(),
                        child_index: i,
                        direction: *direction,
                        x: hx,
                        y: hy,
                        width: hw,
                        height: hh,
                    });
                    offset += SPLITTER_THICKNESS;
                }
            }
        }

        LayoutNode::Stack {
            active, children, ..
        } => {
            // `simplify()` guarantees a Stack's children are all Leaves, so
            // non-Leaf cases are unreachable here.
            let tabs = stack_tabs(
                children,
                titles,
                app_ids,
                display_names,
                zoom_factors,
                fdc3_channels,
                keep_alives,
            );

            stacks.push(StackHeader {
                path: path.clone(),
                x,
                y,
                width: w,
                height: h,
                tab_strip_height: TAB_STRIP_HEIGHT,
                active: *active,
                tabs,
                maximized: false,
            });

            if !children.is_empty() {
                let idx = (*active).min(children.len() - 1);
                path.push(idx);
                walk(
                    &children[idx],
                    path,
                    x,
                    y + TAB_STRIP_HEIGHT,
                    w,
                    (h - TAB_STRIP_HEIGHT).max(0.0),
                    stacks,
                    splitters,
                    titles,
                    app_ids,
                    display_names,
                    zoom_factors,
                    fdc3_channels,
                    keep_alives,
                );
                path.pop();
            }
        }
    }
}
