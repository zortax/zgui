//! Where the panel is, and how tall.
//!
//! The panel used to be positioned against the application's own box rather than against the
//! window, so its height was the *page's* content height: a stub in the corner over a short
//! document, and a strip running off the bottom of the screen over a long one. Neither is a
//! diagnostic surface, and the second is worse than the first, because a body with room for
//! everything never scrolls and the panel simply loses whatever did not fit on the screen.
//!
//! Docked, its height is not a declaration at all — it is the cross size of the flex line it sits
//! on, which is the window. So the assertions below vary the two things that used to decide it,
//! the window's height and the document's, and expect the answer not to move.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui::geom::{CssPx, Device, DevicePx, Size};
use zgui_devtools::DevTools;

use support::{box_of, find_box, resize, run, sized};

/// How close two lengths have to be to be the same length.
const CLOSE: f32 = 0.5;

/// A window `height` tall over a page of `rows` rows, with the panel open.
fn opened_over(
    rows: usize,
    height: f32,
) -> zgui_platform_headless::Harness<zgui::runtime::Runtime> {
    let tools = DevTools::new();
    let size = Size::new(DevicePx(1080.0), DevicePx(height));
    let mut harness = sized(tools, size, rows);
    tools.set_open(true);
    run(&mut harness, 8);
    harness
}

/// The panel is the viewport's height, at three window heights and two document heights.
#[test]
fn the_panel_is_as_tall_as_the_viewport_and_no_taller() {
    for rows in [0, 40] {
        for height in [720.0_f32, 400.0, 1000.0] {
            let harness = opened_over(rows, height);
            let panel = box_of(&harness, "zgui-devtools");
            assert!(
                (panel.size.height.0 - height).abs() < CLOSE,
                "over a {rows}-row page in a {height}px window the panel is \
                 {}px tall",
                panel.size.height.0
            );
            assert!(
                (panel.size.width.0 - 420.0).abs() < CLOSE,
                "the panel is {}px wide rather than the 420 the sheet asks for",
                panel.size.width.0
            );
        }
    }
}

/// A window that is resized takes the panel with it, rather than leaving it the old height.
#[test]
fn resizing_the_window_resizes_the_panel() {
    let mut harness = opened_over(40, 720.0);
    assert!((box_of(&harness, "zgui-devtools").size.height.0 - 720.0).abs() < CLOSE);

    resize(&mut harness, Size::new(DevicePx(1080.0), DevicePx(360.0)));
    run(&mut harness, 8);
    let panel = box_of(&harness, "zgui-devtools");
    assert!(
        (panel.size.height.0 - 360.0).abs() < CLOSE,
        "after the window halved, the panel is still {}px tall",
        panel.size.height.0
    );
}

/// The body has less room than its content, which is what makes `overflow-y: auto` mean anything.
#[test]
fn the_panel_body_is_shorter_than_the_panel_and_scrolls() {
    let harness = opened_over(40, 400.0);
    let panel = box_of(&harness, "zgui-devtools");
    let body = box_of(&harness, "zgui-devtools__body");
    assert!(
        body.size.height.0 < panel.size.height.0,
        "the body is {}px inside a {}px panel, so the tab strip above it has no room",
        body.size.height.0,
        panel.size.height.0
    );
    assert!(
        body.size.height.0 > 0.0,
        "the body has no height at all, so nothing in the panel is visible"
    );
}

/// The application is narrowed by the panel rather than hidden under it.
///
/// The whole reason to dock. An overlay conceals the right-hand edge of the page — which, on the
/// side a panel is usually put, is where a person has just clicked the thing they want to inspect.
#[test]
fn the_application_keeps_the_width_the_panel_does_not_take() {
    let harness = opened_over(0, 720.0);
    assert!(
        find_box(&harness, "zgui-devtools").is_some(),
        "the panel is not in the document, so this proves nothing about where it is"
    );
    let application = box_of(&harness, "page");
    assert!(
        (application.size.width.0 - 660.0).abs() < CLOSE,
        "the application is {}px wide in a 1080px window beside a 420px panel",
        application.size.width.0
    );
}

/// Scrolling the application leaves the panel exactly where it is.
///
/// A docked panel is a sibling of the application rather than a box inside its flow, and the
/// application has its own scroll while the panel is open — so this is true by construction rather
/// than by arrangement. It is asserted anyway, because the previous arrangement made it false and
/// nothing about the panel's own declarations said so.
#[test]
fn scrolling_the_application_leaves_the_panel_where_it_is() {
    let mut harness = opened_over(40, 400.0);
    let before = box_of(&harness, "zgui-devtools");

    for _ in 0..12 {
        harness.deliver_to_first(zgui::platform::SurfaceEvent::Wheel {
            event: zgui::vocab::WheelEvent {
                id: zgui::vocab::PointerId::MOUSE,
                kind: zgui::vocab::PointerKind::Mouse,
                position: zgui::geom::Point::new(CssPx(300.0), CssPx(200.0)),
                delta: zgui::vocab::ScrollDelta::Lines { x: 0.0, y: -3.0 },
                phase: zgui::vocab::ScrollPhase::Discrete,
            },
            modifiers: zgui::vocab::Modifiers::NONE,
            timestamp: zgui::vocab::Timestamp::ORIGIN,
        });
        harness.settle(16);
    }
    run(&mut harness, 8);

    let after = box_of(&harness, "zgui-devtools");
    assert!(
        (after.origin.y.0 - before.origin.y.0).abs() < CLOSE
            && (after.size.height.0 - before.size.height.0).abs() < CLOSE,
        "twelve wheel detents over the application moved the panel from {before:?} to {after:?}"
    );
}

/// A closed inspector leaves the application exactly the window's width.
#[test]
fn a_closed_inspector_takes_no_width_from_the_application() {
    let tools = DevTools::new();
    let harness = sized(tools, Size::new(DevicePx(1080.0), DevicePx(720.0)), 0);
    assert!(
        find_box(&harness, "zgui-devtools").is_none(),
        "the panel is in the document while the inspector is closed"
    );
    let application: zgui::geom::Rect<DevicePx, Device> = box_of(&harness, "page");
    assert!(
        (application.size.width.0 - 1080.0).abs() < CLOSE,
        "a closed inspector narrowed the application to {}px",
        application.size.width.0
    );
}
