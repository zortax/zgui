//! The least-wasted-area choice made when a damage set runs out of room.
//!
//! The merge reads rectangles as their four edges. The geometry crate's own operations go through
//! the unit trait, whose `min` and `max` the optimiser does not inline across crates, and a capacity
//! merge runs a few hundred of them for each rectangle it places.

use zgui_geom::{Device, Point, Rect, Size};

/// A rectangle as its four edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Span {
    /// The left edge.
    left: i32,
    /// The top edge.
    top: i32,
    /// The right edge, excluded.
    right: i32,
    /// The bottom edge, excluded.
    bottom: i32,
}

impl Span {
    /// The edges of `rect`.
    #[inline]
    fn of(rect: Rect<i32, Device>) -> Self {
        Self {
            left: rect.left(),
            top: rect.top(),
            right: rect.right(),
            bottom: rect.bottom(),
        }
    }

    /// The rectangle with these edges.
    #[inline]
    fn rect(self) -> Rect<i32, Device> {
        Rect::new(
            Point::new(self.left, self.top),
            Size::new(self.right - self.left, self.bottom - self.top),
        )
    }

    /// Whether the span holds no pixel. Same rule as [`Rect::is_empty`].
    #[inline]
    fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    /// Whether the two spans share a pixel. Same rule as [`Rect::intersects`].
    ///
    /// Both axes are always tested, so the test compiles without a branch.
    #[inline]
    fn intersects(self, other: Self) -> bool {
        (self.right.min(other.right) > self.left.max(other.left))
            & (self.bottom.min(other.bottom) > self.top.max(other.top))
    }

    /// The smallest span that holds both. Same rule as [`Rect::union`]: an empty span is ignored.
    #[inline]
    fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        Self {
            left: self.left.min(other.left),
            top: self.top.min(other.top),
            right: self.right.max(other.right),
            bottom: self.bottom.max(other.bottom),
        }
    }

    /// The number of pixels the span covers, saturating. Same value as [`area`].
    #[inline]
    fn area(self) -> i64 {
        if self.is_empty() {
            return 0;
        }
        (i64::from(self.right) - i64::from(self.left))
            .saturating_mul(i64::from(self.bottom) - i64::from(self.top))
    }

    /// The number of pixels the two spans share.
    #[inline]
    fn overlap_area(self, other: Self) -> i64 {
        if !self.intersects(other) {
            return 0;
        }
        Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        }
        .area()
    }
}

/// Whether the two rectangles share a pixel. Same answer as [`Rect::intersects`].
#[inline]
pub(super) fn meets(left: Rect<i32, Device>, right: Rect<i32, Device>) -> bool {
    Span::of(left).intersects(Span::of(right))
}

/// The smallest rectangle that holds both. Same answer as [`Rect::union`].
#[inline]
pub(super) fn join(left: Rect<i32, Device>, right: Rect<i32, Device>) -> Rect<i32, Device> {
    Span::of(left).union(Span::of(right)).rect()
}

/// The number of device pixels a rectangle covers, saturating rather than wrapping.
pub(super) fn area(rect: Rect<i32, Device>) -> i64 {
    if rect.is_empty() {
        return 0;
    }
    i64::from(rect.width()).saturating_mul(i64::from(rect.height()))
}

/// The pixels a merge of two disjoint rectangles would newly cover, which is the pixels it would
/// cause to be redrawn for nothing.
#[cfg(test)]
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
    let (held, incoming) = (Span::of(held), Span::of(incoming));
    let covered = held
        .area()
        .saturating_add(incoming.area())
        .saturating_sub(held.overlap_area(incoming));
    let waste = held.union(incoming).area().saturating_sub(covered);
    waste <= held.area().max(incoming.area())
}

/// The parts of `rect` outside `held`, as up to four disjoint rectangles: the rows above and below
/// `held` across the whole of `rect`, and the columns beside it between them.
pub(super) fn outside(
    rect: Rect<i32, Device>,
    held: Rect<i32, Device>,
) -> impl Iterator<Item = Rect<i32, Device>> {
    let (rect, held) = (Span::of(rect), Span::of(held));
    let span = |left: i32, top: i32, right: i32, bottom: i32| Span {
        left,
        top,
        right: right.max(left),
        bottom: bottom.max(top),
    };
    let top = rect.top.max(held.top);
    let bottom = rect.bottom.min(held.bottom);
    [
        span(rect.left, rect.top, rect.right, held.top.min(rect.bottom)),
        span(
            rect.left,
            held.bottom.max(rect.top),
            rect.right,
            rect.bottom,
        ),
        span(rect.left, top, held.left.min(rect.right), bottom),
        span(held.right.max(rect.left), top, rect.right, bottom),
    ]
    .into_iter()
    .filter(|piece| !piece.is_empty())
    .map(Span::rect)
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
            if meets(piece, *one) {
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

/// The rectangles a capacity merge chooses between: a full set plus the one it has no room for.
struct Candidates<const N: usize> {
    /// The held rectangles' edges, in the set's order.
    spans: [Span; N],
    /// Their areas.
    areas: [i64; N],
    /// The incoming rectangle's edges, at index `N`.
    incoming: Span,
    /// Its area.
    incoming_area: i64,
}

impl<const N: usize> Candidates<N> {
    /// How many rectangles there are to choose between.
    const COUNT: usize = N + 1;

    /// The edges of the rectangle at `index`.
    #[inline]
    fn at(&self, index: usize) -> Span {
        if index == N {
            self.incoming
        } else {
            self.spans[index]
        }
    }

    /// The area of the rectangle at `index`.
    #[inline]
    fn area(&self, index: usize) -> i64 {
        if index == N {
            self.incoming_area
        } else {
            self.areas[index]
        }
    }

    /// One bit for each rectangle that shares a pixel with `union`, at the rectangle's index.
    ///
    /// Each test is made without a branch, so the loop unrolls into straight compares.
    #[inline]
    fn hits(&self, union: Span) -> u64 {
        let mut mask = u64::from(self.incoming.intersects(union)) << N;
        for (index, span) in self.spans.iter().enumerate() {
            mask |= u64::from(span.intersects(union)) << index;
        }
        mask
    }

    /// The pixels merging the rectangles at `left` and `right` would redraw for nothing, counting
    /// every other rectangle the union grows over.
    fn closure_waste(&self, left: usize, right: usize) -> i64 {
        let pair = (1_u64 << left) | (1_u64 << right);
        let mut union = self.at(left).union(self.at(right));
        let mut fresh = self.hits(union) & !pair;
        // The common case: a union that meets nothing else.
        if fresh == 0 {
            return union
                .area()
                .saturating_sub(self.area(left))
                .saturating_sub(self.area(right));
        }
        // The union of the taken rectangles is their bounding box, so the closure is the same in
        // every visiting order.
        let mut taken = pair;
        while fresh != 0 {
            taken |= fresh;
            let mut rest = fresh;
            while rest != 0 {
                union = union.union(self.at(rest.trailing_zeros() as usize));
                rest &= rest - 1;
            }
            fresh = self.hits(union) & !taken;
        }
        let covered = (0..Self::COUNT)
            .filter(|index| taken & (1 << index) != 0)
            .fold(0_i64, |sum, index| sum.saturating_add(self.area(index)));
        union.area().saturating_sub(covered)
    }
}

/// Picks the two rectangles whose union wastes the fewest pixels, out of `existing` plus
/// `incoming`.
///
/// What a union costs is everything it pulls in: a union that meets a third rectangle is merged
/// with that one too, so the pixels wasted are counted over the whole closure. Two thin pieces
/// either side of a column merge into a strip across it, and the column with it, which is the
/// waste a pair-wise count would miss.
///
/// `existing` and `incoming` must be pairwise disjoint. Ties go to the first pair in order. The
/// search stops at a pair that wastes nothing, because no merge of disjoint rectangles wastes less.
///
/// The returned indices are into `existing`, except that `N` stands for `incoming`. They are
/// ordered, so the larger can be removed first without disturbing the smaller.
///
/// `N` must be at least 1 and less than 64. The compiler checks this.
pub(super) fn least_wasted_pair<const N: usize>(
    existing: &[Rect<i32, Device>; N],
    incoming: Rect<i32, Device>,
) -> (usize, usize) {
    const {
        assert!(
            N >= 1 && N < 64,
            "a merge needs two rectangles to choose between, and the closure marks what it takes \
             in one 64-bit mask"
        );
    };
    let spans = existing.map(Span::of);
    let incoming = Span::of(incoming);
    let candidates = Candidates {
        areas: spans.map(Span::area),
        spans,
        incoming,
        incoming_area: incoming.area(),
    };

    let mut best = (0, 1);
    let mut best_waste = i64::MAX;
    for left in 0..Candidates::<N>::COUNT {
        for right in (left + 1)..Candidates::<N>::COUNT {
            let waste = candidates.closure_waste(left, right);
            if waste < best_waste {
                best_waste = waste;
                best = (left, right);
                if waste <= 0 {
                    debug_assert!(
                        waste == 0,
                        "the candidates overlap, so the early stop is wrong"
                    );
                    return best;
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use zgui_geom::{Device, Point, Rect, Size};

    use super::{Span, area, join, least_wasted_pair, meets, outside, wasted, worth_merging};
    use crate::damage_set::DamageSet;
    use crate::damage_set::tests::Xorshift;

    #[test]
    fn a_merge_that_would_swallow_a_third_rectangle_is_counted_with_it() {
        // Two slivers either side of a column: merged pair-wise they waste a pixel, and merged in
        // the set they take the column and everything around it with them.
        let existing = [
            rect(1985, 74, 15, 1049),
            rect(319, 1123, 2241, 18),
            rect(319, 1122, 1666, 1),
            rect(2000, 1122, 560, 1),
        ];
        let (left, right) = least_wasted_pair(&existing, rect(2545, 116, 15, 1023));
        assert_ne!(
            (left, right),
            (2, 3),
            "the slivers' union crosses the column"
        );
    }

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
        let pieces: Vec<_> = outside(rect_, held).collect();
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

    /// The pair search as it was before it read edges: every pair scored through the geometry
    /// crate's operations, with no early stop.
    fn reference_pair(
        existing: &[Rect<i32, Device>],
        incoming: Rect<i32, Device>,
    ) -> (usize, usize) {
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
                let mut union = at(left).union(at(right));
                let mut taken = vec![false; count];
                taken[left] = true;
                taken[right] = true;
                let mut grew = true;
                while grew {
                    grew = false;
                    for (index, held) in taken.iter_mut().enumerate() {
                        if !*held && at(index).intersects(union) {
                            union = union.union(at(index));
                            *held = true;
                            grew = true;
                        }
                    }
                }
                let covered = taken
                    .iter()
                    .enumerate()
                    .filter(|(_, held)| **held)
                    .fold(0_i64, |sum, (index, _)| sum.saturating_add(area(at(index))));
                let waste = area(union).saturating_sub(covered);
                if waste < best_waste {
                    best_waste = waste;
                    best = (left, right);
                }
            }
        }
        best
    }

    /// Any rectangle, empty and inverted ones included.
    fn any_rect() -> impl Strategy<Value = Rect<i32, Device>> {
        (-64i32..64, -64i32..64, -8i32..48, -8i32..48)
            .prop_map(|(x, y, width, height)| rect(x, y, width, height))
    }

    proptest! {
        /// The edge operations give the geometry crate's answers.
        #[test]
        fn edges_answer_as_the_geometry_crate_does(left in any_rect(), right in any_rect()) {
            prop_assert_eq!(meets(left, right), left.intersects(right));
            prop_assert_eq!(join(left, right), left.union(right));
            prop_assert_eq!(Span::of(left).area(), area(left));
        }

        /// A full set and a rectangle outside it pick the same pair as the search did before.
        #[test]
        fn the_pair_search_picks_what_the_reference_picks(
            inputs in prop::collection::vec(
                (0i32..400, 0i32..400, 1i32..60, 1i32..60)
                    .prop_map(|(x, y, width, height)| rect(x, y, width, height)),
                6..40,
            ),
        ) {
            let mut set = DamageSet::<6>::new();
            for input in inputs {
                if set.len() == 6 && !set.intersects(input) {
                    let existing: [Rect<i32, Device>; 6] =
                        set.rects().try_into().expect("a full set holds six");
                    prop_assert_eq!(
                        least_wasted_pair(&existing, input),
                        reference_pair(&existing, input)
                    );
                }
                set.absorb(input);
            }
        }
    }

    /// Absorbs `stream` and checks each capacity merge an absorb starts against the reference.
    /// Returns how many merges it checked.
    fn check_stream<const N: usize>(stream: impl IntoIterator<Item = Rect<i32, Device>>) -> usize {
        let mut set = DamageSet::<N>::new();
        let mut checked = 0;
        for input in stream {
            if set.len() == N && !input.is_empty() && !set.intersects(input) {
                let existing: [Rect<i32, Device>; N] =
                    set.rects().try_into().expect("a full set holds N");
                assert_eq!(
                    least_wasted_pair(&existing, input),
                    reference_pair(&existing, input),
                    "{existing:?} + {input:?}"
                );
                checked += 1;
            }
            set.absorb(input);
        }
        checked
    }

    #[test]
    fn the_search_stops_at_the_first_pair_that_wastes_nothing() {
        // Two pairs of neighbours that touch along an edge: both merge for free, and the first in
        // order is the one taken.
        let existing = [
            rect(0, 0, 10, 10),
            rect(10, 0, 10, 10),
            rect(100, 0, 10, 10),
            rect(110, 0, 10, 10),
        ];
        let incoming = rect(500, 500, 4, 4);
        assert_eq!(least_wasted_pair(&existing, incoming), (0, 1));
        assert_eq!(reference_pair(&existing, incoming), (0, 1));
    }

    #[test]
    fn long_streams_merge_as_the_reference_does() {
        // A grid of loaders absorbed one by one, which fills the set and merges on nearly every
        // rectangle after that.
        let grid = (0..500).map(|index| rect(15 + index % 24 * 48, 15 + index / 24 * 22, 42, 16));
        assert!(check_stream::<6>(grid) > 400);

        // Scattered rectangles of mixed sizes. A new set every 40 keeps it from growing whole.
        let mut generator = Xorshift(0x0dd_ba11_cafe_f00d);
        let mut checked = 0;
        for _ in 0..200 {
            let stream: Vec<_> = (0..40)
                .map(|_| {
                    rect(
                        generator.below(1900),
                        generator.below(1000),
                        1 + generator.below(120),
                        1 + generator.below(40),
                    )
                })
                .collect();
            checked += check_stream::<6>(stream.iter().copied());
            checked += check_stream::<4>(stream);
        }
        assert!(checked > 1000, "only {checked} merges were checked");
    }
}
