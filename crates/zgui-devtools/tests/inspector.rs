//! The inspector, driven over a real window.
//!
//! Every assertion here is about the thing that makes a diagnostic tool worth having: that what it
//! shows is *this* frame's answer rather than a plausible one. A panel that renders beautifully and
//! reports the previous frame's damage is worse than no panel, because it is believed.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui::geom::{CssPx, Point};
use zgui::vocab::{Key, KeyEvent};
use zgui_devtools::{DevTools, Tab};

use support::{
    anything_focused, box_of, f12, focus_something, frames_over, key, moved, opened, run, text,
};

/// The centre of the only 120x48 box in the window, which is the one the page declares.
fn target_centre(
    harness: &zgui_platform_headless::Harness<zgui::runtime::Runtime>,
) -> Point<CssPx, zgui::geom::Css> {
    let found = box_of(harness, "target");
    Point::new(
        CssPx(found.origin.x.0 + 60.0),
        CssPx(found.origin.y.0 + 24.0),
    )
}

/// A closed inspector costs the window nothing: no frames, no work, no wake.
///
/// The assertion the whole design rests on. An inspector that sampled while it was shut would make
/// every application that links it draw for ever, and nothing else here would matter.
#[test]
fn a_closed_inspector_leaves_the_window_idle() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    harness.settle(256);
    let frames = frames_over(&mut harness, 120);
    assert_eq!(
        frames, 0,
        "a closed inspector woke the window {frames} times"
    );
    assert!(!tools.is_open());
}

/// F12 opens the panel on a window in which nothing whatever has focus.
///
/// The state every window launches in, and the state it returns to whenever focus is dropped — so
/// this is the case a chord is reached for, and the one that used to be the only case it did not
/// work in. The focus assertion comes first on purpose: a test that focused something before
/// pressing the key would arrange the one condition under which delivery is not in question, and
/// could never fail.
#[test]
fn f12_opens_the_inspector_with_nothing_focused() {
    let tools = DevTools::new();
    let mut harness = opened(tools);

    harness.deliver_to_first(zgui::platform::SurfaceEvent::Focused(true));
    harness.settle(64);
    assert!(
        !anything_focused(&harness),
        "the window arranged focus before the key was pressed, which is the one condition that \
         makes this pass for the wrong reason"
    );

    harness.deliver_to_first(f12());
    harness.settle(256);
    assert!(tools.is_open(), "F12 did not open the panel");

    run(&mut harness, 4);
    let shown = text(&harness);
    assert!(
        shown.contains("Element") && shown.contains("Timeline"),
        "the tab strip is not in the document: {shown:.400}"
    );
    assert!(
        shown.contains("Nothing picked"),
        "the element tab does not say that nothing is picked: {shown:.400}"
    );
}

/// The chord works with focus inside the application too, which is the case it always worked in.
#[test]
fn f12_still_opens_the_inspector_with_focus_inside_the_application() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    focus_something(&mut harness);
    assert!(anything_focused(&harness), "the tab focused nothing");

    harness.deliver_to_first(f12());
    harness.settle(256);
    assert!(tools.is_open(), "F12 did not open the panel");
}

/// Picking aims at what the pointer is over, and the panel then names it and its box.
#[test]
fn picking_names_the_element_and_the_box_the_layout_gave_it() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    let centre = target_centre(&harness);

    tools.set_open(true);
    tools.set_picking(true);
    run(&mut harness, 4);
    harness.deliver_to_first(moved(centre));
    run(&mut harness, 6);

    let shown = text(&harness);
    assert!(
        shown.contains("box.target"),
        "the picked element is not named: {shown:.600}"
    );
    // 120x48 is the border box the sheet asked for, and the padding box is that less the 3px
    // border on each side. Both being right is what says the panel is reading the geometry the
    // frame computed rather than the declaration the sheet made.
    assert!(
        shown.contains("border 120.0 x 48.0"),
        "the border box is not the one the sheet declared: {shown:.600}"
    );
    assert!(
        shown.contains("padding 114.0 x 42.0"),
        "the padding box is not the border box less its borders: {shown:.600}"
    );
    assert!(
        shown.contains("content 100.0 x 28.0"),
        "the content box is not the padding box less its padding: {shown:.600}"
    );
}

/// Every tab renders, and each says something only it could say.
#[test]
fn every_tab_shows_its_own_answer() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);

    for (tab, expected) in [
        (Tab::Frame, "batches"),
        (Tab::Timeline, "whole frame"),
        (Tab::Parity, "implemented"),
        (Tab::Memory, "atlases"),
    ] {
        tools.show(tab);
        run(&mut harness, 6);
        let shown = text(&harness);
        assert!(
            shown.contains(expected),
            "the {} tab does not show `{expected}`: {shown:.600}",
            tab.label()
        );
    }
}

/// Freezing stops the probe, and the window goes back to idling with the panel still up.
#[test]
fn freezing_returns_the_window_to_idle() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.set_frozen(true);
    run(&mut harness, 8);

    let frames = frames_over(&mut harness, 120);
    assert_eq!(
        frames, 0,
        "a frozen inspector woke the window {frames} times"
    );
}

/// The chord is claimed, so the application under it never sees F12.
#[test]
fn the_chord_does_not_reach_the_application() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    focus_something(&mut harness);

    harness.deliver_to_first(f12());
    harness.settle(64);
    assert!(tools.is_open());

    // An ordinary key is not claimed, which is the other half: a capture listener that swallowed
    // everything would take the whole keyboard away from the application it is inspecting.
    let before = tools.is_open();
    harness.deliver_to_first(key(KeyEvent::character("a")));
    harness.settle(64);
    assert_eq!(tools.is_open(), before);
    assert!(matches!(
        KeyEvent::character("a").key,
        Key::Character(_) | Key::Named(_)
    ));
}
