//! Where a transition leaves the element's own computed style once it has finished.
//!
//! A running transition does not write into the cascade. The cascade that *creates* one evaluates
//! it at time zero, so the computed style an element carries for the whole of a transition holds
//! the value it set off from, and every frame in between comes from the interpolation instead.
//!
//! That makes the frame the transition ends on the only frame in which the element's own style can
//! be brought to the destination, and the tick's report is the only thing that says so. Unreported,
//! the element goes on being drawn from a style that holds its starting value — which is a control
//! that lights up under the pointer for the length of the transition and then goes dark again while
//! the pointer is still on it.
//!
//! The cases here are written for the **repaint-only** tier, because that is the one a colour
//! transition takes and the one where the failure is invisible from everywhere else: the values are
//! interpolated correctly, the report is correct while they move, the counters count, and the
//! element still ends up back where it started.

use zgui_style::{AnimatedProperties, AnimationTime};
use zgui_vocab::UiState;

use crate::support::{Harness, animation_frame, background};

/// How long the transition runs, in seconds.
const DURATION: f64 = 0.4;

/// How long one frame is, in seconds.
const FRAME: f64 = 0.016;

/// A background transition, which is the repaint-only tier: nothing inherits it and no box moves.
const SHEET: &str = "root { display: block }
                     .chip { display: block; background-color: rgb(0, 0, 0);
                             transition: background-color 400ms linear }
                     .chip:hover { background-color: rgb(200, 200, 200) }";

/// Where the transition is going.
const DESTINATION: (u8, u8, u8) = (200, 200, 200);

/// Where it sets off from.
const ORIGIN: (u8, u8, u8) = (0, 0, 0);

/// A styled document holding one chip, with nothing yet hovered.
fn document() -> (Harness, zgui_dom::NodeIndex) {
    let mut harness = Harness::new();
    harness.add_author(SHEET);
    let chip = harness.append(harness.root, "box");
    harness.set_classes(chip, &["chip"]);
    harness.frame();
    harness.retire_all();
    (harness, chip)
}

/// Hovers the chip, which is what creates the transition.
fn hover(harness: &mut Harness, chip: zgui_dom::NodeIndex, on: bool) {
    harness.set_state(chip, UiState::HOVER, on);
    harness.frame();
    harness.retire_all();
}

#[test]
fn a_colour_transition_leaves_the_element_at_the_colour_it_was_going_to() {
    let (mut harness, chip) = document();
    assert_eq!(background(&harness, chip), ORIGIN);

    hover(&mut harness, chip, true);
    assert_eq!(
        background(&harness, chip),
        ORIGIN,
        "the cascade that creates a transition evaluates it at time zero, so this is the value \
         the element carries while it runs — if it were already the destination, nothing below \
         would be measuring the settling at all"
    );

    // Well past the transition's own length, so every frame of it and several after it have run.
    for frame in 1..=32u32 {
        animation_frame(&mut harness, f64::from(frame) * FRAME);
    }

    assert_eq!(
        background(&harness, chip),
        DESTINATION,
        "the transition finished and the element's own style still holds the colour it set off \
         from, so the frame the interpolation is dropped on paints it back at the start"
    );
}

#[test]
fn the_settled_colour_survives_every_frame_after_it() {
    let (mut harness, chip) = document();
    hover(&mut harness, chip, true);
    for frame in 1..=32u32 {
        animation_frame(&mut harness, f64::from(frame) * FRAME);
    }

    // Not one reading but a run of them: a value that is right on the frame the transition ended
    // and wrong on the next is exactly as broken, and reads the same from a single assertion.
    for frame in 33..=48u32 {
        animation_frame(&mut harness, f64::from(frame) * FRAME);
        assert_eq!(
            background(&harness, chip),
            DESTINATION,
            "the colour did not hold at frame {frame}"
        );
    }
}

#[test]
fn taking_the_state_away_again_settles_back_at_the_base_colour() {
    let (mut harness, chip) = document();
    hover(&mut harness, chip, true);
    for frame in 1..=32u32 {
        animation_frame(&mut harness, f64::from(frame) * FRAME);
    }
    assert_eq!(background(&harness, chip), DESTINATION);

    hover(&mut harness, chip, false);
    for frame in 33..=80u32 {
        animation_frame(&mut harness, f64::from(frame) * FRAME);
    }
    assert_eq!(
        background(&harness, chip),
        ORIGIN,
        "the reverse transition finished somewhere other than the colour the sheet asks for"
    );
}

#[test]
fn the_frame_a_repaint_only_transition_ends_on_reports_the_element_as_needing_a_cascade() {
    let (mut harness, chip) = document();
    hover(&mut harness, chip, true);

    // The tick alone, without the restyle, so what is asserted is the *report* rather than what a
    // caller happened to do with it. Every frame of the transition, and the ones after its end.
    let mut ending = Vec::new();
    for frame in 1..=32u32 {
        let now = AnimationTime(f64::from(frame) * FRAME);
        let report = harness.engine.animation_tick(&harness.document, now);
        for element in &report.elements {
            if element.index == chip && element.crossed && !element.advancing {
                ending.push((f64::from(frame) * FRAME, element.properties));
            }
        }
        // The cascade the report is asking for, so the next tick sees the style it produced.
        for element in &report.elements {
            if element.properties.is_paint_only() {
                continue;
            }
            harness
                .engine
                .mark_animation_restyle(&harness.document, element.index);
        }
        harness.engine.restyle(&mut harness.document, None);
        harness.retire_all();
    }

    let (at, properties) = *ending.first().expect(
        "the tick never reported the element on the frame its transition ended, so nothing \
         downstream can know that its computed style is still the one it started from",
    );
    assert!(
        at >= DURATION,
        "the end was reported at {at}s, before the transition's {DURATION}s was up"
    );
    assert!(
        !properties.is_paint_only(),
        "the end was reported as a repaint, and a repaint composes values over the very style \
         that has to be replaced: {properties:?}"
    );
    assert!(
        properties.contains(AnimatedProperties::CASCADED),
        "the frame an animation ends on owes a cascade and nothing else: {properties:?}"
    );
}
