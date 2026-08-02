//! Every icon's outline, measured.
//!
//! A path that does not parse, that leaves its square, or whose counter is filled in is invisible
//! in a class list and in an accessibility tree alike — the first sign of any of them is a
//! screenshot nobody took. So the geometry is asserted here, over the same constants a program
//! links.

use zgui::elements::kurbo::{Point, Shape};
use zgui_ui_icons::IconData;
use zgui_ui_icons::set::{arrow, chevron, mark, status, ui};

/// Every icon in the set, so a new one joins these assertions by being added here.
fn every_icon() -> Vec<IconData> {
    vec![
        arrow::ARROW_UP,
        arrow::ARROW_DOWN,
        arrow::ARROW_LEFT,
        arrow::ARROW_RIGHT,
        chevron::CHEVRON_UP,
        chevron::CHEVRON_DOWN,
        chevron::CHEVRON_LEFT,
        chevron::CHEVRON_RIGHT,
        mark::CHECK,
        mark::MINUS,
        mark::PLUS,
        mark::CROSS,
        mark::DISC,
        mark::DOT,
        status::ALERT_CIRCLE,
        status::ALERT_TRIANGLE,
        status::INFO,
        status::CHECK_CIRCLE,
        status::CROSS_CIRCLE,
        ui::SEARCH,
        ui::SPINNER,
        ui::ELLIPSIS,
    ]
}

#[test]
fn every_outline_parses_and_stays_inside_the_square_it_declares() {
    for icon in every_icon() {
        let path = icon.path();
        assert!(
            !path.elements().is_empty(),
            "`{}` parsed to nothing at all",
            icon.name()
        );
        let bounds = path.bounding_box();
        let side = icon.view_box();
        assert!(
            bounds.x0 >= -0.01
                && bounds.y0 >= -0.01
                && bounds.x1 <= side + 0.01
                && bounds.y1 <= side + 0.01,
            "`{}` is drawn outside its own {side}-unit square: {bounds:?}",
            icon.name()
        );
    }
}

#[test]
fn every_name_is_the_kebab_case_of_something_and_no_two_are_the_same() {
    let mut seen: Vec<&str> = Vec::new();
    for icon in every_icon() {
        let name = icon.name();
        assert!(
            name.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-') && !name.is_empty(),
            "`{name}` is not a kebab-case name"
        );
        assert!(!seen.contains(&name), "two icons are called `{name}`");
        seen.push(name);
    }
}

#[test]
fn every_outline_actually_covers_its_own_middle() {
    // A tick written as an open polyline parses, measures and draws nothing. The winding number at
    // a point the shape has to cover is the direct question, and it is the one a bounding box
    // cannot answer.
    for (icon, inside) in [
        (mark::CHECK, Point::new(9.6, 15.5)),
        (mark::MINUS, Point::new(12.0, 12.0)),
        (mark::PLUS, Point::new(12.0, 12.0)),
        (mark::CROSS, Point::new(12.0, 12.0)),
        (mark::DISC, Point::new(12.0, 12.0)),
        (mark::DOT, Point::new(12.0, 12.0)),
        (chevron::CHEVRON_DOWN, Point::new(12.0, 14.0)),
        (chevron::CHEVRON_UP, Point::new(12.0, 10.0)),
        (chevron::CHEVRON_LEFT, Point::new(10.0, 12.0)),
        (chevron::CHEVRON_RIGHT, Point::new(14.0, 12.0)),
        (arrow::ARROW_DOWN, Point::new(12.0, 8.0)),
        (arrow::ARROW_UP, Point::new(12.0, 16.0)),
        (arrow::ARROW_LEFT, Point::new(16.0, 12.0)),
        (arrow::ARROW_RIGHT, Point::new(8.0, 12.0)),
        (ui::ELLIPSIS, Point::new(12.0, 12.0)),
    ] {
        assert_ne!(
            icon.path().winding(inside),
            0,
            "`{}` does not cover {inside:?}, which is inside the shape it is meant to be",
            icon.name()
        );
    }
}

#[test]
fn a_counter_is_a_hole_rather_than_a_second_shape_over_the_first() {
    // The defect this catches is a subpath wound the same way as the one around it: the outline
    // still parses, still measures the same, and draws a filled disc where a ring was meant.
    for (icon, in_the_ring, in_the_hole) in [
        (status::INFO, Point::new(12.0, 4.0), Point::new(6.5, 12.0)),
        (
            status::ALERT_CIRCLE,
            Point::new(12.0, 4.0),
            Point::new(6.5, 12.0),
        ),
        (
            status::CHECK_CIRCLE,
            Point::new(12.0, 4.0),
            Point::new(6.5, 12.0),
        ),
        (
            status::CROSS_CIRCLE,
            Point::new(12.0, 4.0),
            Point::new(6.5, 12.0),
        ),
        (
            status::ALERT_TRIANGLE,
            Point::new(12.0, 19.6),
            Point::new(8.0, 17.0),
        ),
        (ui::SEARCH, Point::new(10.5, 4.5), Point::new(10.5, 8.0)),
        (ui::SPINNER, Point::new(12.0, 4.0), Point::new(12.0, 12.0)),
    ] {
        let path = icon.path();
        assert_ne!(
            path.winding(in_the_ring),
            0,
            "`{}` is not filled at {in_the_ring:?}, where its ring is",
            icon.name()
        );
        assert_eq!(
            path.winding(in_the_hole),
            0,
            "`{}` is filled at {in_the_hole:?}, so its counter is not a hole",
            icon.name()
        );
    }
}

#[test]
fn the_glyph_inside_a_status_ring_is_solid() {
    // The other half of the same defect: a hole wound the wrong way takes the glyph with it.
    for (icon, point) in [
        (status::INFO, Point::new(12.0, 12.0)),
        (status::ALERT_CIRCLE, Point::new(12.0, 10.0)),
        (status::ALERT_TRIANGLE, Point::new(12.0, 12.0)),
        (status::CHECK_CIRCLE, Point::new(10.7, 14.0)),
        (status::CROSS_CIRCLE, Point::new(12.0, 12.0)),
        (ui::SEARCH, Point::new(19.0, 19.0)),
    ] {
        assert_ne!(
            icon.path().winding(point),
            0,
            "`{}` has no ink at {point:?}, where its glyph is",
            icon.name()
        );
    }
}
