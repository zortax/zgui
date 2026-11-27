//! The rectangle arithmetic of the containment check.

use zgui_geom::{Device, Point, Rect, Size};

/// Returns a part of `rect` that no rectangle of `cover` reaches, or `None` when they cover it.
pub(super) fn uncovered(
    rect: Rect<i32, Device>,
    cover: &[Rect<i32, Device>],
) -> Option<Rect<i32, Device>> {
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
