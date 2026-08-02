//! Draw order, against the definition it is an optimisation of.

use proptest::prelude::*;
use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::id::DrawOrder;
use crate::order::BoundsTree;

/// A device rectangle.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(width), DevicePx(height)),
    )
}

/// The definition the tree is an optimisation of: compare against everything inserted so far.
///
/// Quadratic and obviously correct, which is the point — the tree is neither.
#[derive(Default)]
struct Oracle {
    /// Every rectangle inserted, as `[left, top, right, bottom]`, with its order.
    inserted: Vec<([f32; 4], DrawOrder)>,
    /// The lowest order a subsequent insert may take.
    order_floor: DrawOrder,
}

impl Oracle {
    /// The order the definition assigns to `bounds`.
    fn insert(&mut self, bounds: Rect<DevicePx, Device>) -> DrawOrder {
        let edges = edges_of(bounds);
        let mut highest = 0;
        for (other, order) in &self.inserted {
            if overlaps(edges, *other) {
                highest = highest.max(*order);
            }
        }
        let order = (highest + 1).max(self.order_floor);
        self.inserted.push((edges, order));
        order
    }

    /// The order the definition assigns when overlap is ignored.
    fn insert_above_all(&mut self, bounds: Rect<DevicePx, Device>) -> DrawOrder {
        let order = self
            .inserted
            .iter()
            .map(|(_, order)| *order)
            .max()
            .unwrap_or(0)
            + 1;
        self.inserted.push((edges_of(bounds), order));
        order
    }

    /// Raises the floor for everything inserted afterwards.
    fn set_order_floor(&mut self, floor: DrawOrder) {
        self.order_floor = self.order_floor.max(floor);
    }
}

/// A rectangle as `[left, top, right, bottom]`.
fn edges_of(bounds: Rect<DevicePx, Device>) -> [f32; 4] {
    [
        bounds.origin.x.0,
        bounds.origin.y.0,
        bounds.origin.x.0 + bounds.size.width.0,
        bounds.origin.y.0 + bounds.size.height.0,
    ]
}

/// Whether two rectangles share any area, matching the geometry crate's half-open rule.
fn overlaps(left: [f32; 4], right: [f32; 4]) -> bool {
    left[0].max(right[0]) < left[2].min(right[2]) && left[1].max(right[1]) < left[3].min(right[3])
}

/// A deterministic generator, so a failing run is reproducible from its seed alone.
struct Xorshift(u64);

impl Xorshift {
    /// The next value.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A value in `0.0 .. limit`.
    fn float(&mut self, limit: f32) -> f32 {
        (self.next() % 100_000) as f32 / 100_000.0 * limit
    }
}

#[test]
fn overlapping_content_steps_and_disjoint_content_does_not() {
    let mut tree = BoundsTree::new();
    assert_eq!(tree.insert(rect(0.0, 0.0, 10.0, 10.0)), 1);
    assert_eq!(tree.insert(rect(5.0, 5.0, 10.0, 10.0)), 2);
    assert_eq!(tree.insert(rect(8.0, 8.0, 10.0, 10.0)), 3);
    assert_eq!(tree.insert(rect(100.0, 100.0, 10.0, 10.0)), 1);
    assert_eq!(tree.max_order(), 3);
}

#[test]
fn touching_edges_do_not_count_as_overlapping() {
    let mut tree = BoundsTree::new();
    assert_eq!(tree.insert(rect(0.0, 0.0, 10.0, 10.0)), 1);
    assert_eq!(tree.insert(rect(10.0, 0.0, 10.0, 10.0)), 1);
}

#[test]
fn a_marker_sorts_above_everything_whether_it_overlaps_or_not() {
    let mut tree = BoundsTree::new();
    tree.insert(rect(0.0, 0.0, 10.0, 10.0));
    tree.insert(rect(5.0, 5.0, 10.0, 10.0));
    assert_eq!(tree.insert_above_all(rect(500.0, 500.0, 1.0, 1.0)), 3);
}

#[test]
fn an_order_floor_stops_later_content_sorting_inside_a_closed_group() {
    let mut tree = BoundsTree::new();
    tree.insert(rect(0.0, 0.0, 10.0, 10.0));
    let group_end = tree.insert_above_all(rect(0.0, 0.0, 10.0, 10.0));
    tree.set_order_floor(group_end);

    // Nowhere near the group, so an unfloored tree would hand it order 1 and it would sort under
    // content painted before it.
    assert_eq!(tree.insert(rect(900.0, 900.0, 10.0, 10.0)), group_end);
    assert_eq!(tree.order_floor(), group_end);
}

#[test]
fn clearing_returns_the_tree_to_its_initial_state() {
    let mut tree = BoundsTree::new();
    tree.insert(rect(0.0, 0.0, 10.0, 10.0));
    tree.set_order_floor(9);
    tree.clear();

    assert!(tree.is_empty());
    assert_eq!(tree.max_order(), 0);
    assert_eq!(tree.order_floor(), 0);
    assert_eq!(tree.insert(rect(0.0, 0.0, 10.0, 10.0)), 1);
}

/// The acceptance property: ten thousand random rectangles, and the tree's answer is the
/// definition's answer every single time.
#[test]
fn ten_thousand_random_rectangles_match_the_quadratic_definition() {
    let mut generator = Xorshift(0x5eed_1337_c0ff_ee01);
    let mut tree = BoundsTree::new();
    let mut oracle = Oracle::default();

    for index in 0..10_000 {
        let bounds = rect(
            generator.float(1920.0),
            generator.float(1080.0),
            1.0 + generator.float(240.0),
            1.0 + generator.float(160.0),
        );
        let expected = oracle.insert(bounds);
        let actual = tree.insert(bounds);
        assert_eq!(
            actual, expected,
            "rectangle {index} at {bounds:?} was ordered {actual}, not {expected}"
        );
    }
    assert_eq!(tree.len(), 10_000);
}

proptest! {
    /// The same equivalence under the barriers, which the sweep above does not exercise.
    #[test]
    fn the_tree_matches_the_definition_under_both_barriers(
        operations in prop::collection::vec(
            (0u8..8, 0f32..400.0, 0f32..400.0, 1f32..120.0, 1f32..120.0),
            1..200,
        )
    ) {
        let mut tree = BoundsTree::new();
        let mut oracle = Oracle::default();

        for (operation, x, y, width, height) in operations {
            let bounds = rect(x, y, width, height);
            match operation {
                0 => {
                    let expected = oracle.insert_above_all(bounds);
                    prop_assert_eq!(tree.insert_above_all(bounds), expected);
                }
                1 => {
                    let floor = tree.max_order();
                    tree.set_order_floor(floor);
                    oracle.set_order_floor(floor);
                }
                _ => {
                    let expected = oracle.insert(bounds);
                    prop_assert_eq!(tree.insert(bounds), expected);
                }
            }
        }
    }

    /// The invariant the rest of the crate consumes, stated directly rather than inferred.
    #[test]
    fn two_rectangles_at_equal_order_never_overlap(
        rectangles in prop::collection::vec(
            (0f32..300.0, 0f32..300.0, 1f32..90.0, 1f32..90.0),
            1..120,
        )
    ) {
        let mut tree = BoundsTree::new();
        let mut assigned: Vec<([f32; 4], DrawOrder)> = Vec::new();
        for (x, y, width, height) in rectangles {
            let bounds = rect(x, y, width, height);
            assigned.push((edges_of(bounds), tree.insert(bounds)));
        }

        for (index, (left, left_order)) in assigned.iter().enumerate() {
            for (right, right_order) in &assigned[index + 1..] {
                if left_order == right_order {
                    prop_assert!(!overlaps(*left, *right));
                }
            }
        }
    }
}
