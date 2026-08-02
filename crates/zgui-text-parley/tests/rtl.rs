//! Base direction, and the one control that sets it.

mod support;

use support::{BIDI, Fixture, RTL_ONLY};
use zgui_geom::CssPx;
use zgui_text::{BreakRequest, ParagraphShaper};
use zgui_text_parley::Controls;
use zgui_text_style::Direction;

/// The width the alignment offsets below are measured in.
const BOX_WIDTH: CssPx = CssPx(600.0);

/// How far the bidirectional fixture's one line is inset from the start edge of that box once its
/// base direction is right-to-left.
///
/// A fixed number rather than an inequality, and it can be one because the collection holds only
/// the two faces these tests ship: the advance is a property of those faces at sixteen pixels and
/// of nothing the running machine contributes.
const FORCED_RTL_OFFSET: f32 = 412.64;

/// Lays the bidirectional fixture out and reports whether it came out right-to-left and how far
/// its one line is inset from the start edge.
fn lay_out(text: &str, direction: Direction, controls: Controls) -> (bool, f32) {
    let (_fonts, mut shaper) = support::shaper(controls);
    let fixture = Fixture::new(text, direction);
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);
    let broken = shaper.break_lines(&mut shaped, &BreakRequest::new(&content, Some(BOX_WIDTH)));
    let offset = broken
        .geometry
        .lines
        .first()
        .map_or(0.0, |line| line.offset.0);
    (broken.geometry.is_rtl, offset)
}

/// A leading directional mark forces the base direction; an isolate pair does not.
///
/// Four rows, and the second is the load-bearing one. Wrapping the whole paragraph in an isolate
/// pair is the mechanism one reaches for, and it leaves the base level left-to-right: the
/// bidirectional algorithm's paragraph rule skips isolate contents, so no strong character is
/// visible to it at all. The content still reorders correctly, which is why the failure shows up
/// only in the alignment — a right-to-left paragraph sitting against the left edge of its box.
#[test]
fn rtl_mark_flips_base_direction() {
    let isolated = format!("\u{2067}{BIDI}\u{2069}");

    // Automatic detection: the first strong character is the Latin filename.
    let (rtl, offset) = lay_out(BIDI, Direction::LeftToRight, Controls::Verbatim);
    assert!(!rtl, "the fixture's first strong character is Latin");
    assert_eq!(offset, 0.0, "a left-to-right paragraph starts at the left");

    // The negative control: an isolate around the whole paragraph sets nothing.
    let (rtl, offset) = lay_out(&isolated, Direction::RightToLeft, Controls::Verbatim);
    assert!(
        !rtl,
        "an isolate pair around the paragraph must not set the base level"
    );
    assert_eq!(
        offset, 0.0,
        "and so it aligns to the wrong edge, which is the whole failure"
    );

    // The mitigation.
    let (rtl, offset) = lay_out(BIDI, Direction::RightToLeft, Controls::Mark);
    assert!(rtl, "U+200F must make the paragraph right-to-left");
    assert!(
        (offset - FORCED_RTL_OFFSET).abs() < 0.01,
        "the line is inset from the start edge by whatever it does not fill: expected \
         {FORCED_RTL_OFFSET}, got {offset}"
    );

    // And it survives an inner isolate, which a document with `unicode-bidi: isolate` on a span has.
    let (rtl, _) = lay_out(&isolated, Direction::RightToLeft, Controls::Mark);
    assert!(rtl, "the mark still sets the base level through an isolate");

    // The left-to-right mark is the same mechanism in the other direction, and it needs the
    // mirror fixture: the bidirectional one already detects as left-to-right, so a row asserting
    // that over it would pass with nothing prefixed at all.
    let (rtl, offset) = lay_out(RTL_ONLY, Direction::LeftToRight, Controls::Verbatim);
    assert!(
        rtl,
        "every strong character in the mirror fixture is Arabic"
    );
    assert!(offset > 0.0, "and it aligns to the right edge of the box");

    let (rtl, offset) = lay_out(RTL_ONLY, Direction::LeftToRight, Controls::Mark);
    assert!(!rtl, "U+200E must make the paragraph left-to-right");
    assert_eq!(
        offset, 0.0,
        "and a left-to-right paragraph starts at the left"
    );
}

/// The visual left edge of a forced right-to-left line is the *end* of the source text.
#[test]
fn rtl_hit_testing_maps_to_the_logical_tail() {
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let fixture = Fixture::new(BIDI, Direction::RightToLeft);
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);
    let broken = shaper.break_lines(&mut shaped, &BreakRequest::new(&content, Some(BOX_WIDTH)));
    assert!(broken.geometry.is_rtl);

    let line = shaped.engine.layout.get(0).expect("one line");
    let baseline = line.metrics().baseline - 1.0;
    let x = line.metrics().offset + 1.0;
    let (cluster, _) = parley_cluster(&shaped.engine, x, baseline);
    let position = shaped
        .map()
        .to_source_snapped(cluster)
        .expect("the left edge maps to a source position");
    assert!(
        position.offset > BIDI.len() / 2,
        "the visual left edge maps to source offset {} of {}",
        position.offset,
        BIDI.len()
    );

    // The same text read left to right puts the filename on the left instead.
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let fixture = Fixture::new(BIDI, Direction::LeftToRight);
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);
    shaper.break_lines(&mut shaped, &BreakRequest::new(&content, Some(BOX_WIDTH)));
    let line = shaped.engine.layout.get(0).expect("one line");
    let (cluster, _) = parley_cluster(&shaped.engine, 1.0, line.metrics().baseline - 1.0);
    assert_eq!(
        shaped.map().to_source_snapped(cluster).map(|at| at.offset),
        Some(0),
        "left to right puts the filename first"
    );
}

/// The generated offset of the cluster under a point, and whether one was found.
///
/// Asked of the engine directly rather than of anything this crate reports, so the answer counts
/// the directional prefix and has to have it taken off before it means anything to the caller's
/// map. Everything this crate hands out has had that done for it already.
fn parley_cluster(shaped: &zgui_text_parley::ShapedLayout, x: f32, y: f32) -> (usize, bool) {
    match parley::Cluster::from_point(&shaped.layout, x, y) {
        Some((cluster, _)) => (
            cluster.text_range().start.saturating_sub(shaped.prefix),
            true,
        ),
        None => (0, false),
    }
}

/// No offset a caller sees counts the directional prefix.
///
/// The prefix is a real character to the engine — it is shaped, it is a cluster, and every offset
/// the engine reports is that many bytes along. What this crate reports is the caller's own string:
/// the shaped result carries the text and the map the caller handed in, and the line and cluster
/// ranges read out of the engine have the prefix taken off. A shifted map beside shifted offsets
/// would be self-consistent and would put every caret a prefix away from the letter it belongs to.
#[test]
fn the_directional_prefix_is_taken_off_every_offset_reported() {
    let (_fonts, mut shaper) = support::shaper(Controls::Mark);
    let fixture = Fixture::new(BIDI, Direction::RightToLeft);
    let content = fixture.content();
    let mut shaped = shaper.shape(&content);

    assert_eq!(
        shaped.engine.prefix,
        '\u{200f}'.len_utf8(),
        "the engine was handed the mark the caller's string did not carry"
    );
    assert_eq!(
        shaped.text(),
        BIDI,
        "the shaped result carries the caller's string, not the engine's"
    );
    assert_eq!(
        shaped.map().to_source(0).map(|at| at.offset),
        Some(0),
        "offset zero is the caller's first byte and maps to it"
    );

    // The first line covers the caller's whole string and starts at its first byte, not three bytes
    // into it.
    let broken = shaper.break_lines(&mut shaped, &BreakRequest::new(&content, Some(BOX_WIDTH)));
    let first = broken.geometry.lines.first().expect("one line at least");
    assert_eq!(first.text.start, 0);
    assert_eq!(
        broken
            .geometry
            .lines
            .last()
            .expect("one line at least")
            .text
            .end,
        BIDI.len(),
        "the lines cover the caller's string exactly"
    );

    // And so does every cluster: each one names bytes the caller's string really has.
    let mut clusters = 0;
    for line in 0..broken.geometry.lines.len() {
        shaper.visit_clusters(&shaped, line as u16, &mut |run| {
            for cluster in run.clusters {
                assert!(
                    cluster.text.end <= BIDI.len(),
                    "a cluster claims bytes {:?} of a {}-byte string",
                    cluster.text,
                    BIDI.len()
                );
                assert!(
                    BIDI.is_char_boundary(cluster.text.start),
                    "a cluster starts inside a character of the caller's string"
                );
                clusters += 1;
            }
        });
    }
    assert!(clusters > 0, "the fixture shaped nothing at all");
}

/// Nothing is prefixed when the caller says the string already means what it says.
#[test]
fn verbatim_controls_leave_the_string_alone() {
    let (_fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = Fixture::new(BIDI, Direction::RightToLeft);
    let shaped = shaper.shape(&fixture.content());
    assert_eq!(shaped.engine.prefix, 0);
    assert_eq!(shaped.text(), BIDI);
    assert_eq!(shaped.map().to_source(0).map(|at| at.offset), Some(0));
}

/// The same suite run twice produces the same shaped advances.
///
/// Nothing here is a stopwatch: it is the alignment offset, which is the box width less the
/// advance the shaped line occupies, so an advance that moved by a hundredth of a pixel fails it.
#[test]
fn the_shaped_advances_do_not_move_between_runs() {
    let first = lay_out(BIDI, Direction::RightToLeft, Controls::Mark);
    let second = lay_out(BIDI, Direction::RightToLeft, Controls::Mark);
    assert_eq!(first, second);
    assert!((first.1 - FORCED_RTL_OFFSET).abs() < 0.01);
}
