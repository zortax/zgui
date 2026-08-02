//! Hit testing a line the shaper reordered.
//!
//! A bidirectional line is where a hit test written against the string instead of against the
//! layout stops working, and it fails in a way no left-to-right fixture can show: one place on the
//! screen is two offsets in the text, and one offset is two places on the screen. The real engine
//! is the only thing that produces such a line, so this target uses it — over the two faces the
//! text engine's own tests ship, never over whatever fonts the machine happens to have, so the
//! numbers below are a property of the faces and not of the runner.

use std::sync::Arc;

use zgui_edit::LineMap;
use zgui_edit::select::Affinity;
use zgui_geom::{Css, CssPx, Point};
use zgui_scene::PaintSlot;
use zgui_text::{
    BreakRequest, FontSource, ParagraphContent, ParagraphShaper, StyledRun, TextDirection, TextMap,
};
use zgui_text_parley::{Controls, FontSystem, FontSystemOptions, Shaper};
use zgui_text_style::{
    Direction, FamilyName, FontFamilyList, GenericFamily, ParagraphStyle, TextAlign, TextStyle,
};

/// A Latin filename followed by an Arabic body: the first strong character is Latin, so the line
/// holds both directions whichever way its base runs.
const BIDI: &str = "report.pdf ملف تقرير سنوي";

/// Where the shipped faces live.
///
/// They belong to the text engine's own tests and are the only faces checked into the workspace.
/// Reading them from there is better than a second copy of a megabyte of font binary whose only
/// purpose would be to avoid a relative path.
const FONTS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../zgui-text-parley/tests/fonts"
);

/// Reads one of the shipped faces.
fn face(file: &str) -> Arc<dyn AsRef<[u8]> + Send + Sync> {
    let path = format!("{FONTS}/{file}");
    Arc::new(std::fs::read(&path).unwrap_or_else(|error| panic!("reading {path}: {error}")))
}

/// A shaper over the two shipped faces and nothing the machine contributes.
fn shaper() -> Shaper {
    let system = Arc::new(FontSystem::new(FontSystemOptions::registered_only()));
    system
        .register(face("NotoSans-Regular.ttf"), None)
        .expect("the Latin face registers");
    system
        .register(face("NotoSansArabic-Regular.ttf"), None)
        .expect("the Arabic face registers");
    Shaper::with_controls(system, Controls::Mark)
}

/// The style naming the shipped faces.
fn style() -> TextStyle {
    TextStyle {
        family: FontFamilyList::from_iter([
            FamilyName::Named(zgui_interned::Ident::new("Noto Sans Arabic")),
            FamilyName::Named(zgui_interned::Ident::new("Noto Sans")),
            FamilyName::Generic(GenericFamily::SansSerif),
        ]),
        size: CssPx(16.0),
        ..TextStyle::initial()
    }
}

/// The line map of [`BIDI`] laid out in one base direction.
fn laid_out(direction: Direction) -> LineMap {
    let text = BIDI;
    let mut map = TextMap::new();
    map.push(0..text.len(), 0, 0);
    let runs = vec![StyledRun {
        text: 0..text.len(),
        style: Arc::new(style()),
        brush: PaintSlot(0),
    }];
    let paragraph = ParagraphStyle {
        direction,
        align: TextAlign::Start,
        ..ParagraphStyle::initial()
    };
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[],
        paragraph: &paragraph,
        scale: 1.0,
    };
    let mut shaper = shaper();
    let mut shaped = shaper.shape(&content);
    let broken = shaper.break_lines(
        &mut shaped,
        &BreakRequest::new(&content, Some(CssPx(600.0))),
    );
    LineMap::of(&shaper, &shaped, &broken)
}

/// A point in the paragraph's own space.
fn at(x: f32, y: f32) -> Point<CssPx, Css> {
    Point::new(CssPx(x), CssPx(y))
}

#[test]
fn the_line_really_holds_both_directions() {
    // The precondition every other test here rests on: if the engine produced one run, the round
    // trips below would be asserting nothing about bidirectional text at all.
    let map = laid_out(Direction::RightToLeft);
    let line = &map.lines()[0];
    assert!(
        line.runs.len() > 1,
        "the fixture has to reorder, and produced {} run(s)",
        line.runs.len()
    );
    assert!(
        line.runs
            .iter()
            .any(|run| run.direction == TextDirection::RightToLeft)
            && line
                .runs
                .iter()
                .any(|run| run.direction == TextDirection::LeftToRight),
        "and it has to hold a run of each direction"
    );
}

#[test]
fn every_cluster_of_a_reordered_line_round_trips() {
    for direction in [Direction::LeftToRight, Direction::RightToLeft] {
        let map = laid_out(direction);
        for line in map.lines() {
            let y = line.geometry.top.0 + line.geometry.height.0 / 2.0;
            for run in &line.runs {
                for cluster in &run.clusters {
                    if cluster.advance.0 <= 0.0 {
                        // A mark of no width has no box to aim at; it is reached by moving the
                        // caret, never by clicking.
                        continue;
                    }
                    let left = line.geometry.offset.0 + run.start.0 + cluster.offset.0;
                    let quarter = left + cluster.advance.0 / 4.0;
                    let three_quarters = left + cluster.advance.0 * 3.0 / 4.0;
                    let (leading_x, trailing_x) = if run.is_rtl() {
                        (three_quarters, quarter)
                    } else {
                        (quarter, three_quarters)
                    };

                    let leading = map.hit(at(leading_x, y)).expect("inside the line");
                    assert_eq!(
                        leading.offset, cluster.text.start,
                        "the leading edge of a {:?} cluster is its first byte",
                        run.direction
                    );
                    assert_eq!(leading.affinity, Affinity::Downstream);

                    let trailing = map.hit(at(trailing_x, y)).expect("inside the line");
                    assert_eq!(trailing.offset, cluster.text.end);
                    assert_eq!(trailing.affinity, Affinity::Upstream);

                    for hit in [leading, trailing] {
                        let caret = map
                            .caret(hit.offset, hit.affinity)
                            .expect("an offset a hit reported has a caret");
                        let right = left + cluster.advance.0;
                        assert!(
                            caret.origin.x.0 >= left - 0.01 && caret.origin.x.0 <= right + 0.01,
                            "the caret for {hit:?} landed at {} outside {left}..{right}",
                            caret.origin.x.0
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn one_offset_at_a_direction_boundary_has_two_carets() {
    // This is the whole of what affinity is for. At the seam between the two runs, the offset
    // reached by moving forwards through the Latin text and the offset reached by moving backwards
    // into the Arabic text are drawn in two different places, and an editor that kept one caret per
    // offset would put the insertion point at the wrong end of the line.
    let map = laid_out(Direction::RightToLeft);
    let line = &map.lines()[0];
    let mut boundaries = 0;

    for pair in line.runs.windows(2) {
        let (earlier, later) = (&pair[0], &pair[1]);
        if earlier.direction == later.direction {
            continue;
        }
        // The offset drawn at the seam, read from the run on the left of it.
        let seam = if earlier.is_rtl() {
            earlier
                .clusters
                .first()
                .expect("a run has clusters")
                .text
                .start
        } else {
            earlier
                .clusters
                .last()
                .expect("a run has clusters")
                .text
                .end
        };
        let upstream = map.caret(seam, Affinity::Upstream).expect("a caret");
        let downstream = map.caret(seam, Affinity::Downstream).expect("a caret");
        if (upstream.origin.x.0 - downstream.origin.x.0).abs() > 0.5 {
            boundaries += 1;
        }
    }

    assert!(
        boundaries > 0,
        "a line holding both directions has at least one offset with two carets"
    );
}

#[test]
fn the_two_sides_of_a_reordering_seam_are_two_unrelated_offsets() {
    // The other reading of the same fact. Where the drawn order breaks the byte order, the text on
    // the left of a seam and the text on its right are nowhere near each other in the string, so
    // hitting two points four pixels apart reports two offsets that are not neighbours. A hit test
    // computed from the string cannot produce that, and a left-to-right fixture cannot show it.
    let map = laid_out(Direction::RightToLeft);
    let line = &map.lines()[0];
    let y = line.geometry.top.0 + line.geometry.height.0 / 2.0;

    let mut jumps = 0;
    for run in &line.runs {
        let seam = line.geometry.offset.0 + run.start.0 + run.advance();
        let (Some(left), Some(right)) = (map.hit(at(seam - 2.0, y)), map.hit(at(seam + 2.0, y)))
        else {
            continue;
        };
        if left.offset.abs_diff(right.offset) > 1 {
            jumps += 1;
        }
    }

    assert!(
        jumps > 0,
        "a reordered line has at least one seam whose two sides are far apart in the text"
    );
}

#[test]
fn a_selection_spanning_a_direction_boundary_is_painted_as_more_than_one_band() {
    // A logically contiguous range is *visually* split by a reordering, and painting it as one
    // rectangle from its first cluster to its last covers text nobody selected — on this fixture,
    // the whole of the other direction's run. So the fault a single band would produce is not a
    // missing highlight but a highlight over the wrong words, which is why the count is asserted
    // as well as the area.
    let map = laid_out(Direction::RightToLeft);
    let line = &map.lines()[0];

    // A range that starts inside one run and ends inside the next one of the other direction.
    let mut span = None;
    for pair in line.runs.windows(2) {
        let (earlier, later) = (&pair[0], &pair[1]);
        if earlier.direction == later.direction {
            continue;
        }
        let one = earlier.clusters.first().expect("a run has clusters");
        let other = later.clusters.last().expect("a run has clusters");
        let (start, end) = (
            one.text.start.min(other.text.start),
            one.text.end.max(other.text.end),
        );
        span = Some(start..end);
        break;
    }
    let span = span.expect("the fixture reorders, so it has a boundary");

    let bands = map.highlight(span.clone());
    assert!(
        bands.len() > 1,
        "a range across a reordering must be painted as separate bands, not one from {} to {}: {bands:?}",
        span.start,
        span.end
    );

    // The bands do not overlap, and every one of them is inside the line box. Two overlapping bands
    // are a highlight drawn twice, which at any alpha below one is a visible stripe.
    for (index, band) in bands.iter().enumerate() {
        assert!(
            band.size.width.0 > 0.0,
            "an empty band was pushed: {band:?}"
        );
        assert_eq!(band.line, 0);
        for other in &bands[index + 1..] {
            assert!(
                band.right().0 <= other.origin.x.0 || other.right().0 <= band.origin.x.0,
                "two bands overlap: {band:?} and {other:?}"
            );
        }
    }

    // Every cluster the range covers is under a band, and no cluster outside it is. This is the
    // assertion a band count cannot make: a pair of bands in the wrong two places satisfies the
    // count and covers the wrong letters.
    for run in &line.runs {
        for cluster in &run.clusters {
            let middle =
                line.geometry.offset.0 + run.start.0 + cluster.offset.0 + cluster.advance.0 / 2.0;
            let covered = bands
                .iter()
                .any(|band| middle >= band.origin.x.0 && middle < band.right().0);
            let selected = cluster.text.start >= span.start && cluster.text.end <= span.end;
            assert_eq!(
                covered,
                selected,
                "the cluster at {:?} is {} and is {} a band",
                cluster.text,
                if selected { "selected" } else { "not selected" },
                if covered { "under" } else { "not under" },
            );
        }
    }
}
