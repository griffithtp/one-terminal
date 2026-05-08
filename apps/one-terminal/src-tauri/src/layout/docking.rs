//! Tree mutations for docking — moving Leaves between Stacks/Splitters.
//!
//! Session preservation: a moved Leaf keeps its Tauri webview `label`, so the
//! same webview process is reused — cookies, localStorage, WebSocket
//! connections and any in-page state survive the dock because we never
//! destroy/recreate the webview. The next `reflow_layout` simply repositions
//! it to the rect its new home assigns.
//!
//! The mutation runs as: extract → adjust target path for sibling shift →
//! insert → simplify. Doing simplify only at the very end keeps `target_path`
//! valid through the insert step (the shift adjustment handles the one place
//! indices can move during extract).

use serde::Deserialize;

use super::node::{gen_stack_id, Direction, LayoutNode};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DropZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
}

/// Remove the Leaf with `label` from the tree and simplify the result.
/// Returns `true` if the leaf was found and removed.
pub fn remove_leaf(tree: &mut Option<LayoutNode>, label: &str) -> bool {
    let Some(root) = tree.take() else {
        return false;
    };
    let (remainder, extracted) = extract_leaf_by_label(root, label);
    *tree = remainder.and_then(simplify);
    extracted.is_some()
}

/// Set the `active` index of the Stack at `path`. `index` is clamped to the
/// number of children so out-of-range inputs become the last valid tab.
/// Returns `true` if the node at `path` is a Stack and was updated.
pub fn set_active_tab(tree: &mut Option<LayoutNode>, path: &[usize], index: usize) -> bool {
    let Some(root) = tree.as_mut() else {
        return false;
    };
    let Some(node) = walk_mut(root, path, 0) else {
        return false;
    };
    match node {
        LayoutNode::Stack {
            active, children, ..
        } if !children.is_empty() => {
            *active = index.min(children.len() - 1);
            true
        }
        _ => false,
    }
}

fn walk_mut<'a>(
    node: &'a mut LayoutNode,
    path: &[usize],
    depth: usize,
) -> Option<&'a mut LayoutNode> {
    if depth == path.len() {
        return Some(node);
    }
    let idx = path[depth];
    match node {
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => children
            .get_mut(idx)
            .and_then(|c| walk_mut(c, path, depth + 1)),
        LayoutNode::Leaf { .. } => None,
    }
}

/// Move the Leaf with `source_label` to `target_path` under `zone` semantics.
///
/// When `zone == Center` and `insert_index` is `Some(i)`, the leaf is
/// inserted at position `i` of the target Stack's children (rather than
/// appended). Index is clamped to the post-extraction child count. For
/// same-Stack reorders (source and target are siblings), `i` is adjusted
/// for the sibling-shift from extraction.
///
/// Returns `true` if the tree was mutated.
pub fn move_leaf(
    tree: &mut Option<LayoutNode>,
    source_label: &str,
    target_path: &[usize],
    zone: DropZone,
    insert_index: Option<usize>,
) -> bool {
    let Some(root) = tree.take() else {
        return false;
    };

    // Locate the source leaf so we can adjust target_path for the impending
    // sibling-index shift.
    let mut source_path = Vec::new();
    if !find_leaf_path(&root, source_label, &mut source_path) {
        *tree = Some(root);
        return false;
    }

    // No-op: dropping a Stack's only tab back onto its own center.
    if zone == DropZone::Center && !source_path.is_empty() {
        let parent = &source_path[..source_path.len() - 1];
        if parent == target_path && stack_size(&root, parent) == Some(1) {
            *tree = Some(root);
            return false;
        }
    }

    // Same-stack reorder: account for the source slot disappearing.
    let mut adjusted_insert = insert_index;
    if zone == DropZone::Center && !source_path.is_empty() {
        let parent = &source_path[..source_path.len() - 1];
        if parent == target_path {
            let source_idx = *source_path.last().unwrap();
            if let Some(i) = adjusted_insert.as_mut() {
                if source_idx < *i {
                    *i -= 1;
                }
                // Dropping onto own slot is a no-op.
                if *i == source_idx {
                    *tree = Some(root);
                    return false;
                }
            }
        }
    }

    let mut adjusted_target = target_path.to_vec();
    adjust_for_removal(&source_path, &mut adjusted_target);

    let (remainder, extracted) = extract_leaf_by_label(root, source_label);
    let Some(leaf) = extracted else {
        *tree = remainder;
        return false;
    };
    let Some(mid) = remainder else {
        // Tree only held the dragged leaf — wrap it as the new root.
        *tree = Some(wrap_in_stack(leaf));
        return true;
    };

    let inserted = match insert_at(mid, &adjusted_target, 0, zone, adjusted_insert, leaf) {
        Ok(n) => n,
        Err((recovered, leaf_back)) => fallback_reinsert(recovered, leaf_back),
    };
    *tree = simplify(inserted);
    true
}

// ── Helpers exposed to `store` for panel-insertion paths ────────────────────

/// True iff the node at `path` (empty path = root) is a `Stack`.
pub fn is_stack_at(root: &LayoutNode, path: &[usize]) -> bool {
    let mut node = root;
    for &i in path {
        match node {
            LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
                let Some(next) = children.get(i) else {
                    return false;
                };
                node = next;
            }
            LayoutNode::Leaf { .. } => return false,
        }
    }
    matches!(node, LayoutNode::Stack { .. })
}

/// Append `leaf` as a new child of the Stack at `path` and make it active.
/// Returns `true` if the node at `path` is a Stack and was mutated.
pub fn append_to_stack_at(root: &mut LayoutNode, path: &[usize], leaf: LayoutNode) -> bool {
    let Some(node) = walk_mut(root, path, 0) else {
        return false;
    };
    match node {
        LayoutNode::Stack {
            children, active, ..
        } => {
            let new_idx = children.len();
            children.push(reset_weight(leaf));
            *active = new_idx;
            true
        }
        _ => false,
    }
}

/// Insert `leaf` adjacent to the node at `path` under `zone` semantics. Unlike
/// `move_leaf`, the source leaf is supplied fresh rather than extracted from
/// the tree — used by panel-creation flows.
pub fn add_leaf_as_sibling(
    tree: &mut Option<LayoutNode>,
    path: &[usize],
    zone: DropZone,
    leaf: LayoutNode,
) -> bool {
    let Some(root) = tree.take() else {
        *tree = Some(leaf);
        return true;
    };
    let inserted = match insert_at(root, path, 0, zone, None, leaf) {
        Ok(n) => n,
        Err((recovered, leaf_back)) => fallback_reinsert(recovered, leaf_back),
    };
    *tree = simplify(inserted);
    true
}

// ── tree walks ──────────────────────────────────────────────────────────────

pub fn find_leaf_path(node: &LayoutNode, label: &str, acc: &mut Vec<usize>) -> bool {
    match node {
        LayoutNode::Leaf { label: l, .. } => l == label,
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
            for (i, child) in children.iter().enumerate() {
                acc.push(i);
                if find_leaf_path(child, label, acc) {
                    return true;
                }
                acc.pop();
            }
            false
        }
    }
}

fn stack_size(root: &LayoutNode, path: &[usize]) -> Option<usize> {
    let mut node = root;
    for &i in path {
        match node {
            LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
                node = children.get(i)?;
            }
            LayoutNode::Leaf { .. } => return None,
        }
    }
    match node {
        LayoutNode::Stack { children, .. } => Some(children.len()),
        _ => None,
    }
}

/// If extracting at `source` removes a sibling that comes *before* `target`,
/// decrement the affected index in `target` by 1 so the path still points at
/// the same node post-extraction.
fn adjust_for_removal(source: &[usize], target: &mut Vec<usize>) {
    if source.is_empty() {
        return;
    }
    let pdepth = source.len() - 1; // depth of the parent slot containing the source
    if target.len() > pdepth
        && target[..pdepth] == source[..pdepth]
        && target[pdepth] > source[pdepth]
    {
        target[pdepth] -= 1;
    }
}

fn extract_leaf_by_label(
    node: LayoutNode,
    label: &str,
) -> (Option<LayoutNode>, Option<LayoutNode>) {
    match node {
        LayoutNode::Leaf { label: l, weight } if l == label => {
            (None, Some(LayoutNode::Leaf { label: l, weight }))
        }
        LayoutNode::Leaf { .. } => (Some(node), None),
        LayoutNode::Splitter {
            direction,
            weight,
            children,
        } => {
            let (kept, extracted) = walk_extract(children, label);
            (
                Some(LayoutNode::Splitter {
                    direction,
                    weight,
                    children: kept,
                }),
                extracted,
            )
        }
        LayoutNode::Stack {
            id,
            weight,
            active,
            children,
        } => {
            let (kept, extracted) = walk_extract(children, label);
            let new_active = if kept.is_empty() {
                0
            } else {
                active.min(kept.len() - 1)
            };
            (
                Some(LayoutNode::Stack {
                    id,
                    weight,
                    active: new_active,
                    children: kept,
                }),
                extracted,
            )
        }
    }
}

fn walk_extract(children: Vec<LayoutNode>, label: &str) -> (Vec<LayoutNode>, Option<LayoutNode>) {
    let mut kept = Vec::with_capacity(children.len());
    let mut extracted = None;
    for child in children {
        if extracted.is_none() {
            let (rem, ext) = extract_leaf_by_label(child, label);
            if let Some(r) = rem {
                kept.push(r);
            }
            if let Some(e) = ext {
                extracted = Some(e);
            }
        } else {
            kept.push(child);
        }
    }
    (kept, extracted)
}

/// Normalize the tree: collapse empty containers, unwrap 1-child Splitters,
/// and enforce the FlexLayout-style **alternating-direction** invariant — a
/// Splitter never has a same-direction Splitter as a child; such a child is
/// spliced in place so the tree alternates horizontal / vertical at each
/// level. Weights of the spliced grandchildren are rescaled to preserve the
/// flattened child's share of the parent, so the user-visible boundaries
/// don't jump after a drop.
///
/// Returns None if the entire subtree evaporated (no Leaves left).
fn simplify(node: LayoutNode) -> Option<LayoutNode> {
    match node {
        LayoutNode::Leaf { .. } => Some(node),
        LayoutNode::Splitter {
            direction,
            weight,
            children,
        } => {
            // Simplify each child bottom-up so when we inspect them for
            // same-direction flattening they're already normalized.
            let simplified: Vec<LayoutNode> = children.into_iter().filter_map(simplify).collect();

            let mut kids: Vec<LayoutNode> = Vec::with_capacity(simplified.len());
            for c in simplified {
                match c {
                    LayoutNode::Splitter {
                        direction: cd,
                        weight: cw,
                        children: gc,
                    } if cd == direction => {
                        // Same-direction Splitter child — splice its
                        // grandchildren into us, rescaling their weights so
                        // their sum equals the child's weight (`cw`). That
                        // preserves relative sizing across the flatten.
                        let cw = cw.max(0.0);
                        let gc_total: f64 = gc.iter().map(|g| g.weight().max(0.0)).sum();
                        if gc_total <= 0.0 || cw <= 0.0 {
                            // Degenerate weights — splice with uniform share.
                            let fallback = if gc.is_empty() {
                                0.0
                            } else {
                                cw.max(1.0) / gc.len() as f64
                            };
                            for mut g in gc {
                                set_weight(&mut g, fallback);
                                kids.push(g);
                            }
                        } else {
                            let scale = cw / gc_total;
                            for mut g in gc {
                                let new_w = g.weight().max(0.0) * scale;
                                set_weight(&mut g, new_w);
                                kids.push(g);
                            }
                        }
                    }
                    other => kids.push(other),
                }
            }

            match kids.len() {
                0 => None,
                1 => {
                    let mut only = kids.into_iter().next().unwrap();
                    set_weight(&mut only, weight);
                    Some(only)
                }
                _ => Some(LayoutNode::Splitter {
                    direction,
                    weight,
                    children: kids,
                }),
            }
        }
        LayoutNode::Stack {
            id,
            weight,
            active,
            children,
        } => {
            let kids: Vec<_> = children.into_iter().filter_map(simplify).collect();
            // Stack-only-holds-Leaves invariant: any Splitter/Stack child is
            // dissolved into its leaf descendants so the host projection never
            // needs to render a nested placeholder tab.
            let mut leaves: Vec<LayoutNode> = Vec::with_capacity(kids.len());
            for k in kids {
                collect_leaves(k, &mut leaves);
            }
            if leaves.is_empty() {
                return None;
            }
            Some(LayoutNode::Stack {
                id,
                weight,
                active: active.min(leaves.len() - 1),
                children: leaves,
            })
        }
    }
}

fn collect_leaves(node: LayoutNode, acc: &mut Vec<LayoutNode>) {
    match node {
        LayoutNode::Leaf { .. } => acc.push(node),
        LayoutNode::Splitter { children, .. } | LayoutNode::Stack { children, .. } => {
            for c in children {
                collect_leaves(c, acc);
            }
        }
    }
}

// ── insertion ───────────────────────────────────────────────────────────────

type InsertErr = (LayoutNode, LayoutNode);

fn insert_at(
    node: LayoutNode,
    path: &[usize],
    depth: usize,
    zone: DropZone,
    insert_index: Option<usize>,
    leaf: LayoutNode,
) -> Result<LayoutNode, InsertErr> {
    if depth == path.len() {
        return Ok(place_leaf(node, zone, insert_index, leaf));
    }

    let idx = path[depth];
    match node {
        LayoutNode::Leaf { .. } => Err((node, leaf)),
        LayoutNode::Splitter {
            direction,
            weight,
            mut children,
        } => {
            if idx >= children.len() {
                return Err((
                    LayoutNode::Splitter {
                        direction,
                        weight,
                        children,
                    },
                    leaf,
                ));
            }
            let child = children.remove(idx);
            match insert_at(child, path, depth + 1, zone, insert_index, leaf) {
                Ok(new_child) => {
                    children.insert(idx, new_child);
                    Ok(LayoutNode::Splitter {
                        direction,
                        weight,
                        children,
                    })
                }
                Err((c_back, l_back)) => {
                    children.insert(idx, c_back);
                    Err((
                        LayoutNode::Splitter {
                            direction,
                            weight,
                            children,
                        },
                        l_back,
                    ))
                }
            }
        }
        LayoutNode::Stack {
            id,
            weight,
            active,
            mut children,
        } => {
            if idx >= children.len() {
                return Err((
                    LayoutNode::Stack {
                        id,
                        weight,
                        active,
                        children,
                    },
                    leaf,
                ));
            }
            let child = children.remove(idx);
            match insert_at(child, path, depth + 1, zone, insert_index, leaf) {
                Ok(new_child) => {
                    children.insert(idx, new_child);
                    Ok(LayoutNode::Stack {
                        id,
                        weight,
                        active,
                        children,
                    })
                }
                Err((c_back, l_back)) => {
                    children.insert(idx, c_back);
                    Err((
                        LayoutNode::Stack {
                            id,
                            weight,
                            active,
                            children,
                        },
                        l_back,
                    ))
                }
            }
        }
    }
}

fn place_leaf(
    node: LayoutNode,
    zone: DropZone,
    insert_index: Option<usize>,
    leaf: LayoutNode,
) -> LayoutNode {
    match zone {
        DropZone::Center => match node {
            LayoutNode::Stack {
                id,
                weight,
                mut children,
                ..
            } => {
                // Clamp the requested index to a valid insertion slot
                // (0..=children.len()). `None` → append.
                let idx = insert_index
                    .map(|i| i.min(children.len()))
                    .unwrap_or(children.len());
                children.insert(idx, reset_weight(leaf));
                LayoutNode::Stack {
                    id,
                    weight,
                    active: idx,
                    children,
                }
            }
            other => {
                // Wrapping a non-Stack into a new Stack — `insert_index` is
                // meaningful only when `idx == 0` (leaf goes before target)
                // vs `idx >= 1` (leaf goes after, which is the default).
                let leaf_first = matches!(insert_index, Some(0));
                let outer_weight = other.weight();
                let (children, active) = if leaf_first {
                    (vec![reset_weight(leaf), reset_weight(other)], 0)
                } else {
                    (vec![reset_weight(other), reset_weight(leaf)], 1)
                };
                LayoutNode::Stack {
                    id: gen_stack_id(),
                    weight: outer_weight,
                    active,
                    children,
                }
            }
        },
        DropZone::Left | DropZone::Right => wrap_split(
            node,
            leaf,
            Direction::Horizontal,
            matches!(zone, DropZone::Left),
        ),
        DropZone::Top | DropZone::Bottom => wrap_split(
            node,
            leaf,
            Direction::Vertical,
            matches!(zone, DropZone::Top),
        ),
    }
}

fn wrap_split(
    target: LayoutNode,
    leaf: LayoutNode,
    direction: Direction,
    leaf_first: bool,
) -> LayoutNode {
    let outer_weight = target.weight();
    let target_inner = reset_weight(target);
    let leaf_inner = wrap_in_stack(reset_weight(leaf));
    let children = if leaf_first {
        vec![leaf_inner, target_inner]
    } else {
        vec![target_inner, leaf_inner]
    };
    LayoutNode::Splitter {
        direction,
        weight: outer_weight,
        children,
    }
}

fn fallback_reinsert(tree: LayoutNode, leaf: LayoutNode) -> LayoutNode {
    LayoutNode::Splitter {
        direction: Direction::Horizontal,
        weight: 1.0,
        children: vec![reset_weight(tree), wrap_in_stack(reset_weight(leaf))],
    }
}

fn wrap_in_stack(leaf: LayoutNode) -> LayoutNode {
    LayoutNode::Stack {
        id: gen_stack_id(),
        weight: 1.0,
        active: 0,
        children: vec![leaf],
    }
}

fn reset_weight(mut n: LayoutNode) -> LayoutNode {
    set_weight(&mut n, 1.0);
    n
}

fn set_weight(node: &mut LayoutNode, w: f64) {
    match node {
        LayoutNode::Leaf { weight, .. }
        | LayoutNode::Splitter { weight, .. }
        | LayoutNode::Stack { weight, .. } => *weight = w,
    }
}
