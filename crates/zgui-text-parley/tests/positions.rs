//! Positioned glyphs out of a real shaped paragraph.
//!
//! Everything the paint stage draws text from comes through one method, and this is the target that
//! exercises it against real faces rather than a fixture. What matters is not the exact coordinates
//! — those are the face's business — but that the positions are in the **line box's own space**: a
//! run that reported the engine's own paragraph-relative coordinates would draw every line of a
//! paragraph on top of the first one, and a run that forgot the alignment offset would draw a
//! centred line at the left edge.

mod support;

use zgui_text::{ParagraphShaper, RasterStyle, ShapedGlyph, ShapedRun};
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

/// The run a glyph belongs to names a face the rasteriser can actually serve.
#[test]
fn a_run_names_a_face_and_a_size_a_rasteriser_can_use() {
    let (fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new("handgloves", Direction::LeftToRight);
    let shaped = shaper.shape(&fixture.content());
    let raster = zgui_text_parley::Rasteriser::new(fonts);

    let mut drawn = 0;
    shaper.visit_line(&shaped, 0, &mut |run: ShapedRun<'_>| {
        assert!(run.size > 0.0, "a run drawn at no size draws nothing");
        for glyph in run.glyphs {
            let key = run.key_for(*glyph, 0.0, run.raster_style(false));
            if let Some(image) = zgui_text::GlyphRaster::raster(&raster, &key) {
                assert!(image.is_well_formed());
                drawn += usize::from(!image.is_empty());
            }
        }
    });
    assert!(
        drawn >= 8,
        "only {drawn} of the line's glyphs rasterised, so the handle the run reports is not one \
         the rasteriser can resolve"
    );
}

/// The subpixel phase a glyph's position falls at is what the key carries, and it varies.
#[test]
fn the_phase_in_a_key_comes_from_the_glyph_position() {
    let (_, mut shaper) = support::shaper(Controls::Verbatim);
    let fixture = support::Fixture::new("handgloves handgloves", Direction::LeftToRight);
    let shaped = shaper.shape(&fixture.content());

    let mut phases = std::collections::BTreeSet::new();
    shaper.visit_line(&shaped, 0, &mut |run: ShapedRun<'_>| {
        for glyph in run.glyphs {
            phases.insert(run.key_for(*glyph, 0.0, RasterStyle::Grayscale).offset);
        }
    });
    assert!(
        phases.len() > 1,
        "every glyph of a proportional face landed on the same subpixel phase, so the phase is \
         being taken from something other than the position: {phases:?}"
    );
}
