//! The claim the whole crate exists to make: a width change costs a break and not a shape.
//!
//! Every assertion here is on the framework's own counters rather than on a stopwatch, because the
//! claim is about how much work was done and not about how fast the machine is.

mod support;

use std::sync::{Mutex, MutexGuard};

use support::scene::Scene;
use zgui_geom::CssPx;
use zgui_profile::{COUNTERS_ENABLED, Counter, counter};
use zgui_scene::PaintSlot;
use zgui_text::{BreakRequest, InlineBoxGeometry, ParagraphKey, TextMap};
use zgui_text_style::{LengthPercent, TextAlign, TextStyle};

/// The counters are one global block, so the tests that read them take turns.
static COUNTERS: Mutex<()> = Mutex::new(());

/// Zeroes the counters and holds the lock for the duration of one test.
fn counting() -> MutexGuard<'static, ()> {
    let guard = COUNTERS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    counter::reset();
    guard
}

/// A sentence long enough to break several times.
fn lorem() -> String {
    "the quick brown fox jumps over the lazy dog and then keeps on running for a while".to_owned()
}

/// Forty widths over one paragraph shape it once.
#[test]
fn forty_width_changes_shape_once_and_break_forty_times() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());

    for step in 0..40u32 {
        scene.run(Some(CssPx(
            200.0 + f32::from(u16::try_from(step).unwrap()) * 5.0,
        )));
    }

    assert_eq!(scene.shaper.shapes, 1, "one shape for forty widths");
    assert_eq!(scene.shaper.breaks, 40, "one break per width");
    if COUNTERS_ENABLED {
        assert_eq!(counter::get(Counter::TextShaped), 1);
        assert_eq!(counter::get(Counter::TextRebroken), 40);
    }
}

/// Breaking again at a width already broken at costs nothing at all.
#[test]
fn re_breaking_at_the_same_width_is_free() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());

    scene.run(Some(CssPx(300.0)));
    let after_first = scene.shaper.breaks;
    let first = scene.run(Some(CssPx(300.0)));
    let second = scene.run(Some(CssPx(300.0)));

    assert_eq!(scene.shaper.breaks, after_first, "no further breaking pass");
    assert_eq!(first, second, "and the same result is reported");
    if COUNTERS_ENABLED {
        assert_eq!(counter::get(Counter::TextRebroken), 1);
    }

    // The counters above say a pass was skipped; this says the result is entitled to skip it. The
    // glyphs must record the very key the request carries, because that equality is the whole test
    // `begin_break` applies — a recorded `None`, or a stale key, would make the cheap answer a
    // wrong one rather than a free one.
    let content = scene.content();
    let request = BreakRequest::new(&content, Some(CssPx(300.0)));
    let shaped = scene
        .cache
        .get(ParagraphKey::of(&content))
        .expect("the paragraph is held");
    assert_eq!(
        shaped.broken_as(),
        Some(request.key()),
        "the glyphs record exactly the break the skipped request asked for"
    );

    // And the control: a width the paragraph has never been broken at is *not* what it records, so
    // the assertion above can fail.
    let elsewhere = BreakRequest::new(&content, Some(CssPx(180.0)));
    assert_ne!(
        shaped.broken_as(),
        Some(elsewhere.key()),
        "a width never broken at must not read as already broken"
    );
}

/// A brush change is a paint change: neither the key nor the shape moves.
#[test]
fn re_theming_never_reshapes() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());
    scene.run(Some(CssPx(300.0)));

    let before = ParagraphKey::of(&scene.content());
    scene.runs[0].brush = PaintSlot(7);
    let after = ParagraphKey::of(&scene.content());
    assert_eq!(
        before, after,
        "a brush change must not move the shaping key"
    );

    scene.run(Some(CssPx(300.0)));
    assert_eq!(scene.shaper.shapes, 1);
    assert_eq!(scene.shaper.breaks, 1, "and must not even cost a break");
}

/// Changing the font size is a shaping change, and costs exactly one shape.
#[test]
fn a_font_size_change_reshapes_exactly_once() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());
    scene.run(Some(CssPx(300.0)));

    let mut restyled = TextStyle::initial();
    restyled.size = CssPx(18.0);
    scene.runs[0].style = std::sync::Arc::new(restyled);
    scene.run(Some(CssPx(300.0)));

    assert_eq!(scene.shaper.shapes, 2);
    if COUNTERS_ENABLED {
        assert_eq!(counter::get(Counter::TextShaped), 2);
    }
}

/// Alignment costs a break and no shape, and the break actually moves the lines.
#[test]
fn alignment_costs_a_break_and_moves_the_lines() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());
    let start = scene.run(Some(CssPx(300.0)));

    scene.paragraph.align = TextAlign::Center;
    let centred = scene.run(Some(CssPx(300.0)));

    assert_eq!(scene.shaper.shapes, 1, "alignment must not re-shape");
    assert_eq!(scene.shaper.breaks, 2, "but it must cost a break");
    assert_ne!(
        start.geometry.lines[0].offset, centred.geometry.lines[0].offset,
        "and the line must actually have moved",
    );
}

/// The measured shape of the whole design: re-styling `vertical-align` against a warm cache moves
/// the box.
///
/// The shift is baked into the height the shaper was told, so nothing in the shaped glyphs can
/// notice it changed. If the request did not carry it, this would be a silent no-op — the box would
/// stay exactly where it was, with no error anywhere.
#[test]
fn a_vertical_align_restyle_against_a_warm_cache_moves_the_box() {
    let _guard = counting();
    // A tall strut and a small box, so that raising the box moves it *within* the line rather than
    // making the line taller — which is the case a stale shift is invisible in.
    let mut style = TextStyle::initial();
    style.size = CssPx(40.0);
    let mut scene = Scene::plain("an image here", style);
    scene.text.insert(2, '\u{fffc}');
    scene.runs[0].text = 0..scene.text.len();
    scene.boxes.push(InlineBoxGeometry {
        id: 1,
        offset: 2,
        width: CssPx(20.0),
        height: CssPx(20.0),
        ascent: CssPx(20.0),
        shift: CssPx::ZERO,
    });

    let on_baseline = scene.run(Some(CssPx(400.0)));
    let shapes = scene.shaper.shapes;

    // Only the shift changes — the box is the same size and the text is untouched.
    scene.boxes[0].shift = CssPx(6.0);
    let raised = scene.run(Some(CssPx(400.0)));

    assert_eq!(scene.shaper.shapes, shapes, "a shift must not re-shape");
    assert_eq!(scene.shaper.breaks, 2, "but it must cost a break");
    assert!(
        raised.boxes[0].origin.y.0 < on_baseline.boxes[0].origin.y.0 - 5.0,
        "the box must actually have been raised: {:?} then {:?}",
        on_baseline.boxes[0].origin.y,
        raised.boxes[0].origin.y,
    );
    assert_eq!(
        raised.geometry.lines[0].height, on_baseline.geometry.lines[0].height,
        "and the line must be unchanged, which is what makes a stale shift invisible",
    );
}

/// An atomic inline resizing under a different constraint invalidates the break without re-shaping.
#[test]
fn an_inline_box_resizing_costs_a_break_and_no_shape() {
    let _guard = counting();
    let mut scene = Scene::plain("a\u{fffc}b", TextStyle::initial());
    scene.boxes.push(InlineBoxGeometry {
        id: 1,
        offset: 1,
        width: CssPx(20.0),
        height: CssPx(20.0),
        ascent: CssPx(20.0),
        shift: CssPx::ZERO,
    });
    let narrow = scene.run(Some(CssPx(400.0)));

    scene.boxes[0].width = CssPx(60.0);
    let wide = scene.run(Some(CssPx(400.0)));

    assert_eq!(scene.shaper.shapes, 1);
    assert_eq!(scene.shaper.breaks, 2);
    assert!(wide.geometry.lines[0].width.0 > narrow.geometry.lines[0].width.0);
}

/// A percentage indent resolves against the width being proposed, so two widths give two indents.
#[test]
fn a_percentage_indent_resolves_against_the_proposed_width() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());
    scene.paragraph.indent.length = LengthPercent {
        length: CssPx::ZERO,
        percent: 0.25,
    };

    let narrow = scene.run(Some(CssPx(200.0)));
    let wide = scene.run(Some(CssPx(400.0)));

    assert_eq!(scene.shaper.shapes, 1);
    assert!(wide.geometry.lines[0].offset.0 > narrow.geometry.lines[0].offset.0);
}

/// Two paragraphs whose generated strings match but whose *sources* differ do not share a shape.
///
/// A shaped result carries the map back to the source, so sharing one across two paragraphs that
/// were generated differently would serve the second the first one's provenance — and collapsing
/// leading white space is enough to produce exactly that pair. The failure is silent: every caret,
/// selection and hit test in the second paragraph lands at the wrong offset, with nothing to report.
#[test]
fn paragraphs_generated_from_different_sources_do_not_share_a_shape() {
    let _guard = counting();
    let mut plain = Scene::plain("hello", TextStyle::initial());

    // The same five characters, but the source had two leading spaces that collapsed away.
    let mut indented = Scene::plain("hello", TextStyle::initial());
    let mut map = TextMap::new();
    map.push(0..5, 0, 2);
    indented.map = map;

    assert_ne!(plain.map, indented.map, "the two maps genuinely differ");
    assert_ne!(
        ParagraphKey::of(&plain.content()),
        ParagraphKey::of(&indented.content()),
        "and the key has to notice",
    );

    plain.run(Some(CssPx(300.0)));
    let served = plain
        .cache
        .get(ParagraphKey::of(&indented.content()))
        .map(|shaped| shaped.map().clone());
    assert_eq!(
        served, None,
        "the indented paragraph must not be answered with the plain one's map",
    );
}

/// Two paragraphs with the same text and styles share one shaped result.
#[test]
fn identical_paragraphs_share_one_shape() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());
    scene.run(Some(CssPx(300.0)));
    let key = ParagraphKey::of(&scene.content());

    assert!(scene.cache.holds(key));
    assert_eq!(scene.cache.len(), 1);

    // The same content again finds the entry rather than shaping.
    scene.run(Some(CssPx(250.0)));
    assert_eq!(scene.cache.len(), 1);
    assert_eq!(scene.shaper.shapes, 1);
}

/// A layout algorithm cycling three candidate widths breaks three times, not three per round.
///
/// This is the defect the remembered passes exist for. Intrinsic sizing asks a paragraph how narrow
/// it can be, then how wide it wants to be, then how tall it is at the width it has been given, and
/// then does the whole of that again on the next iteration of whatever it is resolving. With one
/// recorded pass each of the three evicts the one before, so every round costs three breaks — and a
/// document with a paragraph inside every panel of a grid pays that on every keystroke.
#[test]
fn cycling_three_candidate_widths_costs_three_breaks_and_not_three_per_round() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());

    // The kept pass first, so that the two probes are the only thing the rounds add.
    scene.run(Some(CssPx(320.0)));
    for _ in 0..12 {
        scene.probe(Some(CssPx(90.0)));
        scene.probe(None);
        scene.probe(Some(CssPx(320.0)));
    }

    assert_eq!(scene.shaper.shapes, 1);
    assert_eq!(
        scene.shaper.breaks, 3,
        "one per distinct width, however many rounds asked for them"
    );
    if COUNTERS_ENABLED {
        assert_eq!(counter::get(Counter::TextRebroken), 3);
    }
}

/// A probe answered from a remembered pass answers exactly what breaking again would have.
#[test]
fn a_remembered_pass_answers_what_a_fresh_break_answers() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());

    let fresh = scene.run(Some(CssPx(140.0)));
    // Move the glyphs to another width, so the answer below cannot come from what they reflect.
    scene.run(Some(CssPx(400.0)));
    let breaks = scene.shaper.breaks;

    let recalled = scene.probe(Some(CssPx(140.0)));
    assert_eq!(scene.shaper.breaks, breaks, "no pass was taken");
    assert_eq!(
        recalled.geometry, fresh.geometry,
        "a remembered pass is the pass, or it is a wrong measurement"
    );
    assert_eq!(recalled.boxes, fresh.boxes);
}

/// A pass whose answer will be kept always moves the glyphs, even at a width already measured.
///
/// The shaper's laid-out form holds one break at a time and it is what the paragraph's glyphs are
/// read out of when it is painted. Serving the kept pass from a remembered measurement would leave
/// that form reflecting some other width, and the lines drawn would be the lines of whichever probe
/// happened to run last.
#[test]
fn a_kept_pass_at_a_remembered_width_still_moves_the_glyphs() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());

    let narrow = scene.run(Some(CssPx(140.0))).geometry.lines.len();
    let key = ParagraphKey::of(&scene.content());
    let at_narrow = scene.cache.get(key).expect("cached").broken_as();
    let wide = scene.run(Some(CssPx(600.0))).geometry.lines.len();
    let at_wide = scene.cache.get(key).expect("cached").broken_as();
    assert_ne!(narrow, wide, "the two widths have to break differently");
    assert_ne!(at_narrow, at_wide);

    let breaks = scene.shaper.breaks;
    scene.probe(Some(CssPx(140.0)));
    assert_eq!(scene.shaper.breaks, breaks, "a probe takes no pass");
    assert_eq!(
        scene.cache.get(key).expect("cached").broken_as(),
        at_wide,
        "and leaves the glyphs reflecting the width they were last laid out at"
    );

    scene.run(Some(CssPx(140.0)));
    assert_eq!(scene.shaper.breaks, breaks + 1, "the kept pass broke");
    assert_eq!(
        scene.cache.get(key).expect("cached").broken_as(),
        at_narrow,
        "and the glyphs now reflect the width they will be drawn at"
    );
}

/// The remembered passes are bounded, so a long drag cannot grow a paragraph without limit.
#[test]
fn what_one_paragraph_remembers_is_bounded() {
    let _guard = counting();
    let mut scene = Scene::plain(&lorem(), TextStyle::initial());

    for step in 0..64u32 {
        scene.run(Some(CssPx(120.0 + f32::from(u16::try_from(step).unwrap()))));
    }
    let key = ParagraphKey::of(&scene.content());
    let held = scene.cache.get(key).expect("the paragraph is cached");
    assert!(
        held.remembered() <= 4,
        "sixty-four widths left {} remembered",
        held.remembered()
    );

    // And the oldest is the one that went: the four most recent widths are still free.
    let breaks = scene.shaper.breaks;
    for step in 60..64u32 {
        scene.probe(Some(CssPx(120.0 + f32::from(u16::try_from(step).unwrap()))));
    }
    assert_eq!(scene.shaper.breaks, breaks, "the four most recent are held");
    scene.probe(Some(CssPx(121.0)));
    assert_eq!(scene.shaper.breaks, breaks + 1, "the oldest was dropped");
}
