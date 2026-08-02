//! The two-key split, determinism, and the atomic inlines.

mod support;

use std::sync::{Arc, Mutex, MutexGuard};

use support::Fixture;
use zgui_geom::CssPx;
use zgui_profile::{Counter, counter};
use zgui_scene::PaintSlot;
use zgui_text::{
    BreakRequest, InlineBoxGeometry, ParagraphCache, ParagraphContent, ParagraphShaper, StyledRun,
    TextMap, lay_out,
};
use zgui_text_parley::Controls;
use zgui_text_style::{Direction, ParagraphStyle};

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

/// A paragraph long enough to break several times.
const PROSE: &str = "the quick brown fox jumps over the lazy dog while the dog sleeps on";

/// Two runs of the same content and style shape to the same advances.
///
/// Nothing here depends on which faces the machine happens to have: the collection holds the two
/// faces these tests ship and nothing else, which is what makes the numbers a property of the code
/// rather than of the runner.
#[test]
fn shaping_is_deterministic() {
    let _guard = counting();
    let advances = |()| {
        let (_fonts, mut shaper) = support::shaper(Controls::Mark);
        let fixture = Fixture::new(PROSE, Direction::LeftToRight);
        let content = fixture.content();
        let mut shaped = shaper.shape(&content);
        let broken = shaper.break_lines(
            &mut shaped,
            &BreakRequest::new(&content, Some(CssPx(200.0))),
        );
        let widths: Vec<u32> = broken
            .geometry
            .lines
            .iter()
            .map(|line| line.width.0.to_bits())
            .collect();
        (widths, shaped.content_widths())
    };
    let (first, first_widths) = advances(());
    let (second, second_widths) = advances(());
    assert!(!first.is_empty());
    assert_eq!(first, second);
    assert_eq!(first_widths, second_widths);
}

/// A width change costs a breaking pass and never a shaping pass.
#[test]
fn a_width_change_costs_a_break_and_not_a_shape() {
    let _guard = counting();
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let mut cache = ParagraphCache::new();
    let fixture = Fixture::new(PROSE, Direction::LeftToRight);
    let content = fixture.content();

    counter::reset();
    for width in [400.0, 320.0, 260.0, 200.0, 160.0] {
        let request = BreakRequest::new(&content, Some(CssPx(width)));
        let (_shaped, broken) = lay_out(&mut shaper, &mut cache, &content, &request);
        assert!(!broken.geometry.lines.is_empty());
    }
    assert_eq!(counter::get(Counter::TextShaped), 1, "one shape in total");
    assert_eq!(
        counter::get(Counter::TextRebroken),
        5,
        "and one breaking pass per distinct width"
    );

    // Asking the same width again costs neither.
    let request = BreakRequest::new(&content, Some(CssPx(160.0)));
    lay_out(&mut shaper, &mut cache, &content, &request);
    assert_eq!(counter::get(Counter::TextShaped), 1);
    assert_eq!(counter::get(Counter::TextRebroken), 5);
}

/// The lines a re-break produces are the lines a fresh shape at that width would have produced.
///
/// The counters prove that no re-shape happened; this proves the answer is still right, which no
/// counter can.
#[test]
fn a_rebreak_equals_a_fresh_shape_at_the_same_width() {
    let _guard = counting();
    let fixture = Fixture::new(PROSE, Direction::LeftToRight);
    let content = fixture.content();

    let (_fonts, mut warm) = support::shaper(Controls::Mark);
    let mut shaped = warm.shape(&content);
    for width in [400.0, 320.0, 260.0] {
        warm.break_lines(
            &mut shaped,
            &BreakRequest::new(&content, Some(CssPx(width))),
        );
    }
    let rebroken = warm.break_lines(
        &mut shaped,
        &BreakRequest::new(&content, Some(CssPx(180.0))),
    );

    let (_fonts, mut cold) = support::shaper(Controls::Mark);
    let mut fresh = cold.shape(&content);
    let directly = cold.break_lines(&mut fresh, &BreakRequest::new(&content, Some(CssPx(180.0))));

    assert_eq!(rebroken.geometry.lines, directly.geometry.lines);
    assert_eq!(rebroken.geometry.size, directly.geometry.size);
}

/// A colour change moves neither key, so it costs nothing at all.
#[test]
fn a_brush_change_costs_nothing() {
    let _guard = counting();
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let mut cache = ParagraphCache::new();
    let style = Arc::new(support::style());
    let mut map = TextMap::new();
    map.push(0..PROSE.len(), 0, 0);
    let paragraph = ParagraphStyle::initial();

    counter::reset();
    for slot in [0u32, 7, 9] {
        let runs = [StyledRun {
            text: 0..PROSE.len(),
            style: style.clone(),
            brush: PaintSlot(slot),
        }];
        let content = ParagraphContent {
            text: PROSE,
            map: &map,
            runs: &runs,
            boxes: &[],
            paragraph: &paragraph,
            scale: 1.0,
        };
        lay_out(
            &mut shaper,
            &mut cache,
            &content,
            &BreakRequest::new(&content, Some(CssPx(300.0))),
        );
    }
    assert_eq!(counter::get(Counter::TextShaped), 1);
    assert_eq!(counter::get(Counter::TextRebroken), 1);
    assert_eq!(cache.len(), 1, "one entry serves all three brushes");
}

/// Re-styling `vertical-align` moves the box without re-shaping the paragraph.
///
/// The shift is folded into the height the engine is told, so nothing in the shaped glyphs can
/// notice it changed. What makes it reach the output is that the geometry is an input on every
/// breaking pass: a shift that moved changes the breaking key, which forces a pass, which pushes
/// the new height in.
#[test]
fn changing_vertical_align_moves_the_box_without_reshaping() {
    let _guard = counting();
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let mut cache = ParagraphCache::new();
    let style = Arc::new(support::style());
    let text = "before after";
    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let paragraph = ParagraphStyle::initial();
    let runs = [StyledRun {
        text: 0..text.len(),
        style,
        brush: PaintSlot(0),
    }];

    let place = |shaper: &mut _, cache: &mut ParagraphCache<_>, shift: f32| {
        let boxes = [InlineBoxGeometry {
            id: 1,
            offset: 6,
            width: CssPx(20.0),
            height: CssPx(20.0),
            ascent: CssPx(20.0),
            shift: CssPx(shift),
        }];
        let content = ParagraphContent {
            text,
            map: &map,
            runs: &runs,
            boxes: &boxes,
            paragraph: &paragraph,
            scale: 1.0,
        };
        let request = BreakRequest::new(&content, Some(CssPx(400.0)));
        let (_shaped, broken) = lay_out(shaper, cache, &content, &request);
        let placement = broken.boxes[0];
        // Measured against the baseline of the line it landed on, because raising a box makes the
        // line taller and moves the baseline down with it: the absolute corner would then move by
        // less than the shift, and an assertion on it would be measuring the line box instead.
        broken.geometry.lines[placement.line].baseline.0 - placement.origin.y.0
    };

    counter::reset();
    let baseline = place(&mut shaper, &mut cache, 0.0);
    let raised = place(&mut shaper, &mut cache, 5.0);

    assert_eq!(
        counter::get(Counter::TextShaped),
        1,
        "the shift is not in the shaping key and must not be"
    );
    assert_eq!(counter::get(Counter::TextRebroken), 2);
    assert!(
        (baseline - 20.0).abs() < 0.001,
        "an unshifted box sits its whole ascent above the baseline, got {baseline}"
    );
    assert!(
        (raised - baseline - 5.0).abs() < 0.001,
        "raising the box by five pixels lifted it from {baseline} to {raised} above the baseline"
    );
}

/// A paragraph with no atomic inlines reports none, and one with them reports where they landed.
#[test]
fn atomic_inlines_are_placed_on_the_line_they_landed_on() {
    let _guard = counting();
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let style = Arc::new(support::style());
    let text = "one two three four five six seven eight";
    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let paragraph = ParagraphStyle::initial();
    let runs = [StyledRun {
        text: 0..text.len(),
        style,
        brush: PaintSlot(0),
    }];
    let boxes = [InlineBoxGeometry {
        id: 42,
        offset: text.len(),
        width: CssPx(30.0),
        height: CssPx(12.0),
        ascent: CssPx(12.0),
        shift: CssPx(0.0),
    }];
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &boxes,
        paragraph: &paragraph,
        scale: 1.0,
    };
    let mut shaped = shaper.shape(&content);
    let broken = shaper.break_lines(
        &mut shaped,
        &BreakRequest::new(&content, Some(CssPx(120.0))),
    );

    assert!(broken.geometry.lines.len() > 1, "it has to wrap");
    let placement = broken.boxes.first().expect("the box was placed");
    assert_eq!(placement.id, 42);
    assert_eq!(
        placement.line,
        broken.geometry.lines.len() - 1,
        "a box at the end of the text lands on the last line"
    );
}

/// The first and last baselines a surrounding flex or grid aligns against are the lines'.
#[test]
fn the_geometry_reports_both_baselines() {
    let _guard = counting();
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let fixture = Fixture::new(PROSE, Direction::LeftToRight);
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);
    let broken = shaper.break_lines(
        &mut shaped,
        &BreakRequest::new(&content, Some(CssPx(160.0))),
    );

    let lines = &broken.geometry.lines;
    assert!(lines.len() > 2);
    assert_eq!(broken.geometry.first_baseline(), Some(lines[0].baseline));
    assert_eq!(
        broken.geometry.last_baseline(),
        Some(lines[lines.len() - 1].baseline)
    );
    assert!(
        lines[0].baseline < lines[1].baseline,
        "later lines sit lower"
    );
    assert!(shaped.strut().line_height.0 > 0.0, "the strut was measured");
}

/// A banded request breaks each line into its own width, and an unbanded one does not.
///
/// The engine breaks a whole paragraph into a single width in one call, so the per-line widths a
/// float leaves free are driven a line at a time instead. That driver is reached only from a layout
/// engine, whose own tests stand a deterministic shaper in for this one — so without this case the
/// only implementation of banding that is ever exercised is the stand-in.
#[test]
fn a_banded_request_gives_each_line_its_own_width() {
    use zgui_text::{LineBand, LineBands};

    let _guard = counting();
    let fixture = Fixture::new(PROSE, Direction::LeftToRight);
    let content = fixture.content();
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let mut shaped = shaper.shape(&content);

    // Two narrow lines beside a float 80 wide, then the full width below it.
    let narrow = LineBand {
        offset: CssPx(80.0),
        max_advance: CssPx(120.0),
    };
    let wide = LineBand::full(CssPx(200.0));
    let bands = [narrow, narrow, wide];
    let banded = shaper.break_lines(
        &mut shaped,
        &BreakRequest::new(&content, Some(CssPx(200.0))).banded(LineBands::new(&bands)),
    );

    assert!(
        banded.geometry.lines.len() > 3,
        "the fixture has to reach past the float, got {} lines",
        banded.geometry.lines.len()
    );
    for (index, line) in banded.geometry.lines.iter().enumerate().take(2) {
        assert!(
            line.width.0 <= 120.0,
            "line {index} is {} wide beside a band of 120",
            line.width.0
        );
        assert!(
            (line.offset.0 - 80.0).abs() < 0.001,
            "line {index} starts at {} rather than after the float",
            line.offset.0
        );
    }
    for (index, line) in banded.geometry.lines.iter().enumerate().skip(2) {
        assert!(
            line.offset.0.abs() < 0.001,
            "line {index} below the float still starts at {}",
            line.offset.0
        );
    }
    assert!(
        banded
            .geometry
            .lines
            .iter()
            .skip(2)
            .any(|line| line.width.0 > 120.0),
        "no line below the float used the width it gave back, so the bands are being applied to \
         every line alike and the driver is untested"
    );

    // The control: the same paragraph at the same paragraph width with no bands at all.
    let plain = shaper.break_lines(
        &mut shaped,
        &BreakRequest::new(&content, Some(CssPx(200.0))),
    );
    assert!(
        plain
            .geometry
            .lines
            .iter()
            .all(|line| line.offset.0.abs() < 0.001),
        "an unbanded break indented a line"
    );
    assert_ne!(
        plain.geometry.lines.len(),
        banded.geometry.lines.len(),
        "banding changed nothing about how the paragraph broke"
    );
}
