//! The check that the compositor redrew nothing the paint walk did not visit.
//!
//! Every pixel a compositor frame redraws because a chunk was recorded, removed or reordered lies
//! where the walk was gated on, because the walk only records what it visits. The compositor also
//! redraws for reasons the walk never sees, so a frame is checked only when it has none of them:
//! no property, paint or offset write in its commit, no shift, a frame before it that retired its
//! damage, and draw damage short of the whole output.

use compositor_types::{CommitCounters, Frame};
use zgui_bits::DamageSet;
use zgui_geom::{Device, Point, Rect, Size};

/// Panics when the draw damage of `frame` reaches a pixel outside `painted`.
///
/// `commit` is what the frame's own commit did, and `retired` whether the frame before it retired
/// its damage. A frame those make unfit for the check passes.
pub(crate) fn check(frame: &Frame, commit: &CommitCounters, retired: bool, painted: &DamageSet) {
    let wrote = commit.property_writes > 0 || commit.paint_writes > 0 || commit.offsets_written > 0;
    let draw = &frame.damage.draw;
    if wrote || !retired || !frame.shifts.is_empty() || draw.is_full() || painted.is_full() {
        return;
    }
    for drawn in draw.rects() {
        let drawn = Rect::new(
            Point::new(drawn.origin.x, drawn.origin.y),
            Size::new(drawn.size.width, drawn.size.height),
        );
        if let Some(outside) = uncovered(drawn, painted.rects()) {
            panic!(
                "the compositor redrew {outside:?}, which the paint walk never visited: \
                 drawn {:?}, painted {:?}",
                draw.rects(),
                painted.rects()
            );
        }
    }
}

/// Returns a part of `rect` that no rectangle of `cover` reaches, or `None` when they cover it.
fn uncovered(rect: Rect<i32, Device>, cover: &[Rect<i32, Device>]) -> Option<Rect<i32, Device>> {
    let mut open = vec![rect];
    for piece in cover {
        open = open
            .into_iter()
            .flat_map(|held| without(held, *piece))
            .collect();
        if open.is_empty() {
            return None;
        }
    }
    open.into_iter().next()
}

/// Returns `rect` without `hole`, as at most four rectangles.
fn without(rect: Rect<i32, Device>, hole: Rect<i32, Device>) -> Vec<Rect<i32, Device>> {
    let Some(cut) = rect.intersection(hole) else {
        return vec![rect];
    };
    let (left, top) = (rect.origin.x, rect.origin.y);
    let (right, bottom) = (left + rect.size.width, top + rect.size.height);
    let (cut_left, cut_top) = (cut.origin.x, cut.origin.y);
    let (cut_right, cut_bottom) = (cut_left + cut.size.width, cut_top + cut.size.height);
    let piece = |x0: i32, y0: i32, x1: i32, y1: i32| {
        (x1 > x0 && y1 > y0).then(|| Rect::new(Point::new(x0, y0), Size::new(x1 - x0, y1 - y0)))
    };
    [
        piece(left, top, right, cut_top),
        piece(left, cut_bottom, right, bottom),
        piece(left, cut_top, cut_left, cut_bottom),
        piece(cut_right, cut_top, right, cut_bottom),
    ]
    .into_iter()
    .flatten()
    .collect()
}
