//! Magnetic snapping between popout windows — pure geometry, no
//! Tauri, so it's unit-tested directly.
//!
//! While a popout (or a glued group of them) is dragged, each moving window's
//! edges are tested against the opposite edges of every other popout of the
//! same Terminal. An edge within [`SNAP_PX`] that overlaps the other window
//! along the perpendicular axis pulls the moving windows flush against it;
//! once flush on one axis, nearly-aligned top/bottom (or left/right) edges are
//! lined up too. Windows left touching after the correction are the
//! `contacts` the dragged group glues to on release.
//!
//! All coordinates are physical desktop pixels — the space `outer_position` /
//! `cursor_position` report in — so mixed-DPI monitors don't skew distances;
//! callers scale [`SNAP_PX`] by the dragged window's scale factor.

/// Snap distance, in logical pixels.
pub const SNAP_PX: f64 = 10.0;

/// Distance within which two edges count as touching after a snap (rounding
/// of physical positions).
const TOUCH_EPS: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
    fn shifted(&self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snap {
    /// Correction to add to every moving window's position.
    pub dx: f64,
    pub dy: f64,
    /// Ids of the stationary windows touching a moving one once corrected.
    pub contacts: Vec<String>,
}

/// Positive length of the overlap of `[a0, a1)` and `[b0, b1)`.
fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    a1.min(b1) - a0.max(b0)
}

/// Keep the smaller-magnitude of `best` and `d` if `d` is within `threshold`.
fn consider(best: &mut Option<f64>, d: f64, threshold: f64) {
    if d.abs() <= threshold && best.is_none_or(|b| d.abs() < b.abs()) {
        *best = Some(d);
    }
}

/// Snap correction for `moving` (already at their dragged positions) against
/// the stationary `others`, with `threshold` in the same units as the rects.
pub fn snap(moving: &[Rect], others: &[(String, Rect)], threshold: f64) -> Snap {
    let mut dx: Option<f64> = None;
    let mut dy: Option<f64> = None;
    for m in moving {
        for (_, o) in others {
            // Side by side: vertical extents must overlap.
            if overlap(m.y, m.bottom(), o.y, o.bottom()) > 0.0 {
                consider(&mut dx, o.x - m.right(), threshold); // m's right → o's left
                consider(&mut dx, o.right() - m.x, threshold); // m's left → o's right
            }
            // Stacked: horizontal extents must overlap.
            if overlap(m.x, m.right(), o.x, o.right()) > 0.0 {
                consider(&mut dy, o.y - m.bottom(), threshold); // m's bottom → o's top
                consider(&mut dy, o.bottom() - m.y, threshold); // m's top → o's bottom
            }
        }
    }

    // Line up the other axis against the windows now flush on one axis, so
    // a snapped pair reads as one block rather than a ragged edge.
    let flush_x = |m: &Rect, o: &Rect, dx: f64| {
        let m = m.shifted(dx, 0.0);
        (m.right() - o.x).abs() < TOUCH_EPS || (m.x - o.right()).abs() < TOUCH_EPS
    };
    let flush_y = |m: &Rect, o: &Rect, dy: f64| {
        let m = m.shifted(0.0, dy);
        (m.bottom() - o.y).abs() < TOUCH_EPS || (m.y - o.bottom()).abs() < TOUCH_EPS
    };
    if let (Some(sx), None) = (dx, dy) {
        for m in moving {
            for (_, o) in others.iter().filter(|(_, o)| flush_x(m, o, sx)) {
                consider(&mut dy, o.y - m.y, threshold);
                consider(&mut dy, o.bottom() - m.bottom(), threshold);
            }
        }
    } else if let (None, Some(sy)) = (dx, dy) {
        for m in moving {
            for (_, o) in others.iter().filter(|(_, o)| flush_y(m, o, sy)) {
                consider(&mut dx, o.x - m.x, threshold);
                consider(&mut dx, o.right() - m.right(), threshold);
            }
        }
    }

    let (dx, dy) = (dx.unwrap_or(0.0), dy.unwrap_or(0.0));
    let contacts = others
        .iter()
        .filter(|(_, o)| {
            moving.iter().any(|m| {
                let m = m.shifted(dx, dy);
                (flush_x(&m, o, 0.0) && overlap(m.y, m.bottom(), o.y, o.bottom()) > 0.0)
                    || (flush_y(&m, o, 0.0) && overlap(m.x, m.right(), o.x, o.right()) > 0.0)
            })
        })
        .map(|(id, _)| id.clone())
        .collect();
    Snap { dx, dy, contacts }
}

// ── Shared-edge resizing ──────────────────────────────────────────────────────

/// Edge distance within which glued windows count as sharing an edge line.
const SHARE_EPS: f64 = 2.0;

fn transpose(r: Rect) -> Rect {
    Rect::new(r.y, r.x, r.h, r.w)
}

/// Resize the glued neighbours of a window that was resized from `old` to
/// `new`, so that no gap opens and nothing overlaps: every edge of the window
/// that moved is treated as a line, and every group member with an edge on
/// that line, connected to the window along it, moves its edge to match. A
/// left window shrinks when its right neighbour grows leftwards, and grows
/// when the neighbour shrinks rightwards; top/bottom likewise.
///
/// No member is squeezed below `min_w` × `min_h`: the line stops there, and
/// the resized window itself is corrected to match. A pure move (size
/// unchanged) changes nothing.
///
/// Returns the new rect of every window that changes — the resized window
/// under `id` only when it had to be corrected.
pub fn follow_resize(
    id: &str,
    old: Rect,
    new: Rect,
    group: &[(String, Rect)],
    min_w: f64,
    min_h: f64,
) -> Vec<(String, Rect)> {
    if (new.w - old.w).abs() < 0.5 && (new.h - old.h).abs() < 0.5 {
        return Vec::new();
    }
    let mut others: Vec<(String, Rect)> = group.iter().filter(|(g, _)| g != id).cloned().collect();
    // Horizontal edges first, then the same logic on the transposed rects.
    let cur = follow_axis(old, new, &mut others, min_w);
    let mut t_others: Vec<(String, Rect)> =
        others.into_iter().map(|(g, r)| (g, transpose(r))).collect();
    let cur = transpose(follow_axis(
        transpose(old),
        transpose(cur),
        &mut t_others,
        min_h,
    ));

    let mut changed: Vec<(String, Rect)> = t_others
        .into_iter()
        .map(|(g, r)| (g, transpose(r)))
        .filter(|(g, r)| group.iter().any(|(o, before)| o == g && before != r))
        .collect();
    if cur != new {
        changed.push((id.to_string(), cur));
    }
    changed
}

/// [`follow_resize`] along x only: moves the vertical edge lines of `old`
/// that differ in `new`. Mutates `others`; returns the corrected `new`.
fn follow_axis(old: Rect, new: Rect, others: &mut [(String, Rect)], min: f64) -> Rect {
    let mut cur = new;
    if (new.x - old.x).abs() > 0.5 {
        cur = move_line(old, old.x, new.x, true, cur, others, min);
    }
    if (new.right() - old.right()).abs() > 0.5 {
        cur = move_line(old, old.right(), cur.right(), false, cur, others, min);
    }
    cur
}

/// Move the vertical line at `from` to `to`. `left_edge` says which edge of
/// the resized window (`cur`, previously `old`) sits on it.
fn move_line(
    old: Rect,
    from: f64,
    to: f64,
    left_edge: bool,
    mut cur: Rect,
    others: &mut [(String, Rect)],
    min: f64,
) -> Rect {
    let on_line = |r: &Rect| (r.right() - from).abs() < SHARE_EPS || (r.x - from).abs() < SHARE_EPS;
    // Members on the line, connected to the window along it (touching
    // vertically counts, so a column of stacked windows moves as one).
    let (mut lo, mut hi) = (old.y, old.bottom());
    let mut on: Vec<usize> = Vec::new();
    loop {
        let before = on.len();
        for (i, (_, r)) in others.iter().enumerate() {
            if !on.contains(&i) && on_line(r) && overlap(lo, hi, r.y, r.bottom()) > -SHARE_EPS {
                on.push(i);
                lo = lo.min(r.y);
                hi = hi.max(r.bottom());
            }
        }
        if on.len() == before {
            break;
        }
    }

    // Windows left of the line can't shrink below `min`, nor those right of it.
    let (mut floor, mut ceil) = (f64::NEG_INFINITY, f64::INFINITY);
    if left_edge {
        ceil = ceil.min(cur.right() - min);
    } else {
        floor = floor.max(cur.x + min);
    }
    for &i in &on {
        let r = others[i].1;
        if (r.right() - from).abs() < SHARE_EPS {
            floor = floor.max(r.x + min);
        } else {
            ceil = ceil.min(r.right() - min);
        }
    }
    let p = if floor <= ceil {
        to.clamp(floor, ceil)
    } else {
        from
    };

    let set_left = |r: &mut Rect| {
        let right = r.right();
        r.x = p;
        r.w = right - p;
    };
    if left_edge {
        set_left(&mut cur);
    } else {
        cur.w = p - cur.x;
    }
    for &i in &on {
        let r = &mut others[i].1;
        if (r.right() - from).abs() < SHARE_EPS {
            r.w = p - r.x;
        } else {
            set_left(r);
        }
    }
    cur
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: f64 = SNAP_PX;

    fn other(x: f64, y: f64, w: f64, h: f64) -> Vec<(String, Rect)> {
        vec![("o".into(), Rect::new(x, y, w, h))]
    }

    #[test]
    fn snaps_each_side_flush() {
        let o = other(500.0, 100.0, 300.0, 200.0);
        // Right edge 6 px short of o's left edge.
        let s = snap(&[Rect::new(194.0, 120.0, 300.0, 200.0)], &o, T);
        assert_eq!((s.dx, s.contacts.len()), (6.0, 1));
        // Left edge 8 px past o's right edge.
        let s = snap(&[Rect::new(808.0, 120.0, 200.0, 200.0)], &o, T);
        assert_eq!((s.dx, s.contacts.len()), (-8.0, 1));
        // Bottom edge 5 px above o's top edge.
        let s = snap(&[Rect::new(540.0, -105.0, 200.0, 200.0)], &o, T);
        assert_eq!((s.dy, s.contacts.len()), (5.0, 1));
        // Top edge 3 px below o's bottom edge (overlapping slightly is fine too).
        let s = snap(&[Rect::new(540.0, 297.0, 200.0, 200.0)], &o, T);
        assert_eq!((s.dy, s.contacts.len()), (3.0, 1));
    }

    #[test]
    fn no_snap_beyond_threshold_or_without_perpendicular_overlap() {
        let o = other(500.0, 100.0, 300.0, 200.0);
        let s = snap(&[Rect::new(180.0, 120.0, 300.0, 200.0)], &o, T);
        assert_eq!(s, Snap::default(), "20 px away");
        // Edges within range but entirely above o — diagonal, not adjacent.
        let s = snap(&[Rect::new(195.0, -150.0, 300.0, 200.0)], &o, T);
        assert_eq!(s, Snap::default());
    }

    #[test]
    fn aligns_the_other_axis_once_flush() {
        let o = other(500.0, 100.0, 300.0, 200.0);
        // Flush-able on x, and its top is 7 px below o's top.
        let s = snap(&[Rect::new(195.0, 107.0, 300.0, 250.0)], &o, T);
        assert_eq!((s.dx, s.dy), (5.0, -7.0));
        // Stacked below, right edges 4 px apart.
        let s = snap(&[Rect::new(504.0, 305.0, 300.0, 200.0)], &o, T);
        assert_eq!((s.dx, s.dy), (-4.0, -5.0));
    }

    #[test]
    fn picks_the_nearest_candidate() {
        let others = vec![
            ("far".into(), Rect::new(500.0, 0.0, 100.0, 400.0)),
            ("near".into(), Rect::new(0.0, 0.0, 100.0, 400.0)),
        ];
        // Left edge 3 px from "near", right edge 9 px from "far".
        let s = snap(&[Rect::new(103.0, 50.0, 388.0, 100.0)], &others, T);
        assert_eq!(s.dx, -3.0);
        assert_eq!(s.contacts, vec!["near".to_string()]);
    }

    #[test]
    fn a_moving_group_snaps_by_any_member_and_reports_all_contacts() {
        let others = vec![
            ("a".into(), Rect::new(1000.0, 0.0, 200.0, 200.0)),
            ("b".into(), Rect::new(1000.0, 200.0, 200.0, 200.0)),
        ];
        // Two glued windows stacked; both right edges 6 px short of a/b.
        let moving = [
            Rect::new(694.0, 0.0, 300.0, 200.0),
            Rect::new(694.0, 200.0, 300.0, 200.0),
        ];
        let s = snap(&moving, &others, T);
        assert_eq!(s.dx, 6.0);
        assert_eq!(s.contacts, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn threshold_scales_with_the_dragged_windows_dpi() {
        let o = other(1000.0, 0.0, 400.0, 400.0);
        // 16 physical px apart: outside 10 px at 1×, inside 20 px at 2×.
        let m = [Rect::new(584.0, 0.0, 400.0, 400.0)];
        assert_eq!(snap(&m, &o, SNAP_PX * 1.0), Snap::default());
        assert_eq!(snap(&m, &o, SNAP_PX * 2.0).dx, 16.0);
    }

    const MIN: f64 = 100.0;

    fn get(changed: &[(String, Rect)], id: &str) -> Option<Rect> {
        changed.iter().find(|(g, _)| g == id).map(|(_, r)| *r)
    }

    fn pair() -> Vec<(String, Rect)> {
        vec![
            ("L".into(), Rect::new(0.0, 0.0, 400.0, 300.0)),
            ("R".into(), Rect::new(400.0, 0.0, 400.0, 300.0)),
        ]
    }

    #[test]
    fn growing_right_leftwards_shrinks_left() {
        let g = pair();
        let out = follow_resize(
            "R",
            g[1].1,
            Rect::new(350.0, 0.0, 450.0, 300.0),
            &g,
            MIN,
            MIN,
        );
        assert_eq!(get(&out, "L"), Some(Rect::new(0.0, 0.0, 350.0, 300.0)));
        assert_eq!(get(&out, "R"), None, "no correction needed");
    }

    #[test]
    fn shrinking_right_rightwards_grows_left() {
        let g = pair();
        let out = follow_resize(
            "R",
            g[1].1,
            Rect::new(460.0, 0.0, 340.0, 300.0),
            &g,
            MIN,
            MIN,
        );
        assert_eq!(get(&out, "L"), Some(Rect::new(0.0, 0.0, 460.0, 300.0)));
    }

    #[test]
    fn resizing_left_rightwards_shrinks_right() {
        let g = pair();
        let out = follow_resize("L", g[0].1, Rect::new(0.0, 0.0, 480.0, 300.0), &g, MIN, MIN);
        assert_eq!(get(&out, "R"), Some(Rect::new(480.0, 0.0, 320.0, 300.0)));
    }

    #[test]
    fn top_and_bottom_follow_the_same_way() {
        let g = vec![
            ("T".into(), Rect::new(0.0, 0.0, 400.0, 300.0)),
            ("B".into(), Rect::new(0.0, 300.0, 400.0, 300.0)),
        ];
        let out = follow_resize(
            "B",
            g[1].1,
            Rect::new(0.0, 250.0, 400.0, 350.0),
            &g,
            MIN,
            MIN,
        );
        assert_eq!(get(&out, "T"), Some(Rect::new(0.0, 0.0, 400.0, 250.0)));
        let out = follow_resize("T", g[0].1, Rect::new(0.0, 0.0, 400.0, 340.0), &g, MIN, MIN);
        assert_eq!(get(&out, "B"), Some(Rect::new(0.0, 340.0, 400.0, 260.0)));
    }

    #[test]
    fn a_neighbour_stops_at_its_minimum_and_corrects_the_resized_window() {
        let g = pair();
        // R's left edge dragged to x=50 would leave L 50 wide; it stops at 100.
        let out = follow_resize(
            "R",
            g[1].1,
            Rect::new(50.0, 0.0, 750.0, 300.0),
            &g,
            MIN,
            MIN,
        );
        assert_eq!(get(&out, "L"), Some(Rect::new(0.0, 0.0, 100.0, 300.0)));
        assert_eq!(get(&out, "R"), Some(Rect::new(100.0, 0.0, 700.0, 300.0)));
    }

    #[test]
    fn every_window_on_the_shared_line_moves_together() {
        // L spans the height; R1 above R2 on its right.
        let g = vec![
            ("L".into(), Rect::new(0.0, 0.0, 400.0, 600.0)),
            ("R1".into(), Rect::new(400.0, 0.0, 400.0, 300.0)),
            ("R2".into(), Rect::new(400.0, 300.0, 400.0, 300.0)),
        ];
        let out = follow_resize(
            "R1",
            g[1].1,
            Rect::new(380.0, 0.0, 420.0, 300.0),
            &g,
            MIN,
            MIN,
        );
        assert_eq!(get(&out, "L"), Some(Rect::new(0.0, 0.0, 380.0, 600.0)));
        assert_eq!(get(&out, "R2"), Some(Rect::new(380.0, 300.0, 420.0, 300.0)));
    }

    #[test]
    fn unrelated_edges_and_pure_moves_change_nothing() {
        let g = pair();
        // R's far (right) edge moves — L doesn't share it.
        let out = follow_resize(
            "R",
            g[1].1,
            Rect::new(400.0, 0.0, 500.0, 300.0),
            &g,
            MIN,
            MIN,
        );
        assert!(out.is_empty());
        // Moved without resizing (e.g. the group drag).
        let out = follow_resize(
            "R",
            g[1].1,
            Rect::new(420.0, 10.0, 400.0, 300.0),
            &g,
            MIN,
            MIN,
        );
        assert!(out.is_empty());
    }
}
