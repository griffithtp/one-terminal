//! Source-of-truth layout tree.
//!
//! Every node carries a `weight` (relative, percentages computed against
//! sibling sums). Three variants:
//!
//! * `Leaf`     — one WebView identified by its Tauri `label`.
//! * `Splitter` — row (Horizontal) or column (Vertical) of weighted children.
//! * `Stack`    — tabset: children share the same rect; only `active` is visible.
//!
//! JSON is tagged (`"type": "leaf" | "splitter" | "stack"`) so the frontend can
//! ship a FlexLayout-shaped tree straight into `update_layout`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum LayoutNode {
    Leaf {
        /// The Tauri webview label for this panel.
        label: String,
        #[serde(default = "default_weight")]
        weight: f64,
    },
    Splitter {
        direction: Direction,
        #[serde(default = "default_weight")]
        weight: f64,
        #[serde(default)]
        children: Vec<LayoutNode>,
    },
    Stack {
        /// Stable identity for this Stack, preserved across tree mutations
        /// that rebuild the enclosing nodes (extract / insert / simplify).
        /// Ephemeral UI state (e.g. the maximized stack) tracks stacks by
        /// this id rather than by path so it survives adds/removes that
        /// shift sibling indices but don't dissolve the stack itself.
        #[serde(default = "gen_stack_id")]
        id: String,
        #[serde(default = "default_weight")]
        weight: f64,
        /// Index into `children` of the currently-visible tab.
        #[serde(default)]
        active: usize,
        #[serde(default)]
        children: Vec<LayoutNode>,
    },
}

fn default_weight() -> f64 { 1.0 }

pub fn gen_stack_id() -> String {
    uuid::Uuid::new_v4().to_string()[..8].to_string()
}

impl LayoutNode {
    pub fn weight(&self) -> f64 {
        match self {
            LayoutNode::Leaf { weight, .. }
            | LayoutNode::Splitter { weight, .. }
            | LayoutNode::Stack { weight, .. } => *weight,
        }
    }
}
