//! A transition on an inherited property, over every frame it runs for.
//!
//! `color` is the ordinary inherited property a component library transitions, and it is the one
//! whose animation a repaint cannot express: what a descendant computes depends on it, so the
//! element has to go back through the cascade. That path is a second, separate traversal, and the
//! two things it can get wrong are invisible from opposite ends. The cascade can silently not run
//! at all, leaving every value frozen at the frame the transition started; or it can run and take
//! the shaped text with it, leaving a button with no label and nothing that would ever put one
//! back.
//!
//! # Why every frame here is repainted whole
//!
//! An undamaged fragment replays the primitives it emitted last frame, so a document at rest keeps
//! drawing text the pipeline could no longer produce if it were asked. Asking is the point: each
//! frame below is forced to emit everything, which is what a window being uncovered does — and
//! what an application does the moment anything at all is damaged over the label.

mod support;

use std::time::Duration;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// A little more than one frame at the surface's refresh rate, so each step is exactly one frame.
const FRAME: Duration = Duration::from_millis(17);

/// A button whose inherited text colour transitions on hover, with a label inside it.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .btn { display: block; width: 200px; height: 100px;
                          color: rgb(16, 16, 16);
                          transition: color 400ms linear }
                   .btn:hover { color: rgb(240, 240, 240) }
                   text { display: block }";

/// A pointer event at a point inside the button.
fn pointer_at(action: PointerAction, x: f32, y: f32) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// One button holding one label, under the sheet above.
fn labelled_button() -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    support::app_with_text(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column()
                        .class("btn")
                        .child(zgui_elements::text().child("abc")),
                )
                .into_view()
                .build(cx),
        )
    })
}

/// Draws one whole frame the way a window that has just been uncovered does.
///
/// The occlusion pair is the real path to a full redraw: nothing observed what the compositor did
/// to the surface while it was hidden, so everything is emitted again. That is what makes the
/// display list afterwards an answer to "what would be on the screen" rather than to "what changed
/// since the last frame".
fn redraw_whole(harness: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>) {
    harness.deliver_to_first(SurfaceEvent::Occluded(true));
    harness.deliver_to_first(SurfaceEvent::Occluded(false));
    harness.pump();
}

/// The tints of the glyph sprites in the last frame the window drew.
fn glyphs(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Vec<[f32; 4]> {
    harness.app().windows()[0]
        .scene()
        .primitives
        .mono_sprites
        .iter()
        .map(|sprite| sprite.color)
        .collect()
}

/// The one grey every glyph in the window is drawn in, as a byte, or nothing if they disagree.
///
/// Read off the sprites rather than off the element, because the element is not what decides it: a
/// glyph is drawn through the brush slot it named when it was shaped, and an element whose cascade
/// moved while that slot did not is an element that reports a colour nothing on the screen is in.
fn drawn_grey(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> Option<u8> {
    let greys: std::collections::BTreeSet<u8> = glyphs(harness)
        .iter()
        .map(|colour| (colour[0] * 255.0).round() as u8)
        .collect();
    (greys.len() == 1).then(|| greys.into_iter().next().expect("one grey"))
}

/// How many cascade results the window's brush table still answers to.
fn brush_keys(harness: &zgui_platform_headless::Harness<zgui_runtime::Runtime>) -> usize {
    harness.app().windows()[0].scene().text_paints.keys()
}

/// The label's glyphs are drawn in the colour the transition has reached, on every frame of it.
///
/// This is the whole point of animating an inherited `color`, and it is invisible to everything
/// else: the cascade settles on the new colour, the element reports it, the animation reports
/// itself as advancing, the glyphs are all drawn — and every one of them is drawn in the colour the
/// button had before the pointer arrived, for as long as the shaping survives, which is for ever.
#[test]
fn every_frame_of_a_colour_transition_reaches_the_glyphs() {
    let mut harness = labelled_button();
    harness.settle(8);
    redraw_whole(&mut harness);
    let start = drawn_grey(&harness).expect("the label is drawn in one colour before the hover");
    assert_eq!(
        start, 16,
        "the label was not drawn in the button's own colour"
    );

    harness.deliver_to_first(pointer_at(PointerAction::Moved, 10.0, 10.0));
    harness.settle(8);

    // One frame past the transition's 400ms at 17ms a frame, so the last reading is the end value.
    let mut greys = Vec::new();
    for _ in 0..25 {
        harness.advance(FRAME);
        harness.pump();
        redraw_whole(&mut harness);
        greys.push(drawn_grey(&harness).expect("every glyph of one label shares one colour"));
    }

    assert!(
        greys.windows(2).all(|pair| pair[1] >= pair[0]),
        "the drawn colour did not move towards the hovered one: {greys:?}"
    );
    assert!(
        greys.iter().any(|grey| (17..=239).contains(grey)),
        "the glyphs jumped from one end to the other, so nothing between was ever drawn: {greys:?}"
    );
    assert_eq!(
        greys.last().copied(),
        Some(240),
        "the transition ended and the glyphs are not in the colour it ended at: {greys:?}"
    );

    // A cascade result is a new object every frame, and the table is asked for one slot per
    // element, not one per frame: an entry per frame is a table that grows for as long as anything
    // on the screen is animating.
    let keys = brush_keys(&harness);
    assert!(
        keys <= 8,
        "the brush table answers to {keys} cascade results after 25 frames of one transition"
    );
}

#[test]
fn a_colour_transition_never_takes_the_text_it_applies_to_with_it() {
    let mut harness = labelled_button();
    harness.settle(8);
    redraw_whole(&mut harness);
    assert_eq!(
        glyphs(&harness).len(),
        3,
        "the label was not drawn before the hover, so nothing below is about a transition"
    );

    // A real hover, through the real router, so the transition starts the way it starts in an
    // application rather than because a test wrote a class.
    harness.deliver_to_first(pointer_at(PointerAction::Moved, 10.0, 10.0));
    harness.settle(8);

    // Well past the transition's 400ms, so the frames after it ends are covered too: the end is a
    // cascade of its own, and it is the one with nothing left to advance.
    for frame in 0..40 {
        harness.advance(FRAME);
        harness.pump();
        redraw_whole(&mut harness);
        let drawn = glyphs(&harness);
        assert_eq!(
            drawn.len(),
            3,
            "frame {frame} of the transition drew {} glyphs: the label has gone from the display \
             list, so anything that damages the button erases it from the screen for good",
            drawn.len()
        );
    }
}

/// An indeterminate progress bar: a keyframe animation that never ends, on a property that moves a
/// whole subtree and therefore cannot be composed as a repaint.
const PROGRESS_CSS: &str = "root { display: block; width: 400px; height: 300px }
                            @keyframes indeterminate {
                                from { transform: translateX(-100px) }
                                to { transform: translateX(400px) }
                            }
                            .bar { display: block; width: 100px; height: 8px;
                                   background-color: rgb(90, 90, 200);
                                   animation: indeterminate 1200ms linear infinite }";

/// A never-ending cascading animation runs for hundreds of frames without tripping an assertion.
///
/// The failure this is written against is a debug-build abort inside the style engine: an element
/// carrying an animation restyle hint that the ordinary traversal reaches before the animation-only
/// one has processed it. An optimised build does not abort — it silently cascades what it was told
/// not to — so this case is only ever an oracle when it runs with debug assertions on, which is why
/// it asserts on the animation as well: a run that quietly stopped animating cannot trip anything.
#[test]
fn an_indeterminate_progress_bar_runs_for_hundreds_of_frames() {
    let mut harness = support::app(PROGRESS_CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::column().class("bar"))
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);

    let mut frames = 0;
    for _ in 0..600 {
        harness.advance(FRAME);
        frames += harness.pump();
    }
    assert!(
        frames > 500,
        "the loop stopped waking for an infinite animation after {frames} frames, so the frames \
         that would have tripped the assertion were never run"
    );
}
