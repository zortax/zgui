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

use support::{
    box_of, centre, find_box, frames_over, moved, pressed, released, resize, run, sized,
};

/// How close two lengths have to be to be the same length.
const CLOSE: f32 = 0.5;

/// How wide the panel is before anybody drags it.
const DEFAULT: f32 = 560.0;

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
                (panel.size.width.0 - DEFAULT).abs() < CLOSE,
                "the panel is {}px wide rather than the {DEFAULT} it opens at",
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
    let divider = box_of(&harness, "zgui-devtools__divider");
    let expected = 1080.0 - DEFAULT - divider.size.width.0;
    assert!(
        (application.size.width.0 - expected).abs() < CLOSE,
        "the application is {}px wide in a 1080px window beside a {DEFAULT}px panel and a \
         {}px divider",
        application.size.width.0,
        divider.size.width.0
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

/// Drags the divider `by` CSS pixels, negative being leftwards, and lets the window settle.
///
/// The press has to land on the divider itself, so it is found rather than guessed: the panel's
/// width is what this is about to change, and a test that computed the divider's position from the
/// width it expects would pass whatever the divider did.
fn drag_divider(harness: &mut zgui_platform_headless::Harness<zgui::runtime::Runtime>, by: f32) {
    let scale = harness.app().windows()[0].scale().get();
    let grip = centre(box_of(harness, "zgui-devtools__divider"), scale);
    harness.deliver_to_first(pressed(grip));
    harness.settle(16);
    harness.deliver_to_first(moved(zgui::geom::Point::new(
        CssPx(grip.x.0 + by),
        CssPx(grip.y.0),
    )));
    harness.settle(16);
    harness.deliver_to_first(released(zgui::geom::Point::new(
        CssPx(grip.x.0 + by),
        CssPx(grip.y.0),
    )));
    run(harness, 8);
}

/// Dragging the divider towards the application widens the panel by what the pointer moved.
#[test]
fn dragging_the_divider_resizes_the_panel() {
    let mut harness = opened_over(0, 720.0);
    drag_divider(&mut harness, -100.0);

    let panel = box_of(&harness, "zgui-devtools");
    assert!(
        (panel.size.width.0 - (DEFAULT + 100.0)).abs() < CLOSE,
        "a 100px drag left made the {DEFAULT}px panel {}px wide",
        panel.size.width.0
    );

    // And the application gave up exactly that width rather than being covered by it.
    let application = box_of(&harness, "page");
    let divider = box_of(&harness, "zgui-devtools__divider");
    let expected = 1080.0 - panel.size.width.0 - divider.size.width.0;
    assert!(
        (application.size.width.0 - expected).abs() < CLOSE,
        "the panel took {}px and the application is {}px of a 1080px window",
        panel.size.width.0,
        application.size.width.0
    );
}

/// The drag stops at both ends rather than running off them.
#[test]
fn the_divider_clamps_at_both_ends() {
    // Far enough right to ask for a negative width.
    let mut harness = opened_over(0, 720.0);
    drag_divider(&mut harness, 900.0);
    let panel = box_of(&harness, "zgui-devtools");
    assert!(
        (panel.size.width.0 - 280.0).abs() < CLOSE,
        "dragged 900px right the panel is {}px wide rather than the 280px floor",
        panel.size.width.0
    );

    // And far enough left to ask for the whole window, which would take the divider off the edge
    // with it and leave no way back.
    let mut harness = opened_over(0, 720.0);
    drag_divider(&mut harness, -900.0);
    let panel = box_of(&harness, "zgui-devtools");
    let application = box_of(&harness, "page");
    assert!(
        application.size.width.0 >= 160.0 - CLOSE,
        "dragged 900px left the application is {}px wide, under the 160px it keeps",
        application.size.width.0
    );
    assert!(
        panel.size.width.0 < 1080.0,
        "the panel took the whole window"
    );
}

/// A divider nobody is holding lets the window idle again.
///
/// The drag itself draws frames, as it must. What matters is that letting go ends them: a resize
/// that left a signal being written every frame would keep the window awake for as long as the
/// panel stayed open, which is the one cost the inspector is not allowed to have.
#[test]
fn a_released_divider_returns_the_window_to_idle() {
    let mut harness = opened_over(0, 720.0);
    drag_divider(&mut harness, -60.0);
    run(&mut harness, 64);

    let frames = frames_over(&mut harness, 120);
    assert_eq!(frames, 0, "a released divider drew {frames} frames");
}

/// The width outlives the panel being closed and opened again.
#[test]
fn the_width_survives_closing_and_reopening() {
    let tools = DevTools::new();
    let mut harness = sized(tools, Size::new(DevicePx(1080.0), DevicePx(720.0)), 0);
    tools.set_open(true);
    run(&mut harness, 8);
    drag_divider(&mut harness, -80.0);
    let dragged = box_of(&harness, "zgui-devtools").size.width.0;

    tools.set_open(false);
    run(&mut harness, 8);
    tools.set_open(true);
    run(&mut harness, 8);

    let reopened = box_of(&harness, "zgui-devtools").size.width.0;
    assert!(
        (reopened - dragged).abs() < CLOSE,
        "the panel was dragged to {dragged}px and came back {reopened}px wide"
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

/// A closed inspector leaves the application the window's height as well as its width.
///
/// The host and the application wrapper are boxes the inspector adds around a view that was a
/// direct child of the root. The root is a block of the window's height, and a block's child is as
/// tall as its own content — so a wrapper that does not say otherwise turns a view that filled the
/// window into a view with the window's leftover height empty underneath it, and only while the
/// panel is *shut*, which is the state an application spends most of its life in.
#[test]
fn a_closed_inspector_leaves_the_application_the_window_s_height() {
    let tools = DevTools::new();
    let harness = sized(tools, Size::new(DevicePx(1080.0), DevicePx(720.0)), 0);

    let application = box_of(&harness, "zgui-devtools-app");
    assert!(
        (application.size.height.0 - 720.0).abs() < CLOSE,
        "with the panel shut the application is {}px tall in a 720px window",
        application.size.height.0
    );
}

/// And an open one still does.
#[test]
fn an_open_inspector_leaves_the_application_the_window_s_height() {
    let harness = opened_over(0, 720.0);

    let application = box_of(&harness, "zgui-devtools-app");
    assert!(
        (application.size.height.0 - 720.0).abs() < CLOSE,
        "with the panel open the application is {}px tall in a 720px window",
        application.size.height.0
    );
}
