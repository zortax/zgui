//! Positioned glyphs out of a real shaped paragraph.
//!
//! Everything the paint stage draws text from comes through one method, and this is the target that
//! exercises it against real faces rather than a fixture. What matters is not the exact coordinates
//! — those are the face's business — but that the positions are in the **line box's own space**: a
//! run that reported the engine's own paragraph-relative coordinates would draw every line of a
//! paragraph on top of the first one, and a run that forgot the alignment offset would draw a
//! centred line at the left edge.

mod support;

use zgui_text::{ParagraphShaper, ShapedGlyph, ShapedRun};
use zgui_text_parley::Controls;
use zgui_text_style::Direction;

/// Every glyph of one line, in the order the runs report them.
fn glyphs_of(shaper: &zgui_text_parley::Shaper, shaped: &Shaped, line: u16) -> Vec<ShapedGlyph> {
    let mut collected = Vec::new();
    shaper.visit_line(shaped, line, &mut |run: ShapedRun<'_>| {
        collected.extend_from_slice(run.glyphs);
    });
    collected
}

/// The shaped form these tests carry around.
type Shaped = zgui_text::ShapedParagraph<zgui_text_parley::ShapedLayout>;

/// A line of Latin text produces glyphs that advance across the line from its own left edge.
#[test]
fn a_line_reports_its_glyphs_in_its_own_space() {
    let (_, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new("handgloves", Direction::LeftToRight);
    let shaped = shaper.shape(&fixture.content());

    let glyphs = glyphs_of(&shaper, &shaped, 0);
    assert!(
        glyphs.len() >= 8,
        "ten letters shaped to {} glyphs, which is not a shaping at all",
        glyphs.len()
    );
    assert!(
        glyphs[0].x >= 0.0,
        "the first glyph of a start-aligned line begins at the line box's own left edge, and this \
         one is at {}",
        glyphs[0].x
    );
    for pair in glyphs.windows(2) {
        assert!(
            pair[1].x > pair[0].x,
            "left-to-right glyphs advance rightwards: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
    let baseline = glyphs[0].y;
    assert!(
        baseline > 0.0,
        "the baseline sits below the top of the line box, and is reported at {baseline}"
    );
    assert!(
        glyphs.iter().all(|glyph| glyph.y == baseline),
        "every glyph of one horizontal run sits on one baseline"
    );
}

/// A second line's glyphs are reported against its own box, not stacked below the first.
#[test]
fn a_second_line_is_not_offset_by_the_first() {
    let (_, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new("alpha beta", Direction::LeftToRight);
    let mut shaped = shaper.shape(&fixture.content());
    let request = zgui_text::BreakRequest::new(&fixture.content(), Some(zgui_geom::CssPx(48.0)));
    let broken = shaper.break_lines(&mut shaped, &request);
    assert!(
        broken.geometry.lines.len() >= 2,
        "the fixture was supposed to wrap and did not"
    );

    let first = glyphs_of(&shaper, &shaped, 0);
    let second = glyphs_of(&shaper, &shaped, 1);
    assert!(!first.is_empty() && !second.is_empty());
    // The exact invariant, against the geometry layout positions the line boxes with: a glyph's
    // reported height is its line's baseline measured from that line's own top edge. The two lines
    // here are deliberately of different heights — the space falls back to a taller face — so a
    // conversion that happened to be right for one of them is visibly wrong for the other.
    for (index, glyphs) in [(0usize, &first), (1, &second)] {
        let line = &broken.geometry.lines[index];
        assert_eq!(
            glyphs[0].y,
            line.baseline.0 - line.top.0,
            "line {index} reported its glyphs against the paragraph rather than against its own \
             box: {line:?}"
        );
        assert!(
            glyphs[0].y < line.height.0,
            "a baseline outside its own line box is not a baseline: {line:?}"
        );
    }
    assert!(
        second[0].x < 8.0,
        "the second line starts at its own left edge and not after the first line's advance: {}",
        second[0].x
    );
}

/// The run a glyph belongs to names a face the compositor's font registry knows.
#[test]
fn a_run_names_a_face_and_a_size_the_compositor_can_draw() {
    use iceblink::FontRegistry;

    let (fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new("handgloves", Direction::LeftToRight);
    let shaped = shaper.shape(&fixture.content());
    let registry = zgui_text_parley::CompositorFonts::new(fonts);

    let mut runs = 0;
    shaper.visit_line(&shaped, 0, &mut |run: ShapedRun<'_>| {
        assert!(run.size > 0.0, "a run drawn at no size draws nothing");
        assert!(
            registry.get(iceblink::FontId(run.face.0)).is_some(),
            "the run names {:?}, which the registry does not know",
            run.face
        );
        assert!(
            run.glyphs.iter().all(|glyph| glyph.glyph != 0),
            "every letter of the line is in the face"
        );
        runs += 1;
    });
    assert!(runs > 0, "the line has a run");
}

/// A run reports where its text starts in the caller's string, with the direction mark left out.
///
/// Two zero-width inline boxes stand for a span's edges around `beta`, and a run starts at each.
#[test]
fn a_run_reports_where_its_text_starts() {
    let (_, mut shaper) = support::shaper(Controls::Mark);
    let fixture = support::Fixture::new("alpha beta gamma", Direction::LeftToRight);
    let edge = |id: u64, offset: usize| zgui_text::InlineBoxGeometry {
        id,
        offset,
        width: zgui_geom::CssPx(0.0),
        height: zgui_geom::CssPx(0.0),
        ascent: zgui_geom::CssPx(0.0),
        shift: zgui_geom::CssPx(0.0),
    };
    let boxes = [edge(0, 6), edge(1, 10)];
    let content = zgui_text::ParagraphContent {
        boxes: &boxes,
        ..fixture.content()
    };
    let shaped = shaper.shape(&content);
    let mut starts = Vec::new();
    shaper.visit_line(&shaped, 0, &mut |run: ShapedRun<'_>| {
        starts.push(run.text_start);
    });
    // The engine may split runs further, at word boundaries.
    assert!(
        starts.windows(2).all(|pair| pair[0] <= pair[1]),
        "runs come in text order: {starts:?}"
    );
    assert!(
        starts.contains(&6) && starts.contains(&10),
        "a run starts at each edge: {starts:?}"
    );
    assert!(
        !starts.contains(&3),
        "the three bytes of the direction mark are not counted: {starts:?}"
    );
}
