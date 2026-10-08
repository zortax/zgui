//! What a finished keyframe animation that fills forwards holds through a later restyle.
//!
//! A surface centred by `left: 50%; top: 50%` holds its centring transform only as the forward fill
//! of its entrance animation. A theme change matches the element again, and the match reads the
//! animation declarations from the document's animation set. The cases here finish the animation,
//! restyle the element the way a theme change does, and assert on the computed transform.

use style::values::computed::Transform;
use zgui_dom::NodeIndex;
use zgui_style::{AnimationEdge, AnimationTime, Lifecycle};

use crate::support::{Harness, animation_frame};

/// How long one frame is, in seconds.
const FRAME: f64 = 0.016;

/// A light theme, a centred surface with an entrance that fills both ways, and a reference box.
const LIGHT: &str = "root { display: block; --tone: rgb(250, 250, 250) }
                     .surface { display: block; position: fixed; left: 50%; top: 50%;
                                background-color: var(--tone);
                                --place: translate(-50%, -50%);
                                animation: enter 100ms linear both }
                     .surface.busy { color: rgb(1, 2, 3) }
                     .surface.paused { animation-play-state: paused }
                     .surface.other { animation: other 100ms linear both }
                     .reference { display: block;
                                  transform: translate(-50%, -50%) translate(0px, 0px) scale(1) }
                     @keyframes other {
                         from { opacity: 0 }
                         to { opacity: 1 }
                     }
                     @keyframes enter {
                         from { transform: var(--place, translate(0px, 0px)) translate(0px, 8px)
                                           scale(0.95) }
                         to { transform: var(--place, translate(0px, 0px)) translate(0px, 0px)
                                         scale(1) }
                     }";

/// The same sheet with the theme's custom property changed.
const DARK: &str = "root { display: block; --tone: rgb(20, 20, 20) }
                    .surface { display: block; position: fixed; left: 50%; top: 50%;
                               background-color: var(--tone);
                               --place: translate(-50%, -50%);
                               animation: enter 100ms linear both }
                    .surface.busy { color: rgb(1, 2, 3) }
                    .reference { display: block;
                                 transform: translate(-50%, -50%) translate(0px, 0px) scale(1) }
                    @keyframes enter {
                        from { transform: var(--place, translate(0px, 0px)) translate(0px, 8px)
                                          scale(0.95) }
                        to { transform: var(--place, translate(0px, 0px)) translate(0px, 0px)
                                        scale(1) }
                    }";

/// The computed transform of `node`.
fn transform(harness: &Harness, node: NodeIndex) -> Transform {
    harness
        .document
        .node(node)
        .primary_style()
        .expect("the element is styled")
        .get_box()
        .clone_transform()
}

/// A styled document holding the surface and the reference, with the entrance run to its end.
fn settled() -> (Harness, NodeIndex, NodeIndex) {
    let mut harness = Harness::new();
    harness.add_author(LIGHT);
    let surface = harness.append(harness.root, "box");
    harness.set_classes(surface, &["surface"]);
    let reference = harness.append(harness.root, "box");
    harness.set_classes(reference, &["reference"]);
    harness.frame();
    harness.retire_all();
    for frame in 1..=16u32 {
        animation_frame(&mut harness, f64::from(frame) * FRAME);
    }
    assert_eq!(
        transform(&harness, surface),
        transform(&harness, reference),
        "the entrance finished away from its last keyframe"
    );
    (harness, surface, reference)
}

#[test]
fn a_filling_animation_keeps_its_last_keyframe_through_a_theme_sheet_swap() {
    let (mut harness, surface, reference) = settled();

    harness.replace(0, DARK);
    harness.frame();
    harness.retire_all();
    for frame in 17..=24u32 {
        animation_frame(&mut harness, f64::from(frame) * FRAME);
        assert_eq!(
            transform(&harness, surface),
            transform(&harness, reference),
            "the forward fill was dropped after the theme sheet changed, at frame {frame}"
        );
    }
}

#[test]
fn a_filling_animation_keeps_its_last_keyframe_through_every_later_match() {
    let (mut harness, surface, reference) = settled();

    // Each class change matches the element again, which reads the animation declarations anew.
    for (round, classes) in [&["surface", "busy"][..], &["surface"], &["surface", "busy"]]
        .into_iter()
        .enumerate()
    {
        harness.set_classes(surface, classes);
        harness.frame();
        harness.retire_all();
        assert_eq!(
            transform(&harness, surface),
            transform(&harness, reference),
            "the forward fill was dropped by match {round} after the animation finished"
        );
    }
}

#[test]
fn a_held_fill_reports_no_second_end_and_lets_the_loop_sleep() {
    let (mut harness, surface, _reference) = settled();

    harness.replace(0, DARK);
    harness.frame();
    harness.retire_all();
    harness.set_classes(surface, &["surface", "busy"]);
    harness.frame();
    harness.retire_all();

    let report = harness
        .engine
        .animation_tick(&harness.document, AnimationTime(17.0 * FRAME));
    assert!(
        report.edges.is_empty(),
        "the restyles after the end made the animation cross an edge again: {:?}",
        report.edges
    );
    assert!(
        report.elements.iter().all(|element| !element.advancing),
        "a finished fill asks the loop to come back for it: {:?}",
        report.elements
    );
}

/// The edges the animations of the document cross over the frames `frames`, as name and edge.
fn edges(harness: &mut Harness, frames: std::ops::RangeInclusive<u32>) -> Vec<(String, Lifecycle)> {
    let mut crossed = Vec::new();
    for frame in frames {
        let now = AnimationTime(f64::from(frame) * FRAME);
        let report = harness.engine.animation_tick(&harness.document, now);
        crossed.extend(
            report
                .edges
                .iter()
                .map(|edge: &AnimationEdge| (edge.name.clone(), edge.lifecycle)),
        );
        animation_frame(harness, f64::from(frame) * FRAME);
    }
    crossed
}

#[test]
fn a_held_fill_that_a_restyle_pauses_ends_once_and_resumes_finished() {
    let (mut harness, surface, reference) = settled();

    harness.set_classes(surface, &["surface", "paused"]);
    harness.frame();
    harness.retire_all();
    assert_eq!(edges(&mut harness, 17..=20), []);
    assert_eq!(transform(&harness, surface), transform(&harness, reference));

    harness.set_classes(surface, &["surface"]);
    harness.frame();
    harness.retire_all();
    assert_eq!(
        edges(&mut harness, 21..=30),
        [],
        "the resumed animation ended a second time"
    );
    assert_eq!(transform(&harness, surface), transform(&harness, reference));
}

#[test]
fn a_new_animation_name_on_a_held_fill_runs_its_own_course_once() {
    let (mut harness, surface, _reference) = settled();

    harness.set_classes(surface, &["surface", "other"]);
    harness.frame();
    harness.retire_all();
    let crossed = edges(&mut harness, 17..=40);
    assert!(
        crossed.iter().all(|(name, _)| name == "other"),
        "the old animation crossed an edge again: {crossed:?}"
    );
    let ends = crossed
        .iter()
        .filter(|(_, lifecycle)| *lifecycle == Lifecycle::Ended)
        .count();
    assert_eq!(ends, 1, "the new animation ends once: {crossed:?}");
}
