//! Counter measurements.
//!
//! **Every test in this file holds a [`Recording`] for as long as it is doing any work that moves a
//! counter, and that is a rule about the whole binary rather than about each test.** The counters
//! are one process-wide block: a test measuring in one thread reads whatever a test in another
//! thread is doing, so a measuring test can only be trusted in a binary where nothing runs
//! unguarded. Cargo runs test *targets* one at a time, so a target in which every test is guarded
//! is a target that measures what it says.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::CssPx;
use zgui_profile::{COUNTERS_ENABLED, Counter};
use zgui_render::Renderer;
use zgui_scene::PaintSlot;
use zgui_testkit_scene::CaptureRenderer;
use zgui_testkit_scene::counters::{Recording, is_meaningful};
use zgui_testkit_scene::{MonoLayout, MonoShaper};
use zgui_text::{
    BreakRequest, ParagraphCache, ParagraphContent, ParagraphShaper, StyledRun, TextMap, lay_out,
};
use zgui_text_style::{ParagraphStyle, TextStyle};

use crate::support::kitchen_sink;

#[test]
fn the_counters_are_compiled_into_this_build() {
    // Without this the whole file is vacuous: every counter would read zero and every assertion
    // below would hold while measuring nothing at all. It is a compile-time check because a
    // *runtime* one in a build with the counters off would be a test that fails rather than a
    // build that does.
    const { assert!(COUNTERS_ENABLED) };
}

#[test]
#[should_panic(expected = "reads zero under a renderer that submits no work")]
fn a_test_asserting_on_draw_calls_fails_loudly_rather_than_passing() {
    // `draw_calls == 0` is true under a capture renderer and means nothing, so a budget test that
    // reached for it would be green for ever while asserting nothing.
    let mut recording = Recording::begin();
    let mut renderer = CaptureRenderer::new();
    let measured = recording.measure(|| {
        renderer.draw(&kitchen_sink(), &DamageSet::full());
    });
    let _ = measured.get(Counter::DrawCalls);
}

#[test]
fn the_backend_neutral_counters_are_readable_and_say_what_they_mean() {
    // The control for the refusal above: the refusal is narrow rather than a blanket "nothing is
    // assertable". Building the scene emits primitives; drawing it emits none, because a renderer
    // does not build display lists.
    assert!(is_meaningful(Counter::PrimitivesEmitted));

    let mut recording = Recording::begin();
    let mut renderer = CaptureRenderer::new();
    let mut scene = None;

    let built = recording.measure(|| scene = Some(kitchen_sink()));
    let control = built.control(Counter::PrimitivesEmitted);
    assert!(control.value() > 10);

    let scene = scene.expect("the scene was built");
    let drawn = recording.measure(|| {
        renderer.draw(&scene, &DamageSet::full());
    });
    drawn.assert_zero(Counter::PrimitivesEmitted, &control);
}

#[test]
fn nothing_the_kitchen_sink_pushes_is_culled_by_its_clips() {
    let mut recording = Recording::begin();

    // The control: a primitive pushed wholly outside its clip is culled, so the counter can move.
    let culled = recording.measure(|| {
        let mut scene = zgui_scene::Scene::new();
        scene.begin_frame(zgui_geom::Size::new(64, 64));
        let fill = zgui_scene::PaintRef::solid(scene.paints.solid(zgui_color::Color::BLACK));
        let clip = scene.clips.only(zgui_scene::ClipLink::rect(support::rect(
            0.0, 0.0, 8.0, 8.0,
        )));
        assert!(
            scene
                .push_quad(
                    zgui_scene::Quad::filled(support::rect(100.0, 100.0, 8.0, 8.0), fill)
                        .clipped(clip)
                )
                .is_none()
        );
    });
    let control = culled.control(Counter::PrimitivesCulled);
    assert_eq!(control.value(), 1);

    let mut scene = None;
    let built = recording.measure(|| scene = Some(kitchen_sink()));
    built.assert_zero(Counter::PrimitivesCulled, &control);
    assert!(scene.is_some());
}

#[test]
fn shaping_is_counted_once_and_breaking_once_per_distinct_request() {
    let mut recording = Recording::begin();

    let text = "aaa bbb ccc";
    let map = TextMap::new();
    let runs = [StyledRun {
        text: 0..text.len(),
        style: Arc::new(TextStyle::initial()),
        brush: PaintSlot(0),
    }];
    let paragraph = ParagraphStyle::initial();
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[],
        paragraph: &paragraph,
        scale: 1.0,
    };

    let mut shaper = MonoShaper::new();
    let mut cache: ParagraphCache<MonoLayout> = ParagraphCache::new();

    let measured = recording.measure(|| {
        for width in [80.0, 40.0, 40.0, 40.0] {
            let request = BreakRequest::new(&content, Some(CssPx(width)));
            lay_out(&mut shaper, &mut cache, &content, &request);
        }
    });

    // The framework's counters and the shaper's own view of what it did have to agree: either alone
    // is satisfied by a shaper that reports a cache hit while performing a pass.
    measured.assert_exactly(Counter::TextShaped, 1);
    measured.assert_exactly(Counter::TextRebroken, 2);
    assert_eq!(shaper.shapes(), 1);
    assert_eq!(shaper.breaks(), 2);
}

#[test]
fn a_repeated_request_costs_no_pass_at_all() {
    let mut recording = Recording::begin();

    let text = "aaa bbb";
    let map = TextMap::new();
    let runs = [StyledRun {
        text: 0..text.len(),
        style: Arc::new(TextStyle::initial()),
        brush: PaintSlot(0),
    }];
    let paragraph = ParagraphStyle::initial();
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[],
        paragraph: &paragraph,
        scale: 1.0,
    };
    let request = BreakRequest::new(&content, Some(CssPx(40.0)));

    let mut shaper = MonoShaper::new();
    let mut cache: ParagraphCache<MonoLayout> = ParagraphCache::new();

    // The control: the first lay-out does shape and does break.
    let first = recording.measure(|| {
        lay_out(&mut shaper, &mut cache, &content, &request);
    });
    let shaped = first.control(Counter::TextShaped);
    let broken = first.control(Counter::TextRebroken);

    let repeated = recording.measure(|| {
        for _ in 0..16 {
            lay_out(&mut shaper, &mut cache, &content, &request);
        }
    });
    repeated.assert_zero(Counter::TextShaped, &shaped);
    repeated.assert_zero(Counter::TextRebroken, &broken);
}

#[test]
fn a_strut_costs_no_shaping_pass() {
    let mut recording = Recording::begin();
    let mut shaper = MonoShaper::new();
    let mut scene = None;

    let control = recording
        .measure(|| scene = Some(kitchen_sink()))
        .control(Counter::PrimitivesEmitted);
    assert!(scene.is_some());

    let measured = recording.measure(|| {
        for _ in 0..100 {
            shaper.strut(&TextStyle::initial());
        }
    });
    // A block establishes a strut whether or not it holds any text, so asking for one must not be a
    // shaping pass — and must not be a primitive either.
    assert_eq!(measured.counters().text_shaped, 0);
    measured.assert_zero(Counter::PrimitivesEmitted, &control);
}
