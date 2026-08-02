//! Turning composed rectangles into damage, and the overlap tests the folds are built on.

use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

/// Whether two rectangles share any area, treating an empty one as sharing none.
pub(super) fn overlaps(one: Rect<DevicePx, Device>, other: Rect<DevicePx, Device>) -> bool {
    !one.is_empty() && !other.is_empty() && one.intersects(other)
}

/// Whether no two of the rectangles overlap.
///
/// Answered by sweeping a line down the page rather than by comparing every pair, because the
/// pieces this is asked about are one box's children and a document's largest box has as many
/// children as the list it holds has rows. Comparing every pair costs the square of that, on every
/// frame that enters the box — so a list twice as long costs four times as much to decide nothing
/// about, which is the shape of a cost that is invisible until the document is large.
///
/// The sweep is exact and not an estimate. Rectangles are ordered by their top edge; a candidate is
/// compared only against those still open at its own top edge, and one whose bottom edge has passed
/// cannot meet anything below it. Two rectangles overlap only if they overlap on *both* axes, so a
/// pair separated vertically is never compared at all.
pub(super) fn pairwise_disjoint(rects: &[Rect<DevicePx, Device>]) -> bool {
    // Below this the sweep's sort costs more than the comparisons it saves, and a box with a
    // handful of children is the overwhelming majority of boxes.
    const SWEEP_ABOVE: usize = 8;
    if rects.len() <= SWEEP_ABOVE {
        for (index, one) in rects.iter().enumerate() {
            if rects
                .iter()
                .skip(index + 1)
                .any(|other| overlaps(*one, *other))
            {
                return false;
            }
        }
        return true;
    }

    let mut order: Vec<usize> = (0..rects.len())
        .filter(|index| !rects[*index].is_empty())
        .collect();
    order.sort_unstable_by(|left, right| rects[*left].top().0.total_cmp(&rects[*right].top().0));
    // The rectangles whose bottom edge has not yet been passed, kept in the order they opened.
    let mut open: Vec<usize> = Vec::with_capacity(order.len());
    for index in order {
        let candidate = rects[index];
        let top = candidate.top().0;
        open.retain(|held| rects[*held].bottom().0 > top);
        for held in &open {
            if overlaps(candidate, rects[*held]) {
                return false;
            }
        }
        open.push(index);
    }
    true
}

/// Adds what a rectangle covers to the damage, ignoring one that covers nothing.
pub(super) fn absorb(damage: &mut DamageSet, rect: Rect<DevicePx, Device>) {
    if rect.is_empty() {
        return;
    }
    damage.absorb(pixels(rect));
}

/// The whole device pixels a rectangle touches.
///
/// Damage is measured in real pixels and a rectangle that covers part of one has to redraw all of
/// it, so the conversion is outwards on every side. Rounding to the nearest pixel instead would
/// leave a hairline of last frame's content along an edge.
pub fn pixels(rect: Rect<DevicePx, Device>) -> Rect<i32, Device> {
    if rect.is_empty() {
        return Rect::new(
            Point::new(
                rect.origin.x.0.floor() as i32,
                rect.origin.y.0.floor() as i32,
            ),
            Size::new(0, 0),
        );
    }
    let left = rect.left().0.floor() as i32;
    let top = rect.top().0.floor() as i32;
    let right = rect.right().0.ceil() as i32;
    let bottom = rect.bottom().0.ceil() as i32;
    Rect::new(Point::new(left, top), Size::new(right - left, bottom - top))
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};

    use super::{overlaps, pairwise_disjoint, pixels};

    #[test]
    fn a_rectangle_covering_part_of_a_pixel_damages_all_of_it() {
        let rect: Rect<DevicePx, Device> = Rect::new(
            Point::new(DevicePx(10.25), DevicePx(4.75)),
            Size::new(DevicePx(3.5), DevicePx(2.0)),
        );
        let pixels = pixels(rect);
        assert_eq!(pixels.origin, Point::new(10, 4));
        assert_eq!(pixels.size, Size::new(4, 3));
    }

    /// A rectangle at one place with one extent.
    fn at(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(x), DevicePx(y)),
            Size::new(DevicePx(width), DevicePx(height)),
        )
    }

    /// Every pair, compared directly. What the sweep has to agree with.
    fn quadratic(rects: &[Rect<DevicePx, Device>]) -> bool {
        for (index, one) in rects.iter().enumerate() {
            if rects
                .iter()
                .skip(index + 1)
                .any(|other| overlaps(*one, *other))
            {
                return false;
            }
        }
        true
    }

    #[test]
    fn the_sweep_answers_what_comparing_every_pair_answers() {
        // Long enough to take the sweep, and shaped like the cases it has to separate: a stacked
        // column that touches but never overlaps, one row grown into its neighbour, and a set that
        // overlaps horizontally while being separated vertically.
        let column: Vec<Rect<DevicePx, Device>> = (0..40)
            .map(|row| at(0.0, row as f32 * 10.0, 100.0, 10.0))
            .collect();
        assert!(quadratic(&column));
        assert!(pairwise_disjoint(&column));

        let mut grown = column.clone();
        grown[17] = at(0.0, 170.0, 100.0, 11.0);
        assert!(!quadratic(&grown));
        assert!(!pairwise_disjoint(&grown));

        let mut empties = column.clone();
        empties.push(at(50.0, 50.0, 0.0, 0.0));
        assert!(quadratic(&empties));
        assert!(pairwise_disjoint(&empties));

        // Every rectangle spanning the same vertical band, so the sweep never closes anything and
        // has to fall back on comparing them all.
        let band: Vec<Rect<DevicePx, Device>> = (0..20)
            .map(|at_| at(at_ as f32 * 10.0, 0.0, 10.0, 100.0))
            .collect();
        assert!(quadratic(&band));
        assert!(pairwise_disjoint(&band));
        let mut crowded = band.clone();
        crowded[3] = at(30.0, 0.0, 11.0, 100.0);
        assert!(!quadratic(&crowded));
        assert!(!pairwise_disjoint(&crowded));
    }

    #[test]
    fn touching_rectangles_are_disjoint_and_overlapping_ones_are_not() {
        let one: Rect<DevicePx, Device> = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(10.0), DevicePx(10.0)),
        );
        let touching = Rect::new(
            Point::new(DevicePx(10.0), DevicePx(0.0)),
            Size::new(DevicePx(10.0), DevicePx(10.0)),
        );
        let overlapping = Rect::new(
            Point::new(DevicePx(9.0), DevicePx(0.0)),
            Size::new(DevicePx(10.0), DevicePx(10.0)),
        );
        assert!(pairwise_disjoint(&[one, touching]));
        assert!(!pairwise_disjoint(&[one, overlapping]));
    }
}
