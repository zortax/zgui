//! The least-wasted-area choice made when a damage set runs out of room.

use zgui_geom::{Device, Rect};

/// The number of device pixels a rectangle covers, saturating rather than wrapping.
pub(super) fn area(rect: Rect<i32, Device>) -> i64 {
    if rect.is_empty() {
        return 0;
    }
    i64::from(rect.width()).saturating_mul(i64::from(rect.height()))
}

/// The pixels a merge of two disjoint rectangles would newly cover, which is the pixels it would
/// cause to be redrawn for nothing.
fn wasted(left: Rect<i32, Device>, right: Rect<i32, Device>) -> i64 {
    area(left.union(right))
        .saturating_sub(area(left))
        .saturating_sub(area(right))
}

/// Picks the two rectangles whose union wastes the fewest pixels, out of `existing` plus
/// `incoming`.
///
/// The returned indices are into `existing`, except that `existing.len()` stands for `incoming`.
/// They are ordered, so the larger can be removed first without disturbing the smaller.
///
/// # Panics
///
/// Panics if `existing` is empty, since a pair needs two rectangles to choose between.
pub(super) fn least_wasted_pair(
    existing: &[Rect<i32, Device>],
    incoming: Rect<i32, Device>,
) -> (usize, usize) {
    assert!(
        !existing.is_empty(),
        "a merge needs at least two rectangles to choose between"
    );
    let count = existing.len() + 1;
    let at = |index: usize| {
        if index == existing.len() {
            incoming
        } else {
            existing[index]
        }
    };

    let mut best = (0, 1);
    let mut best_waste = i64::MAX;
    for left in 0..count {
        for right in (left + 1)..count {
            let waste = wasted(at(left), at(right));
            if waste < best_waste {
                best_waste = waste;
                best = (left, right);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Device, Point, Rect, Size};

    use super::{area, least_wasted_pair, wasted};

    /// A rectangle in whole device pixels.
    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
        Rect::new(Point::new(x, y), Size::new(width, height))
    }

    #[test]
    fn an_empty_rectangle_has_no_area() {
        assert_eq!(area(rect(4, 4, 0, 10)), 0);
        assert_eq!(area(rect(4, 4, 10, -1)), 0);
    }

    #[test]
    fn merging_neighbours_wastes_less_than_merging_strangers() {
        let near = wasted(rect(0, 0, 10, 10), rect(10, 0, 10, 10));
        let far = wasted(rect(0, 0, 10, 10), rect(1000, 1000, 10, 10));
        assert_eq!(near, 0);
        assert!(far > near);
    }

    #[test]
    fn the_chosen_pair_is_the_closest_one() {
        let existing = [
            rect(0, 0, 10, 10),
            rect(500, 500, 10, 10),
            rect(10, 0, 10, 10),
        ];
        assert_eq!(least_wasted_pair(&existing, rect(900, 900, 4, 4)), (0, 2));
    }

    #[test]
    fn the_incoming_rectangle_is_a_candidate_like_any_other() {
        let existing = [rect(0, 0, 10, 10), rect(500, 500, 10, 10)];
        assert_eq!(least_wasted_pair(&existing, rect(500, 510, 10, 10)), (1, 2));
    }
}
