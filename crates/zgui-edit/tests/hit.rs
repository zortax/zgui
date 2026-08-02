//! Points to offsets and back, over a paragraph a shaper really laid out.
//!
//! Nothing here computes an expected position from the string. Every coordinate asked about comes
//! out of the same shaping the assertions are made against, which is the only way this can be a
//! round trip rather than two copies of one arithmetic mistake.

use std::sync::Arc;

use zgui_edit::LineMap;
use zgui_edit::select::Affinity;
use zgui_geom::{Css, CssPx, Point};
use zgui_scene::PaintSlot;
use zgui_testkit_scene::MonoShaper;
use zgui_text::{
    BreakRequest, BrokenParagraph, ParagraphContent, ParagraphShaper, ShapedParagraph, StyledRun,
    TextMap,
};
use zgui_text_style::{ParagraphStyle, TextStyle};

/// A paragraph laid out at a width, and the map over it.
struct Laid {
    /// The map under test.
    map: LineMap,
    /// The lines it was built from.
    broken: BrokenParagraph,
}

/// Lays `text` out at `width` and builds the line map from the same break.
fn lay_out(text: &str, width: Option<CssPx>) -> Laid {
    let style = Arc::new(TextStyle::initial());
    let paragraph = ParagraphStyle::initial();
    let map = TextMap::new();
    let runs = [StyledRun {
        text: 0..text.len(),
        style,
        brush: PaintSlot(0),
    }];
    let content = ParagraphContent {
        text,
        map: &map,
        runs: &runs,
        boxes: &[],
        paragraph: &paragraph,
        scale: 1.0,
    };
    let mut shaper = MonoShaper::new();
    let mut shaped: ShapedParagraph<_> = shaper.shape(&content);
    let broken = shaper.break_lines(&mut shaped, &BreakRequest::new(&content, width));
    Laid {
        map: LineMap::of(&shaper, &shaped, &broken),
        broken,
    }
}

/// A point in the paragraph's own space.
fn at(x: f32, y: f32) -> Point<CssPx, Css> {
    Point::new(CssPx(x), CssPx(y))
}

#[test]
fn every_cluster_boundary_the_shaper_produced_round_trips() {
    // The property, over every cluster of every line: hitting inside a cluster reports one of its
    // two edges, and asking for that edge's caret puts it back inside the same cluster.
    let laid = lay_out("one two three four five", Some(CssPx(80.0)));
    assert!(laid.map.lines().len() > 1, "the fixture has to wrap");

    for line in laid.map.lines() {
        for run in &line.runs {
            for cluster in &run.clusters {
                let left = line.geometry.offset.0 + run.start.0 + cluster.offset.0;
                let right = left + cluster.advance.0;
                let y = line.geometry.top.0 + line.geometry.height.0 / 2.0;
                let quarter = left + cluster.advance.0 / 4.0;
                let three_quarters = right - cluster.advance.0 / 4.0;

                let leading = laid.map.hit(at(quarter, y)).expect("inside the paragraph");
                assert_eq!(leading.offset, cluster.text.start);
                assert_eq!(leading.affinity, Affinity::Downstream);

                let trailing = laid
                    .map
                    .hit(at(three_quarters, y))
                    .expect("inside the paragraph");
                assert_eq!(trailing.offset, cluster.text.end);
                assert_eq!(trailing.affinity, Affinity::Upstream);

                for hit in [leading, trailing] {
                    let caret = laid
                        .map
                        .caret(hit.offset, hit.affinity)
                        .expect("an offset a hit reported has a caret");
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

#[test]
fn a_click_below_the_last_line_lands_on_it_and_not_nowhere() {
    let laid = lay_out("one two three", Some(CssPx(40.0)));
    let last = laid.map.lines().len() - 1;
    let hit = laid
        .map
        .hit(at(1000.0, 1000.0))
        .expect("a click in the margin still lands");
    assert_eq!(hit.line, last);
    assert_eq!(
        hit.offset, laid.broken.geometry.lines[last].text.end,
        "past the end of the last line is the end of its text"
    );
}

#[test]
fn a_click_before_the_start_of_a_line_lands_at_its_first_offset() {
    let laid = lay_out("one two three", Some(CssPx(40.0)));
    let hit = laid
        .map
        .hit(at(-50.0, 1.0))
        .expect("still on the first line");
    assert_eq!(hit.line, 0);
    assert_eq!(hit.offset, 0);
    assert_eq!(hit.affinity, Affinity::Downstream);
}

#[test]
fn the_caret_of_every_line_start_sits_on_that_line() {
    let laid = lay_out("one two three four", Some(CssPx(60.0)));
    for (index, line) in laid.broken.geometry.lines.iter().enumerate() {
        let caret = laid
            .map
            .caret(line.text.start, Affinity::Downstream)
            .expect("a line start has a caret");
        assert_eq!(caret.line, index);
        assert_eq!(caret.origin.y.0, line.top.0);
        assert_eq!(caret.height, line.height);
    }
}

#[test]
fn a_click_in_an_empty_paragraph_lands_at_its_only_offset_rather_than_nowhere() {
    // An empty field is the ordinary state of one nobody has typed into yet, and clicking it has
    // to put the caret at offset zero — an answer of "no line" there is a field a click cannot
    // focus. The shaper produces one empty line for it, which carries no clusters at all, so this
    // is also the case where the run and cluster walks both come up empty.
    let laid = lay_out("", None);
    assert_eq!(laid.map.lines().len(), 1, "an empty paragraph is one line");
    let hit = laid
        .map
        .hit(at(0.0, 1.0))
        .expect("a click in an empty field has to land somewhere");
    assert_eq!(hit.offset, 0);
    assert_eq!(hit.line, 0);
    assert!(
        laid.map.caret(0, Affinity::Downstream).is_some(),
        "and the caret it reported has a place to be drawn"
    );
}

#[test]
fn a_hit_is_mapped_back_through_the_offsets_the_source_actually_had() {
    // The generated string is not the source string: this map says the shaped text began two bytes
    // into the source, which is what collapsing leading white space produces.
    let laid = lay_out("abc", None);
    let mut map = TextMap::new();
    map.push(0..3, 0, 2);
    let hit = laid.map.hit(at(9.0, 1.0)).expect("inside the text");
    let source = laid.map.to_source(hit, &map).expect("a mapped position");
    assert_eq!(source.run, 0);
    assert_eq!(source.offset, hit.offset + 2);
}
