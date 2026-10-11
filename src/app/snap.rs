//! Window snapping and docking geometry, in logical screen points.
//!
//! As in Winamp: a dragged window snaps to any edge of the others — side
//! by side or aligned — and to the screen's edges, when within
//! [`SNAP`] points. Windows whose edges touch are attached; dragging the
//! main window carries everything attached to it, directly or through
//! another window. Pure: the app feeds it rectangles.

/// How close, in points, an edge must come to another to snap to it.
pub const SNAP: f64 = 10.0;
/// Edges this close count as touching: positions pass through physical
/// pixels, so a snapped pair can end up a rounding error apart.
const TOUCH: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    fn right(&self) -> f64 {
        self.x + self.w
    }
    fn bottom(&self) -> f64 {
        self.y + self.h
    }
    fn moved(&self, dx: f64, dy: f64) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// Overlap of the spans `[a0, a1]` and `[b0, b1]`; negative for a gap.
fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    a1.min(b1) - a0.max(b0)
}

/// `a` and `b` share a stretch of edge: one sits flush against the other.
pub fn touching(a: &Rect, b: &Rect) -> bool {
    let side = ((a.right() - b.x).abs() <= TOUCH || (b.right() - a.x).abs() <= TOUCH)
        && overlap(a.y, a.bottom(), b.y, b.bottom()) > 0.0;
    let stacked = ((a.bottom() - b.y).abs() <= TOUCH || (b.bottom() - a.y).abs() <= TOUCH)
        && overlap(a.x, a.right(), b.x, b.right()) > 0.0;
    side || stacked
}

/// Indices of every rectangle attached to `start`, through any chain of
/// touching ones, `start` included.
pub fn attached(start: usize, rects: &[Rect]) -> Vec<usize> {
    let mut group = vec![start];
    let mut i = 0;
    while i < group.len() {
        let at = group[i];
        for (j, r) in rects.iter().enumerate() {
            if !group.contains(&j) && touching(&rects[at], r) {
                group.push(j);
            }
        }
        i += 1;
    }
    group
}

/// The nearest of `candidates` to zero, if any is within [`SNAP`].
fn nearest(candidates: impl Iterator<Item = f64>) -> Option<f64> {
    candidates
        .filter(|d| d.abs() <= SNAP)
        .min_by(|a, b| a.abs().total_cmp(&b.abs()))
}

/// Where `moving` — a group dragged as one, by `(dx, dy)` from where it
/// stands — should go instead: the offset adjusted so that, on each axis
/// independently, the nearest edge within reach of an edge of `fixed` or
/// of a screen lines up with it.
pub fn snap(moving: &[Rect], (dx, dy): (f64, f64), fixed: &[Rect], screens: &[Rect]) -> (f64, f64) {
    let moved: Vec<Rect> = moving.iter().map(|r| r.moved(dx, dy)).collect();
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for m in &moved {
        for f in fixed {
            // Only edges near each other on the other axis attract: a
            // window across the screen should not pull.
            if overlap(m.y, m.bottom(), f.y, f.bottom()) >= -SNAP {
                for (from, to) in [
                    (m.x, f.right()),
                    (m.right(), f.x),
                    (m.x, f.x),
                    (m.right(), f.right()),
                ] {
                    xs.push(to - from);
                }
            }
            if overlap(m.x, m.right(), f.x, f.right()) >= -SNAP {
                for (from, to) in [
                    (m.y, f.bottom()),
                    (m.bottom(), f.y),
                    (m.y, f.y),
                    (m.bottom(), f.bottom()),
                ] {
                    ys.push(to - from);
                }
            }
        }
        // Screens hold windows in: their inside edges attract.
        for s in screens {
            xs.extend([s.x - m.x, s.right() - m.right()]);
            ys.extend([s.y - m.y, s.bottom() - m.bottom()]);
        }
    }
    (
        dx + nearest(xs.into_iter()).unwrap_or(0.0),
        dy + nearest(ys.into_iter()).unwrap_or(0.0),
    )
}

/// How far each other rectangle must move when `rects[changed]` grows or
/// shrinks to `new` from its top-left corner: those attached below it
/// follow its bottom edge, those attached to its right follow its right
/// edge, and so on down the chain. Returns `(index, dx, dy)` for every
/// rectangle that moves.
pub fn follow_resize(changed: usize, new: Rect, rects: &[Rect]) -> Vec<(usize, f64, f64)> {
    let old = rects[changed];
    let (dx, dy) = (new.right() - old.right(), new.bottom() - old.bottom());
    let mut moves: Vec<(usize, f64, f64)> = Vec::new();
    // Rectangles hanging off the old bottom edge, then everything hanging
    // off those, move down by dy; likewise off the right edge by dx.
    let chain = |is_next: &dyn Fn(&Rect, &Rect) -> bool| {
        let mut found = vec![changed];
        let mut i = 0;
        while i < found.len() {
            let at = rects[found[i]];
            for (j, r) in rects.iter().enumerate() {
                if !found.contains(&j) && is_next(&at, r) {
                    found.push(j);
                }
            }
            i += 1;
        }
        found.remove(0);
        found
    };
    let below = |a: &Rect, b: &Rect| {
        (a.bottom() - b.y).abs() <= TOUCH && overlap(a.x, a.right(), b.x, b.right()) > 0.0
    };
    let right_of = |a: &Rect, b: &Rect| {
        (a.right() - b.x).abs() <= TOUCH && overlap(a.y, a.bottom(), b.y, b.bottom()) > 0.0
    };
    if dy != 0.0 {
        for j in chain(&below) {
            moves.push((j, 0.0, dy));
        }
    }
    if dx != 0.0 {
        for j in chain(&right_of) {
            match moves.iter_mut().find(|m| m.0 == j) {
                Some(m) => m.1 = dx,
                None => moves.push((j, dx, 0.0)),
            }
        }
    }
    moves
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = Rect::new(100.0, 100.0, 275.0, 116.0);
    const SCREEN: Rect = Rect::new(0.0, 25.0, 1440.0, 875.0);

    #[test]
    fn stacked_and_side_by_side_windows_touch() {
        let below = Rect::new(100.0, 216.0, 275.0, 116.0);
        let beside = Rect::new(375.0, 150.0, 275.0, 116.0);
        let corner = Rect::new(375.0, 216.0, 275.0, 116.0);
        let apart = Rect::new(100.0, 220.0, 275.0, 116.0);
        assert!(touching(&MAIN, &below));
        assert!(touching(&MAIN, &beside));
        assert!(
            !touching(&MAIN, &corner),
            "meeting at a corner is not touching"
        );
        assert!(!touching(&MAIN, &apart));
    }

    #[test]
    fn attachment_is_transitive() {
        let rects = [
            MAIN,
            Rect::new(100.0, 216.0, 275.0, 116.0), // EQ under main
            Rect::new(375.0, 216.0, 275.0, 232.0), // playlist right of EQ
            Rect::new(900.0, 600.0, 275.0, 116.0), // off on its own
        ];
        let mut group = attached(0, &rects);
        group.sort_unstable();
        assert_eq!(group, vec![0, 1, 2]);
        assert_eq!(attached(3, &rects), vec![3]);
    }

    #[test]
    fn a_window_dropped_near_another_lands_flush() {
        let eq = Rect::new(500.0, 500.0, 275.0, 116.0);
        // Dragged to 6pt right of and 4pt below directly-under-main.
        let (dx, dy) = snap(&[eq], (-394.0, -280.0), &[MAIN], &[]);
        assert_eq!(eq.moved(dx, dy), Rect::new(100.0, 216.0, 275.0, 116.0));
    }

    #[test]
    fn edges_out_of_reach_do_not_pull() {
        let eq = Rect::new(500.0, 500.0, 275.0, 116.0);
        assert_eq!(snap(&[eq], (-30.0, 0.0), &[MAIN], &[]), (-30.0, 0.0));
        // Horizontally within reach of main's right edge, but far below it.
        let far = Rect::new(380.0, 700.0, 275.0, 116.0);
        assert_eq!(snap(&[far], (0.0, 0.0), &[MAIN], &[]), (0.0, 0.0));
    }

    #[test]
    fn screen_edges_attract_from_inside() {
        let (dx, dy) = snap(&[MAIN], (-93.0, -70.0), &[], &[SCREEN]);
        assert_eq!(MAIN.moved(dx, dy), Rect::new(0.0, 25.0, 275.0, 116.0));
    }

    #[test]
    fn a_group_snaps_by_whichever_member_is_nearest() {
        let eq = Rect::new(100.0, 216.0, 275.0, 116.0);
        // The group's EQ comes within 3pt of the screen bottom.
        let (_, dy) = snap(&[MAIN, eq], (0.0, 565.0), &[], &[SCREEN]);
        assert_eq!(eq.bottom() + dy, SCREEN.bottom());
    }

    #[test]
    fn windows_below_follow_a_height_change() {
        let rects = [
            MAIN,
            Rect::new(100.0, 216.0, 275.0, 116.0), // EQ
            Rect::new(100.0, 332.0, 275.0, 232.0), // playlist under the EQ
            Rect::new(375.0, 100.0, 275.0, 116.0), // beside main: unaffected
        ];
        // Main rolls up to its 14pt strip.
        let moves = follow_resize(0, Rect::new(100.0, 100.0, 275.0, 14.0), &rects);
        assert_eq!(moves, vec![(1, 0.0, -102.0), (2, 0.0, -102.0)]);
    }

    #[test]
    fn windows_beside_follow_a_width_change() {
        let rects = [MAIN, Rect::new(375.0, 100.0, 275.0, 232.0)];
        let moves = follow_resize(0, Rect::new(100.0, 100.0, 550.0, 232.0), &rects);
        assert_eq!(moves, vec![(1, 275.0, 0.0)]);
    }
}
