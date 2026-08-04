//! The outline the inspector draws over what it is pointing at.
//!
//! Three things have to hold. It has to land on the thing being pointed at, or it is pointing at
//! the wrong thing. It has to go away again, or the last thing somebody hovered stays outlined for
//! the rest of the session. And holding it still has to cost nothing, because an outline is what
//! somebody looks at *while* reading a number somewhere else on the screen.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test target is a test module"
)]

mod support;

use zgui_devtools::{DevTools, Tab};

use support::{box_of, centre, find_box, frames_over, moved, opened, run};

/// How close two lengths have to be to be the same length.
const CLOSE: f32 = 1.0;

/// Picking aims the outline at whatever the pointer is over.
#[test]
fn picking_outlines_what_the_pointer_is_over() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    run(&mut harness, 8);

    let scale = harness.app().windows()[0].scale().get();
    let target = box_of(&harness, "target");
    tools.set_picking(true);
    harness.deliver_to_first(moved(centre(target, scale)));
    run(&mut harness, 8);

    let outline = box_of(&harness, "zgui-devtools-highlight");
    assert!(
        (outline.origin.x.0 - target.origin.x.0).abs() < CLOSE
            && (outline.origin.y.0 - target.origin.y.0).abs() < CLOSE
            && (outline.size.width.0 - target.size.width.0).abs() < CLOSE
            && (outline.size.height.0 - target.size.height.0).abs() < CLOSE,
        "the outline is at {outline:?} and the box it names is at {target:?}"
    );
}

/// Leaving picking takes the outline away.
///
/// Without this the last thing somebody aimed at stays outlined for the rest of the session, over
/// an application nobody is inspecting any more.
#[test]
fn leaving_picking_takes_the_outline_away() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    run(&mut harness, 8);

    let scale = harness.app().windows()[0].scale().get();
    let target = box_of(&harness, "target");
    tools.set_picking(true);
    harness.deliver_to_first(moved(centre(target, scale)));
    run(&mut harness, 8);
    assert!(
        find_box(&harness, "zgui-devtools-highlight").is_some(),
        "nothing was outlined to begin with, so this proves nothing"
    );

    tools.set_picking(false);
    tools.set_highlighted(None);
    run(&mut harness, 8);
    assert!(
        find_box(&harness, "zgui-devtools-highlight").is_none(),
        "the outline is still drawn after picking ended"
    );
}

/// A held outline over a still document draws no frames.
///
/// The outline is what somebody looks at while reading something else, so it is held for a long
/// time by definition — and an outline resolved from the layout every frame is a window that never
/// idles for exactly as long as anybody is using the feature.
#[test]
fn a_held_outline_leaves_the_window_idle() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    tools.show(Tab::Elements);
    run(&mut harness, 8);

    let scale = harness.app().windows()[0].scale().get();
    let target = box_of(&harness, "target");
    tools.set_picking(true);
    harness.deliver_to_first(moved(centre(target, scale)));
    run(&mut harness, 120);

    let frames = frames_over(&mut harness, 300);
    assert_eq!(
        frames, 0,
        "a held outline woke the window {frames} times over 300 vsyncs"
    );
}

/// The inspector never outlines, or picks, itself.
///
/// The listeners are on a host that wraps the application *and* the panel, so without a filter the
/// pointer on its way to anything at all would pick the panel it crossed — and the only element
/// anybody could inspect would be the inspector.
#[test]
fn picking_never_picks_the_inspector_itself() {
    let tools = DevTools::new();
    let mut harness = opened(tools);
    tools.set_open(true);
    run(&mut harness, 8);

    let scale = harness.app().windows()[0].scale().get();
    let target = box_of(&harness, "target");
    tools.set_picking(true);
    harness.deliver_to_first(moved(centre(target, scale)));
    run(&mut harness, 8);
    let aimed = tools.picked();
    assert!(aimed.is_some(), "nothing was picked to begin with");

    // Now across the panel's own tab strip, which is what a pointer crosses on its way anywhere.
    let bar = box_of(&harness, "zgui-devtools__bar");
    harness.deliver_to_first(moved(centre(bar, scale)));
    run(&mut harness, 8);

    assert_eq!(
        tools.picked(),
        aimed,
        "moving the pointer over the panel picked the panel"
    );
}
