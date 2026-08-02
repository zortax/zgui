//! What the sweep finds, and what it refuses to answer.

use zgui_geom::{Device, DevicePx, Point, Rect, Size};

use crate::order::sweep::{DEFAULT_MAX, Ordered, overlaps};
use crate::prim::PrimitiveKind;

/// A ten-by-ten square at `x`, `y`.
fn at(x: f32, y: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x), DevicePx(y)),
        Size::new(DevicePx(10.0), DevicePx(10.0)),
    )
}

/// One primitive at `order` covering `ink`.
fn one(order: u32, ink: Rect<DevicePx, Device>) -> Ordered {
    Ordered {
        order,
        kind: PrimitiveKind::Quad,
        ink,
    }
}

#[test]
fn a_row_of_disjoint_boxes_at_one_order_is_what_the_assigner_is_for() {
    // The commonest document, and the case a pairwise check would spend fifty million comparisons
    // on: a page of non-overlapping boxes all end up at order one.
    let class: Vec<Ordered> = (0..500)
        .map(|column| one(1, at(column as f32 * 20.0, 0.0)))
        .collect();
    assert_eq!(
        overlaps(&class, DEFAULT_MAX).expect("under the cap"),
        vec![]
    );
}

#[test]
fn two_boxes_that_share_an_order_and_a_region_are_found() {
    let found = overlaps(&[one(3, at(0.0, 0.0)), one(3, at(5.0, 5.0))], DEFAULT_MAX)
        .expect("under the cap");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].order, 3);
}

#[test]
fn overlapping_boxes_at_different_orders_are_exactly_what_ordering_produces() {
    // The non-vacuity control the other way round: two boxes that cover one another are *supposed*
    // to end up at different orders, and a check that flagged those would flag every document.
    assert_eq!(
        overlaps(&[one(1, at(0.0, 0.0)), one(2, at(5.0, 5.0))], DEFAULT_MAX)
            .expect("under the cap"),
        vec![]
    );
}

#[test]
fn boxes_that_share_a_column_but_not_a_row_do_not_meet() {
    // The half of the answer a sweep along one axis cannot give on its own: both are open at the
    // same moment and their vertical extents are disjoint.
    assert_eq!(
        overlaps(&[one(1, at(0.0, 0.0)), one(1, at(2.0, 40.0))], DEFAULT_MAX)
            .expect("under the cap"),
        vec![]
    );
}

#[test]
fn an_order_class_past_the_cap_is_refused_rather_than_skipped() {
    // A cap that skipped quietly would switch the check off exactly where the document is large
    // enough for the failure to be hard to find.
    let class: Vec<Ordered> = (0..9)
        .map(|column| one(1, at(column as f32 * 20.0, 0.0)))
        .collect();
    let refused = overlaps(&class, 4).expect_err("nine is past a cap of four");
    assert_eq!(refused.order, 1);
    assert_eq!(refused.held, 9);
    assert_eq!(refused.max, 4);
}

#[test]
fn an_empty_rectangle_meets_nothing() {
    // A primitive covering no area cannot be drawn over anything, so it is not a violation however
    // many things share its order and its position.
    let empty = Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(DevicePx(0.0), DevicePx(0.0)),
    );
    assert_eq!(
        overlaps(&[one(1, empty), one(1, at(0.0, 0.0))], DEFAULT_MAX).expect("under the cap"),
        vec![]
    );
}
