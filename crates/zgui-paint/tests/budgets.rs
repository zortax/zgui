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

#[test]
fn resizing_paints_nothing_wrappers_replays_without_reencoding_and_matches_fresh_scene() {
    let mut recording = Recording::begin();
    let tree = Element::new("root").children(
        (0..30)
            .map(|_| {
                Element::new("row")
                    .classes(&["wrap"])
                    .children(vec![Element::new("span").classes(&["chip"])])
            })
            .collect(),
    );
    let css = "root { display: block }
               .wrap { display: block; height: 24px }
               .chip { display: block; width: 16px; height: 16px; background: #268 }";
    let mut harness = Harness::sized(tree, css, 300.0, 800.0);

    let first = recording.measure(|| {
        harness.paint_everything();
    });
    let control = first.control(Counter::ChunksReencoded);
    assert!(
        first.get(Counter::ChunksReencoded) >= 61,
        "root, thirty wrappers and thirty chips are encoded on the first frame"
    );

    // Widen the viewport so `root` and every `.wrap` box resize from 300px to 420px while
    // keeping `PAINTS_NOTHING`, and every `.chip` keeps its 16x16 size.
    harness.viewport = zgui_geom::Size::new(420, 800);
    harness.compose(420.0, 800.0);

    let resized = recording.measure(|| {
        harness.paint_everything();
    });
    resized.assert_zero(Counter::ChunksReencoded, &control);
    assert!(
        resized.get(Counter::ChunksTranslated) >= 61,
        "every empty wrapper and unchanged chip replayed: {}",
        resized.get(Counter::ChunksTranslated)
    );

    // Scene output must match a cold painter encoding the resized fragment tree from scratch.
    let mut fresh_scene = zgui_scene::Scene::new();
    fresh_scene.begin_frame(harness.viewport);
    let placements = zgui_scene::Placements::of(&harness.scene.spatial);
    let full = zgui_bits::DamageSet::full();
    let mut input = zgui_paint::PaintInput::new(&harness.store, &full);
    input.placements = Some(&placements);
    let mut fresh_painter = zgui_paint::Painter::new();
    let fresh_report = fresh_painter.emit(&input, &mut fresh_scene);
    fresh_scene.finish(&full);

    assert_eq!(fresh_report.primitives, 30);
    assert_eq!(
        harness.scene.primitives.quads.len(),
        fresh_scene.primitives.quads.len()
    );
    for (replayed, encoded) in harness
        .scene
        .primitives
        .quads
        .iter()
        .zip(&fresh_scene.primitives.quads)
    {
        assert_eq!(replayed.bounds, encoded.bounds);
        assert_eq!(replayed.order, encoded.order);
    }
}

#[test]
fn resizing_painted_boxes_or_boxes_that_start_painting_reencodes_at_new_size() {
    let mut recording = Recording::begin();
    let tree = Element::new("root").children(vec![
        Element::new("bg").classes(&["bg"]),
        Element::new("bordered").classes(&["bordered"]),
        Element::new("shadowed").classes(&["shadowed"]),
        Element::new("outlined").classes(&["outlined"]),
        Element::new("awakened").classes(&["plain"]),
    ]);
    let css = "root { display: block }
               .bg { display: block; height: 20px; background: #246 }
               .bordered { display: block; height: 20px; border: 2px solid #333 }
               .shadowed { display: block; height: 20px; box-shadow: 0 2px 6px #000 }
               .outlined { display: block; height: 20px; outline: 2px solid #090 }
               .plain { display: block; height: 20px }
               .lit { display: block; height: 20px; background: #e33 }";
    let mut harness = Harness::sized(tree, css, 300.0, 400.0);
    harness.paint_everything();

    let before_fragments: Vec<_> = ["root", "bg", "bordered", "shadowed", "outlined", "awakened"]
        .map(|name| harness.fragment_of(name))
        .into();

    // Flip `awakened` from `PAINTS_NOTHING` (`.plain`) to painting (`.lit`) in place so every
    // box keeps its `FragKey` and recorded `PaintCache` entry while resizing from 300px to 440px.
    let target = harness.element("awakened");
    harness.edit_and_restyle(|edit| {
        edit.set_classes(target, &[zgui_interned::ClassName::new("lit")]);
    });
    zgui_layout::boxtree::patch::style::restyle(&mut harness.store, &harness.document, [target]);
    harness.viewport = zgui_geom::Size::new(440, 400);
    harness.compose(440.0, 400.0);

    let after_fragments: Vec<_> = ["root", "bg", "bordered", "shadowed", "outlined", "awakened"]
        .map(|name| harness.fragment_of(name))
        .into();
    assert_eq!(
        before_fragments, after_fragments,
        "patching styles in place keeps every FragKey so PaintCache::reuse tests existing records"
    );

    let resized = recording.measure(|| {
        harness.paint_everything();
    });
    // `.bg`, `.bordered`, `.shadowed`, `.outlined`, and `awakened` (`.lit`) all re-encode at
    // 440px; `.outlined`'s own box fragment has `PAINTS_NOTHING == false` (outline is in ink),
    // so none of the five may take the empty-record bypass.
    assert_eq!(
        resized.get(Counter::ChunksReencoded),
        5,
        "all four painted boxes and the box that started painting re-encoded on resize, while \
         only `root` replayed its empty chunk"
    );
    assert_eq!(resized.get(Counter::ChunksTranslated), 1);

    // Every resized decoration spans the new 440px width, matching a cold painter.
    let mut fresh_scene = zgui_scene::Scene::new();
    fresh_scene.begin_frame(harness.viewport);
    let placements = zgui_scene::Placements::of(&harness.scene.spatial);
    let full = zgui_bits::DamageSet::full();
    let mut input = zgui_paint::PaintInput::new(&harness.store, &full);
    input.placements = Some(&placements);
    let mut fresh_painter = zgui_paint::Painter::new();
    fresh_painter.emit(&input, &mut fresh_scene);
    fresh_scene.finish(&full);

    assert_eq!(
        harness.scene.primitives.quads.len(),
        fresh_scene.primitives.quads.len()
    );
    assert_eq!(
        harness.scene.primitives.shadows.len(),
        fresh_scene.primitives.shadows.len()
    );
    for (got, expected) in harness
        .scene
        .primitives
        .quads
        .iter()
        .zip(&fresh_scene.primitives.quads)
    {
        assert_eq!(got.bounds, expected.bounds);
    }
    for (got, expected) in harness
        .scene
        .primitives
        .shadows
        .iter()
        .zip(&fresh_scene.primitives.shadows)
    {
        assert_eq!(got.bounds, expected.bounds);
    }
}
