//! Unit and property tests for [`DamageSet`](super::DamageSet).

use std::collections::HashSet;

use proptest::prelude::*;
use zgui_geom::{Device, Point, Rect, Size};

use super::DamageSet;

/// A rectangle in whole device pixels.
fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect<i32, Device> {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

/// Every pixel a rectangle covers.
fn pixels(rect: Rect<i32, Device>) -> impl Iterator<Item = (i32, i32)> {
    (rect.top()..rect.bottom()).flat_map(move |y| (rect.left()..rect.right()).map(move |x| (x, y)))
}

/// Every pixel a set covers.
fn covered<const N: usize>(set: &DamageSet<N>) -> HashSet<(i32, i32)> {
    set.rects().iter().copied().flat_map(pixels).collect()
}

/// Asserts the one invariant the whole type exists to keep.
fn assert_disjoint<const N: usize>(set: &DamageSet<N>) {
    let rects = set.rects();
    assert!(rects.len() <= N, "the set exceeded its own capacity");
    for (index, left) in rects.iter().enumerate() {
        for right in &rects[index + 1..] {
            assert!(
                !left.intersects(*right),
                "{left:?} and {right:?} overlap, so their shared pixels would be redrawn twice"
            );
        }
    }
}

/// A small, deterministic generator, so a ten-thousand-step run is reproducible and cheap.
struct Xorshift(u64);

impl Xorshift {
    /// The next value in the sequence.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A value below `limit`.
    fn below(&mut self, limit: i32) -> i32 {
        (self.next() % limit as u64) as i32
    }
}

/// Rectangles small enough for a pixel-set oracle to stay cheap.
fn any_rect() -> impl Strategy<Value = Rect<i32, Device>> {
    (0i32..48, 0i32..48, 0i32..12, 0i32..12)
        .prop_map(|(x, y, width, height)| rect(x, y, width, height))
}

proptest! {
    /// The set never covers less than what it was given, and never holds two rectangles that
    /// overlap.
    #[test]
    fn the_set_stays_disjoint_and_covers_everything_absorbed(
        inputs in prop::collection::vec(any_rect(), 0..40),
    ) {
        let mut set = DamageSet::<4>::new();
        let mut expected: HashSet<(i32, i32)> = HashSet::new();
        for input in inputs {
            set.absorb(input);
            expected.extend(pixels(input));
            assert_disjoint(&set);
        }
        prop_assert!(expected.is_subset(&covered(&set)));
        prop_assert!(set.area().expect("not full") >= expected.len() as i64);
    }

    /// After absorbing a rectangle, one single rectangle of the set contains all of it — which is
    /// what a plain push cannot promise and what the read-extent expansion depends on.
    #[test]
    fn absorbing_leaves_one_rectangle_containing_the_whole_input(
        inputs in prop::collection::vec(any_rect(), 1..24),
    ) {
        let mut set = DamageSet::<4>::new();
        for input in inputs {
            set.absorb(input);
            if input.is_empty() {
                continue;
            }
            prop_assert!(
                set.rects().iter().any(|held| held.contains_rect(input)),
                "no single rectangle of {:?} contains {input:?}",
                set
            );
        }
    }

    /// Everything absorbed is reported as intersecting afterwards.
    #[test]
    fn what_was_absorbed_intersects(inputs in prop::collection::vec(any_rect(), 1..24)) {
        let mut set = DamageSet::<4>::new();
        for input in &inputs {
            set.absorb(*input);
        }
        for input in &inputs {
            prop_assert_eq!(set.intersects(*input), !input.is_empty());
        }
    }

    /// Absorbing at capacities either side of the default keeps the same promises.
    #[test]
    fn the_invariant_holds_at_other_capacities(inputs in prop::collection::vec(any_rect(), 0..24)) {
        let mut one = DamageSet::<1>::new();
        let mut eight = DamageSet::<8>::new();
        for input in inputs {
            one.absorb(input);
            eight.absorb(input);
            assert_disjoint(&one);
            assert_disjoint(&eight);
        }
        prop_assert!(one.len() <= 1);
    }
}

#[test]
fn ten_thousand_random_merges_keep_the_set_disjoint() {
    let mut generator = Xorshift(0x5eed_1234_9abc_def0);
    let mut set = DamageSet::<4>::new();
    let mut expected: HashSet<(i32, i32)> = HashSet::new();

    for _ in 0..10_000 {
        let candidate = rect(
            generator.below(400),
            generator.below(400),
            generator.below(24),
            generator.below(24),
        );
        set.absorb(candidate);
        expected.extend(pixels(candidate));
        assert_disjoint(&set);
    }

    let final_cover = covered(&set);
    assert!(
        expected.is_subset(&final_cover),
        "a pixel that was damaged is no longer in the set"
    );
    assert!(set.area().expect("not full") >= expected.len() as i64);
}

#[test]
fn a_capacity_merge_is_transitively_closed() {
    // Four spread-out rectangles fill the set, then a fifth forces a merge whose union swallows a
    // rectangle that touched neither of the merged pair.
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(0, 0, 10, 10));
    set.absorb(rect(100, 0, 10, 10));
    set.absorb(rect(50, 0, 10, 10));
    set.absorb(rect(0, 200, 10, 10));
    assert_eq!(set.len(), 4);

    set.absorb(rect(0, 400, 10, 10));
    assert_disjoint(&set);
    assert!(set.len() <= 4);
}

#[test]
fn an_empty_rectangle_damages_nothing() {
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(4, 4, 0, 8));
    assert!(set.is_empty());
    assert_eq!(set.area(), Some(0));
    assert!(!set.intersects(rect(0, 0, 100, 100)));
}

#[test]
fn a_full_set_swallows_everything_and_reports_no_rectangles() {
    let mut set = DamageSet::<4>::full();
    assert!(set.is_full());
    assert!(!set.is_empty());
    assert_eq!(set.rects(), &[]);
    assert_eq!(set.area(), None);
    assert_eq!(set.bounds(), None);

    set.absorb(rect(0, 0, 10, 10));
    assert_eq!(set.rects(), &[]);
    assert!(set.intersects(rect(900, 900, 1, 1)));
    assert!(!set.intersects(rect(0, 0, 0, 0)));
}

#[test]
fn going_full_discards_the_rectangles_it_supersedes() {
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(0, 0, 10, 10));
    set.set_full();
    assert_eq!(set.rects(), &[]);
    set.clear();
    assert!(set.is_empty());
    assert!(!set.is_full());
}

#[test]
fn absorbing_a_set_takes_its_rectangles_and_its_fullness() {
    let mut left = DamageSet::<4>::new();
    left.absorb(rect(0, 0, 10, 10));
    let mut right = DamageSet::<4>::new();
    right.absorb(rect(100, 100, 10, 10));

    left.absorb_set(&right);
    assert_eq!(left.len(), 2);

    left.absorb_set(&DamageSet::<4>::full());
    assert!(left.is_full());
}

#[test]
fn bridging_two_rectangles_collapses_all_three() {
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(0, 0, 10, 10));
    set.absorb(rect(20, 0, 10, 10));
    assert_eq!(set.len(), 2);
    set.absorb(rect(5, 0, 20, 10));
    assert_eq!(set.rects(), &[rect(0, 0, 30, 10)]);
}

#[test]
fn the_capacity_merge_picks_the_pair_that_wastes_least() {
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(0, 0, 10, 10));
    set.absorb(rect(11, 0, 10, 10));
    set.absorb(rect(0, 500, 10, 10));
    set.absorb(rect(500, 0, 10, 10));
    set.absorb(rect(500, 500, 10, 10));

    assert_disjoint(&set);
    // The two neighbours at the origin are the cheapest pair to fuse, so they are the pair that
    // fused, and the three distant rectangles are untouched.
    assert!(set.rects().contains(&rect(0, 0, 21, 10)));
    assert!(set.rects().contains(&rect(0, 500, 10, 10)));
    assert!(set.rects().contains(&rect(500, 0, 10, 10)));
    assert!(set.rects().contains(&rect(500, 500, 10, 10)));
}

#[test]
fn bounds_cover_every_rectangle() {
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(100, 100, 10, 10));
    set.absorb(rect(0, 0, 10, 10));
    assert_eq!(set.bounds(), Some(rect(0, 0, 110, 110)));
}

#[test]
fn a_frame_starts_from_the_environment_override() {
    // The override is off in the test process, so a frame starts empty.
    assert_eq!(
        DamageSet::<4>::for_frame().is_full(),
        super::full_damage_forced()
    );
}

#[test]
fn clipping_cuts_what_hangs_off_the_surface_and_drops_what_is_wholly_outside() {
    let surface = rect(0, 0, 100, 100);
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(-40, -40, 80, 80));
    set.absorb(rect(200, 200, 50, 50));
    set.clip_to(surface);
    assert_eq!(set.rects(), &[rect(0, 0, 40, 40)]);
    assert_disjoint(&set);
}

#[test]
fn clipping_leaves_a_full_set_alone() {
    // A full set already means the surface, so cutting it to the surface would only be a way of
    // turning "everything" into a rectangle that happens to be everything.
    let mut set = DamageSet::<4>::full();
    set.clip_to(rect(0, 0, 100, 100));
    assert!(set.is_full());
    assert!(set.rects().is_empty());
}

#[test]
fn absorbing_a_rectangle_already_covered_leaves_the_set_exactly_as_it_was() {
    // The case a scroll produces once per moved piece: a scrollport damaged whole, then every row
    // inside it offered one at a time. Each of those has to cost a containment test and not the
    // merge loop, and — because the set compares by the order it holds its rectangles in — has to
    // leave the set the identical value rather than an equivalent one.
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(0, 0, 100, 100));
    set.absorb(rect(500, 500, 10, 10));
    let before = set;
    set.absorb(rect(20, 20, 30, 30));
    set.absorb(rect(0, 0, 100, 100));
    set.absorb(rect(505, 505, 1, 1));
    assert_eq!(set, before);
}

#[test]
fn a_rectangle_spanning_two_held_ones_is_still_merged_in() {
    // Covered by the set's *area* and by neither of its rectangles. Answering "already contained"
    // over the union would leave two rectangles that both meet it, and the set promises none do.
    let mut set = DamageSet::<4>::new();
    set.absorb(rect(0, 0, 10, 10));
    set.absorb(rect(20, 0, 10, 10));
    assert!(!set.contains(rect(0, 0, 30, 10)));
    set.absorb(rect(0, 0, 30, 10));
    assert_eq!(set.rects(), &[rect(0, 0, 30, 10)]);
    assert_disjoint(&set);
}

#[test]
fn a_full_set_contains_everything_and_an_empty_one_contains_nothing_it_has_not_been_given() {
    assert!(DamageSet::<4>::full().contains(rect(0, 0, 1, 1)));
    assert!(!DamageSet::<4>::new().contains(rect(0, 0, 1, 1)));
}
