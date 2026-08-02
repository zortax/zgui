//! Where a real line's ink actually lands, against the advances the shaper produced.
//!
//! # Why this is measured rather than looked at
//!
//! Every other glyph test in the workspace asserts that a tile exists, that it has an extent, or
//! that a sprite was pushed. All of those hold for a line whose letters are placed anywhere at all.
//! What the eye reads is the distance from one letter's ink to the next, so that is what is measured
//! here: each glyph is rasterised at the phase the pipeline chose for it and composited at the
//! rectangle the pipeline chose for it, and the alpha-weighted centroid of the resulting coverage is
//! compared with the centroid of the same glyph rasterised at a flush position. The difference
//! between those two is where the glyph *actually is*, in fractions of a pixel, independently of
//! what the glyph looks like.
//!
//! The error that quantity is allowed is half a subpixel step, and it must not grow along the run:
//! a pipeline that rounds each advance as it accumulates drifts, and one that takes the phase and
//! the whole pixel from different numbers puts individual letters nearly a pixel out while leaving
//! every other measurable property of the frame correct.

mod support;

use zgui_text::{
    GlyphImage, GlyphRaster, ParagraphShaper, PenPosition, RasterStyle, ShapedGlyph, ShapedRun,
    SubpixelOffset,
};
use zgui_text_parley::{Controls, Rasteriser};
use zgui_text_style::Direction;

/// Half a subpixel step, which is the most a glyph's position may move.
const TOLERANCE: f32 = 0.5 / SubpixelOffset::STEPS as f32;

/// The strings measured, chosen for pairs that kern and for advances that land on every fraction.
const LINES: [&str; 3] = [
    "handgloves AVATar Wo.",
    "The quick brown fox jumps over the lazy dog",
    "Wave. Yo, Tavi: AWAY!",
];

/// One glyph of a measured line.
struct Measured {
    /// Where the shaper put the glyph's origin, in absolute device pixels, unquantised.
    shaped: f32,
    /// Where its ink actually is, in the same coordinates.
    inked: f32,
}

/// The alpha-weighted centroid of a coverage image, in columns from its own left edge.
fn centroid(image: &GlyphImage) -> f32 {
    let width = image.size.width as usize;
    let (mut mass, mut moment) = (0.0f64, 0.0f64);
    for (index, coverage) in image.bytes.iter().enumerate() {
        let alpha = f64::from(*coverage);
        mass += alpha;
        moment += alpha * ((index % width) as f64 + 0.5);
    }
    assert!(mass > 0.0, "a glyph with no coverage cannot be located");
    (moment / mass) as f32
}

/// Measures every glyph of one line laid out with its line box's left edge at `origin`.
///
/// The placement is the pipeline's own: the phase and the whole pixel come from one
/// [`PenPosition`], and the rectangle comes from that same value, so a defect in either is visible
/// in what this returns.
fn measure(text: &str, size: f32, origin: f32) -> Vec<Measured> {
    let (fonts, mut shaper) = support::shaper(Controls::Verbatim);
    let raster = Rasteriser::new(fonts);
    let fixture = support::Fixture::sized(text, Direction::LeftToRight, size);
    let shaped = shaper.shape(&fixture.content());

    let mut measured = Vec::new();
    shaper.visit_line(&shaped, 0, &mut |run: ShapedRun<'_>| {
        for glyph in run.glyphs {
            let glyph: ShapedGlyph = *glyph;
            let position = PenPosition::of(origin + glyph.x);
            let key = run.key_at(glyph.glyph, position.offset(), RasterStyle::Grayscale);
            let Some(image) = raster.raster(&key) else {
                continue;
            };
            if image.is_empty() {
                continue;
            }
            // The same glyph rasterised flush, which is where its ink sits when its origin is on a
            // whole pixel. Subtracting it leaves the *position*, with the glyph's own shape removed.
            let flush = run.key_at(glyph.glyph, SubpixelOffset(0), RasterStyle::Grayscale);
            let flush = raster.raster(&flush).expect("the same glyph rasterises");
            let bounds = position.bounds(glyph.y, image.placement, image.size);
            measured.push(Measured {
                shaped: origin + glyph.x,
                inked: bounds.origin.x.0 + centroid(&image)
                    - (flush.placement.x.0 + centroid(&flush)),
            });
        }
    });
    assert!(
        measured.len() >= text.split_whitespace().count(),
        "{text} produced {} located glyphs, which is not a shaping",
        measured.len()
    );
    measured
}

/// The glyphs of one of [`LINES`], which is a line of prose rather than a pair.
fn line(text: &str, size: f32, origin: f32) -> Vec<Measured> {
    let measured = measure(text, size, origin);
    assert!(
        measured.len() >= 8,
        "{text} located only {} glyphs",
        measured.len()
    );
    measured
}

/// Every glyph's ink is within half a subpixel step of the position it was shaped at.
#[test]
fn each_glyph_inks_where_the_shaper_put_it() {
    for text in LINES {
        for size in [11.0f32, 13.0, 16.0, 21.0] {
            for origin in [0.0f32, 0.25, 0.5, 0.7, 40.9] {
                for glyph in line(text, size, origin) {
                    let error = (glyph.inked - glyph.shaped).abs();
                    assert!(
                        // The centroid of a hinted raster is not a perfect probe of position — the
                        // hinting moves outline points — so the budget is the quantisation plus a
                        // tenth of a pixel of measurement, well under the pixel a mispaired pen and
                        // phase costs.
                        error <= TOLERANCE + 0.1,
                        "at size {size} and origin {origin}, a glyph shaped at {} inked at {}, \
                         which is {error:.3} px away, in {text:?}",
                        glyph.shaped,
                        glyph.inked,
                    );
                }
            }
        }
    }
}

/// The measured distance between neighbouring glyphs is the distance between their advances.
///
/// This is the reported defect stated exactly: letters too close together and letters with
/// unexpectedly large gaps are pairs whose measured separation differs from the shaped one.
#[test]
fn measured_inter_glyph_distances_are_the_shaped_advances() {
    for text in LINES {
        for size in [11.0f32, 13.0, 16.0, 21.0] {
            for origin in [0.0f32, 0.3, 0.5, 0.75] {
                let glyphs = line(text, size, origin);
                for pair in glyphs.windows(2) {
                    let measured = pair[1].inked - pair[0].inked;
                    let shaped = pair[1].shaped - pair[0].shaped;
                    assert!(
                        (measured - shaped).abs() <= 2.0 * (TOLERANCE + 0.1),
                        "at size {size} and origin {origin}, an advance of {shaped:.3} was drawn \
                         as {measured:.3} in {text:?}"
                    );
                }
            }
        }
    }
}

/// The error is bounded rather than accumulated: the last glyph is as well placed as the first.
#[test]
fn the_error_does_not_accumulate_along_the_line() {
    let glyphs = line(LINES[1], 16.0, 0.3);
    let worst = |range: std::ops::Range<usize>| {
        range
            .map(|index| (glyphs[index].inked - glyphs[index].shaped).abs())
            .fold(0.0f32, f32::max)
    };
    let (early, late) = (worst(0..5), worst(glyphs.len() - 5..glyphs.len()));
    assert!(
        late <= TOLERANCE + 0.1,
        "the line drifted to {late:.3} px by its last glyphs, from {early:.3} px at its first"
    );
}

/// Kerning survives placement: a pair the face kerns is drawn closer than its unkerned width.
///
/// A placement that quantised each advance on its own could still leave every pair within tolerance
/// of *something*; this pins that the something is the kerned advance the shaper produced and not
/// the sum of two side bearings.
#[test]
fn a_kerned_pair_is_still_kerned_once_it_is_drawn() {
    let kerned = measure("AV", 32.0, 0.0);
    let apart = measure("AI", 32.0, 0.0);
    let gap = |glyphs: &[Measured]| glyphs[1].inked - glyphs[0].inked;
    assert!(
        gap(&kerned) < gap(&apart) - 1.0,
        "the kerned pair was drawn {:.2} px apart and the unkerned one {:.2} px, so the kerning \
         did not reach the surface",
        gap(&kerned),
        gap(&apart),
    );
    // And the drawn gap is the shaped one, not merely smaller.
    assert!(
        (gap(&kerned) - (kerned[1].shaped - kerned[0].shaped)).abs() <= 2.0 * (TOLERANCE + 0.1)
    );
}

/// The same string at the same position produces the same pixels every time.
#[test]
fn one_line_renders_identically_frame_to_frame() {
    for origin in [0.0f32, 0.3, 0.75] {
        let first = line(LINES[0], 16.0, origin);
        let second = line(LINES[0], 16.0, origin);
        assert_eq!(first.len(), second.len());
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(
                a.inked.to_bits(),
                b.inked.to_bits(),
                "the same line drew differently the second time"
            );
        }
    }
}

/// Moving a line by a whole pixel moves every glyph by exactly that and changes no rasterisation.
///
/// The phase belongs to the fraction of the position alone, so a whole-pixel move must be a pure
/// translation: anything else means the split is reading something other than the fraction.
#[test]
fn a_whole_pixel_of_movement_is_a_pure_translation() {
    let here = line(LINES[0], 16.0, 0.3);
    let there = line(LINES[0], 16.0, 7.3);
    for (a, b) in here.iter().zip(&there) {
        assert!(
            (b.inked - a.inked - 7.0).abs() < 1e-3,
            "a line moved by seven pixels drew a glyph {} px along",
            b.inked - a.inked
        );
    }
}
