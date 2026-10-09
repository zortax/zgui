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

/// Whether two overlapping rectangles are better redrawn as their union than apart.
///
/// The union is taken when what it adds to the two is no more than the larger of them: two pieces
/// of one region, or strips of different widths along one edge, become one rectangle, and two thin
/// strips that only meet at a corner are kept apart.
pub(super) fn worth_merging(held: Rect<i32, Device>, incoming: Rect<i32, Device>) -> bool {
    let covered = area(held)
        .saturating_add(area(incoming))
        .saturating_sub(area(held.intersection(incoming).unwrap_or(Rect::ZERO)));
    let waste = area(held.union(incoming)).saturating_sub(covered);
    waste <= area(held).max(area(incoming))
}

/// The parts of `rect` outside `held`, as up to four disjoint rectangles: the rows above and below
/// `held` across the whole of `rect`, and the columns beside it between them.
pub(super) fn outside(rect: Rect<i32, Device>, held: Rect<i32, Device>) -> Vec<Rect<i32, Device>> {
    let span = |left: i32, top: i32, right: i32, bottom: i32| {
        Rect::from_corners(
            zgui_geom::Point::new(left, top),
            zgui_geom::Point::new(right.max(left), bottom.max(top)),
        )
    };
    let top = rect.top().max(held.top());
    let bottom = rect.bottom().min(held.bottom());
    [
        span(
            rect.left(),
            rect.top(),
            rect.right(),
            held.top().min(rect.bottom()),
        ),
        span(
            rect.left(),
            held.bottom().max(rect.top()),
            rect.right(),
            rect.bottom(),
        ),
        span(rect.left(), top, held.left().min(rect.right()), bottom),
        span(held.right().max(rect.left()), top, rect.right(), bottom),
    ]
    .into_iter()
    .filter(|piece| !piece.is_empty())
    .collect()
}

/// What is left of `rect` outside every one of `held`, as disjoint pieces, or `None` once it takes
/// more than `most` pieces to say.
pub(super) fn remainder(
    rect: Rect<i32, Device>,
    held: &[Rect<i32, Device>],
    most: usize,
) -> Option<Vec<Rect<i32, Device>>> {
    let mut pieces = vec![rect];
    for one in held {
        let mut next = Vec::with_capacity(pieces.len() + 3);
        for piece in pieces {
            if piece.intersects(*one) {
                next.extend(outside(piece, *one));
            } else {
                next.push(piece);
            }
        }
        if next.len() > most {
            return None;
        }
        pieces = next;
    }
    Some(pieces)
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

    use super::{area, least_wasted_pair, outside, wasted, worth_merging};

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

    #[test]
    fn strips_meeting_at_a_corner_are_kept_apart_and_overlapping_pieces_are_merged() {
        assert!(!worth_merging(rect(90, 0, 10, 100), rect(0, 90, 100, 10)));
        assert!(worth_merging(rect(0, 0, 10, 10), rect(5, 5, 10, 10)));
        assert!(worth_merging(rect(90, 0, 10, 100), rect(88, 0, 4, 100)));
    }

    #[test]
    fn what_lies_outside_a_held_rectangle_covers_the_rest_and_nothing_twice() {
        let rect_ = rect(0, 0, 100, 100);
        let held = rect(30, 40, 50, 20);
        let pieces = outside(rect_, held);
        assert_eq!(pieces.len(), 4);
        let covered: i64 = pieces.iter().map(|piece| area(*piece)).sum();
        assert_eq!(covered, area(rect_) - area(held));
        for (index, one) in pieces.iter().enumerate() {
            assert!(!one.intersects(held));
            for other in &pieces[index + 1..] {
                assert!(!one.intersects(*other));
            }
        }
    }
}
