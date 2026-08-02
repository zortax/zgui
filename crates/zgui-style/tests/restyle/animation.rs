//! The animation-only traversal: whether it runs, what it reaches, and what it leaves behind.
//!
//! An animation whose values a repaint cannot express is serviced by a second, separate descent
//! that no other input asks for. Every failure it has is silent from everywhere else: the clock
//! advances, the animations tick, the values are sampled and reported, the counters count — and
//! the element is never styled again, so the screen holds the frame the transition started at. So
//! the cases here drive the tick the way a frame does, and then assert on the *computed style*
//! the cascade produced, which is the one thing that is false when the traversal did not run.

use zgui_dom::NodeIndex;

use crate::support::{Harness, animation_frame};

/// A colour transition on a property every descendant inherits.
const SHEET: &str = "root { display: block; color: rgb(0, 0, 0) }
                     .btn { display: block; color: rgb(0, 0, 0);
                            transition: color 400ms linear }
                     .btn.hot { color: rgb(200, 200, 200) }
                     .label { display: block }";

/// The red channel of `node`'s computed colour, in eighths of a percent so it compares exactly.
fn colour(harness: &Harness, node: NodeIndex) -> u32 {
    let style = harness
        .document
        .node(node)
        .primary_style()
        .expect("the element is styled");
    let colour = style.get_inherited_text().clone_color();
    (colour.components.0 * 10_000.0) as u32
}

/// A document of root > button > label, styled, with the button about to transition.
fn document() -> (Harness, NodeIndex, NodeIndex) {
    let mut harness = Harness::new();
    harness.add_author(SHEET);
    let root = harness.root;
    let button = harness.append(root, "box");
    harness.set_classes(button, &["btn"]);
    let label = harness.append(button, "box");
    harness.set_classes(label, &["label"]);
    harness.frame();
    harness.retire_all();
    (harness, button, label)
}

#[test]
fn a_transition_on_an_inherited_colour_is_cascaded_frame_by_frame() {
    let (mut harness, button, label) = document();
    let start = colour(&harness, button);
    assert_eq!(
        colour(&harness, label),
        start,
        "the label does not inherit the button's colour, so this proves nothing about descendants"
    );

    // A class change, which is what starts the transition: the cascade compares the two results
    // and creates it. Nothing here reaches into the engine's animation table.
    harness.set_classes(button, &["btn", "hot"]);
    harness.frame();
    harness.retire_all();

    let mut passes = 0;
    let mut moved = Vec::new();
    for frame in 1..=24u32 {
        let now = f64::from(frame) * 0.016;
        let pass = animation_frame(&mut harness, now);
        passes += u32::from(pass.animation_pass);
        let on_button = colour(&harness, button);
        assert_eq!(
            colour(&harness, label),
            on_button,
            "at {now}s the label's inherited colour is not the button's: the animation-only \
             traversal styled the element and not what inherits from it"
        );
        moved.push(on_button);
    }

    assert!(
        passes >= 20,
        "the animation-only traversal ran on {passes} of 24 frames, so the hint that only it \
         processes was left outstanding for the rest"
    );
    assert!(
        moved.windows(2).all(|pair| pair[1] >= pair[0]),
        "the cascaded colour did not move towards the destination: {moved:?}"
    );
    assert!(
        moved.last().copied().unwrap_or_default() > start,
        "the colour never left where it started: {moved:?}"
    );
    assert!(
        *moved.last().expect("frames ran") == colour(&harness, button),
        "the last frame's reading is not the current one"
    );

    // Past the transition's 400ms. The end is the frame nothing is left to interpolate on, and it
    // is the one whose cascade decides what the element keeps: unrun, the element holds the last
    // value that was interpolated into it — a few per cent short of the destination — for the whole
    // of the rest of its life, and every hover in the application settles on the wrong colour.
    for frame in 25..=28u32 {
        animation_frame(&mut harness, f64::from(frame) * 0.016);
    }
    let destination = 7843;
    assert_eq!(
        colour(&harness, button),
        destination,
        "the transition is over and the element did not arrive at the colour it was going to"
    );
    assert_eq!(
        colour(&harness, label),
        destination,
        "the element arrived and what inherits from it did not"
    );

    // And it is over: an element that keeps asking for the animation traversal after its last
    // animation ended keeps the loop awake for ever over a value that will never move again.
    let after = animation_frame(&mut harness, 0.5);
    assert!(
        !after.animation_pass,
        "the animation-only traversal is still running a frame after everything ended"
    );
}

#[test]
fn the_animation_traversal_is_not_run_on_a_frame_that_owes_it_nothing() {
    let (mut harness, _button, _label) = document();
    // Nothing is animating, so the tick reports nothing, nothing is marked, and the second descent
    // must not happen: it is not free, and a document at rest pays for it on every frame.
    let pass = animation_frame(&mut harness, 0.016);
    assert!(
        !pass.animation_pass,
        "the animation-only traversal ran over a document with no animation in it"
    );
}

#[test]
fn the_descent_flag_does_not_survive_the_traversal_that_read_it() {
    let (mut harness, button, _label) = document();
    harness.set_classes(button, &["btn", "hot"]);
    harness.frame();
    harness.retire_all();
    animation_frame(&mut harness, 0.016);

    let root = harness.root;
    assert!(
        !harness.document.node(root).has_animation_work_below(),
        "the flag that takes the animation traversal below the root is still raised, so every \
         later frame descends into a subtree with nothing in it to do"
    );
}
