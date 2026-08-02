//! The counter budgets, in a binary of their own.
//!
//! The frame counters are one block of atomics shared by the whole process, so a measurement taken
//! while another test is painting measures that test too. Every test here therefore holds the
//! recording, which serialises them against each other — and they are in a separate binary from the
//! structural assertions, which paint without taking one.

mod support;

use support::{Element, Harness};
use zgui_profile::Counter;
use zgui_testkit_scene::counters::Recording;

/// A root with `count` identically classed children.
fn rows(count: usize) -> Element {
    Element::new("root").children(
        (0..count)
            .map(|_| Element::new("li").classes(&["btn"]))
            .collect(),
    )
}

#[test]
fn a_thousand_identical_buttons_lower_a_handful_of_paint_styles() {
    // The claim the whole lowering cache exists to make. The custom property is declared on an
    // *ancestor* rather than per button on purpose: an element declaring its own gets a fresh
    // identity whenever it restyles, so a per-button declaration would measure that instead.
    let mut recording = Recording::begin();
    let mut harness = Harness::sized(
        rows(1000),
        "root { display: block; width: 300px; --theme: #123456 }
         .btn { display: block; height: 20px; background: #eee; border: 1px solid #333 }",
        300.0,
        20_000.0,
    );

    let measurement = recording.measure(|| {
        harness.paint_everything();
    });
    let lowered = measurement.get(Counter::StylesLowered);
    assert!(
        lowered <= 5,
        "a thousand identically styled buttons lowered {lowered} paint styles"
    );
    assert!(
        lowered >= 1,
        "nothing was lowered at all, so the bound above separates nothing"
    );
    assert!(
        measurement.get(Counter::StylesLoweredFromCache) > 900,
        "every button after the first has to come out of the cache"
    );
    assert_eq!(
        harness.painter.styles().len(),
        lowered as usize,
        "the counter and the table it counts have to agree, or one of them is measuring something \
         else"
    );
}

#[test]
fn an_unchanged_document_replays_its_operations_instead_of_re_encoding_them() {
    // What the two chunk counters measure, and what an unchanged or scrolled frame is claimed to
    // cost. Without a producer for them the assertion below could never fail.
    let mut recording = Recording::begin();
    let mut harness = Harness::sized(
        rows(40),
        "root { display: block; width: 300px }
         .btn { display: block; height: 20px; background: #eee }",
        300.0,
        800.0,
    );
    let first = recording.measure(|| {
        harness.paint_everything();
    });
    let control = first.control(Counter::ChunksReencoded);
    assert!(
        first.get(Counter::ChunksReencoded) > 40,
        "the first frame has to encode everything, or the second measures nothing"
    );

    let second = recording.measure(|| {
        harness.paint_everything();
    });
    second.assert_zero(Counter::ChunksReencoded, &control);
    assert!(
        second.get(Counter::ChunksTranslated) > 40,
        "and it has to have replayed them: {} replays",
        second.get(Counter::ChunksTranslated)
    );
}

#[test]
fn a_restyle_re_encodes_the_fragment_it_changed_and_replays_the_rest() {
    let mut recording = Recording::begin();
    let mut harness = Harness::sized(
        rows(40),
        "root { display: block; width: 300px }
         .btn { display: block; height: 20px; background: #eee }
         .hot { background: #f00 }",
        300.0,
        800.0,
    );
    harness.paint_everything();
    harness.paint_everything();

    let target = harness.element("li");
    harness.edit_and_restyle(|batch| {
        batch.set_classes(
            target,
            &[
                zgui_interned::ClassName::new("btn"),
                zgui_interned::ClassName::new("hot"),
            ],
        );
    });
    harness.rebuild(300.0, 800.0);

    let measurement = recording.measure(|| {
        harness.paint_everything();
    });
    assert!(
        measurement.get(Counter::ChunksReencoded) >= 1,
        "the recoloured row has to be encoded again rather than replayed in its old colour"
    );
}

#[test]
fn a_translucent_row_of_disjoint_children_allocates_no_group_target() {
    let mut recording = Recording::begin();
    let mut harness = Harness::sized(
        rows(50),
        "root { display: block; width: 300px }
         .btn { display: block; height: 20px; opacity: 0.5; background: #eee }",
        300.0,
        1200.0,
    );
    let measurement = recording.measure(|| {
        harness.paint_everything();
    });
    assert_eq!(
        measurement.get(Counter::GroupTargets),
        0,
        "fifty non-overlapping rows folded their alpha rather than taking fifty targets"
    );
    assert!(
        measurement.get(Counter::PrimitivesEmitted) >= 50,
        "and they were drawn, rather than skipped into a zero that looks like a win"
    );
}

#[test]
fn a_filter_allocates_a_group_target_whatever_the_geometry_says() {
    let mut recording = Recording::begin();
    let mut harness = Harness::new(
        Element::new("root").children(vec![Element::new("card")]),
        "root { display: block; width: 300px }
         card { display: block; height: 60px; filter: blur(4px); background: #eee }",
    );
    let measurement = recording.measure(|| {
        harness.paint_everything();
    });
    assert!(
        measurement.get(Counter::GroupTargets) >= 1,
        "a blur has to be composited into a target of its own"
    );
}
