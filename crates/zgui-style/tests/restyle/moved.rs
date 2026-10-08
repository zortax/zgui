//! An element whose animation asked for a cascade, and that moved before the restyle ran.
//!
//! A portal mounts its content under an overlay root, and a view can take a subtree out and put it
//! back. When that happens between the animation tick and the restyle, the descent flags the mark
//! raised stand on the ancestors the element left. The animation-only traversal must still reach
//! the element where it is now, or the ordinary traversal meets an animation hint it refuses.

use zgui_dom::NodeIndex;

use crate::support::{Harness, width};

/// A width transition, which is the cascading tier: a box moves.
const SHEET: &str = "root { display: block }
                     .side { display: block }
                     .field { display: block; width: 100px; transition: width 400ms linear }
                     .field.wide { width: 300px }";

/// How long one frame is, in seconds.
const FRAME: f64 = 0.05;

/// A document of root > (left > field, right), with the field's width transition running.
fn document() -> (Harness, NodeIndex, NodeIndex) {
    let mut harness = Harness::new();
    harness.add_author(SHEET);
    let root = harness.root;
    let left = harness.append(root, "box");
    harness.set_classes(left, &["side"]);
    let right = harness.append(root, "box");
    harness.set_classes(right, &["side"]);
    let field = harness.append(left, "box");
    harness.set_classes(field, &["field"]);
    harness.frame();
    harness.retire_all();
    harness.set_classes(field, &["field", "wide"]);
    harness.frame();
    harness.retire_all();
    (harness, field, right)
}

/// The tick and the marks of one frame at `now`, without the restyle.
fn tick(harness: &mut Harness, now: f64) {
    let report = harness
        .engine
        .animation_tick(&harness.document, zgui_style::AnimationTime(now));
    for element in &report.elements {
        if element.properties.is_paint_only() || !(element.advancing || element.crossed) {
            continue;
        }
        harness
            .engine
            .mark_animation_restyle(&harness.document, element.index);
    }
}

/// The restyle that ends the frame.
fn restyle(harness: &mut Harness) {
    harness.engine.restyle(&mut harness.document, None);
    harness.retire_all();
}

#[test]
fn a_field_moved_to_another_parent_after_its_mark_is_cascaded_where_it_now_is() {
    let (mut harness, field, right) = document();
    tick(&mut harness, FRAME);
    restyle(&mut harness);
    let before = width(&harness, field);

    tick(&mut harness, 3.0 * FRAME);
    harness.edit(|edit| edit.insert_before(right, field, None));
    restyle(&mut harness);

    let after = width(&harness, field);
    assert!(
        after > before && after < 300.0,
        "the moved field took the transition's value for this frame: {before} then {after}"
    );
}

#[test]
fn a_field_taken_out_and_put_back_after_its_mark_is_cascaded() {
    let (mut harness, field, right) = document();
    tick(&mut harness, FRAME);
    restyle(&mut harness);
    let before = width(&harness, field);

    tick(&mut harness, 3.0 * FRAME);
    harness.edit(|edit| edit.remove(field));
    restyle(&mut harness);
    harness.edit(|edit| edit.insert_before(right, field, None));
    restyle(&mut harness);

    let after = width(&harness, field);
    assert!(
        after > before && after <= 300.0,
        "the field put back is styled from its transition: {before} then {after}"
    );
}

#[test]
fn a_field_taken_out_and_dropped_after_its_mark_is_forgotten() {
    let (mut harness, field, right) = document();
    tick(&mut harness, FRAME);
    restyle(&mut harness);

    tick(&mut harness, 3.0 * FRAME);
    harness.edit(|edit| edit.remove(field));
    restyle(&mut harness);
    assert_eq!(
        harness.engine.animation_waiting(),
        1,
        "the field waits out of the document"
    );

    // The frame ends with the field still out, so the document drops it. The next restyle, here
    // one a class change asks for, lets go of it.
    zgui_dom::arena::end_frame(&mut harness.document);
    tick(&mut harness, 4.0 * FRAME);
    harness.set_classes(right, &["field"]);
    restyle(&mut harness);
    assert_eq!(
        harness.engine.animation_waiting(),
        0,
        "a dropped field waits for nothing"
    );
}

#[test]
fn a_field_out_of_the_document_for_several_restyles_waits_and_is_cascaded_when_it_returns() {
    let (mut harness, field, right) = document();
    tick(&mut harness, FRAME);
    restyle(&mut harness);
    let before = width(&harness, field);

    tick(&mut harness, 2.0 * FRAME);
    harness.edit(|edit| edit.remove(field));
    for frame in 3..6u32 {
        restyle(&mut harness);
        tick(&mut harness, f64::from(frame) * FRAME);
        assert_eq!(
            harness.engine.animation_waiting(),
            1,
            "the field waits, once, while it is out"
        );
    }
    harness.edit(|edit| edit.insert_before(right, field, None));
    restyle(&mut harness);

    assert_eq!(harness.engine.animation_waiting(), 0);
    let after = width(&harness, field);
    assert!(
        after > before && after <= 300.0,
        "the field that returned is styled from its transition: {before} then {after}"
    );
}
