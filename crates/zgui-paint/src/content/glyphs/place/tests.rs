//! Where the glyphs of a run actually land, against the positions shaping asked for.
//!
//! Every case here reconstructs the *effective* position of a glyph — the whole pixel its tile was
//! placed at, plus the phase its pixels were rasterised for, less the bearing the rasteriser
//! reported — and compares it with the position the run carried. That is the only quantity the eye
//! sees, and it is the one an implementation that quantises the phase and the pixel separately gets
//! wrong by nearly a whole pixel while every extent, tile and count stays right.

use std::sync::Mutex;

use zgui_atlas::{Atlas, AtlasLimits};
use zgui_geom::{DevicePx, Point, Size};
use zgui_scene::PaintSlot;
use zgui_text::{
    FaceId, GlyphFormat, GlyphImage, GlyphKey, GlyphRaster, RasterStyle, ShapedGlyph, ShapedRun,
    SubpixelOffset,
};

use crate::content::glyphs::cache::{GlyphCache, Rasterising};
use crate::emit::text::PlacedGlyph;

/// The bearing the fixture's rasteriser reports for every glyph.
const BEARING: f32 = 1.0;

/// Half a quantisation step: the most a glyph's position may move.
const TOLERANCE: f32 = 0.5 / SubpixelOffset::STEPS as f32;

/// A rasteriser that answers uniformly and records the keys it was asked for, in order.
#[derive(Default)]
struct Recording {
    /// Every key, in the order it arrived.
    asked: Mutex<Vec<GlyphKey>>,
}

impl GlyphRaster for Recording {
    fn raster(&self, key: &GlyphKey) -> Option<GlyphImage> {
        self.asked
            .lock()
            .expect("no panic held the lock")
            .push(*key);
        Some(GlyphImage {
            size: Size::new(5, 9),
            placement: Point::new(DevicePx(BEARING), DevicePx(7.0)),
            format: GlyphFormat::Mono,
            bytes: vec![255; 45],
        })
    }

    /// No curves: every case here is about the pen and the phase, which curves do not have.
    fn outline(&self, _key: &zgui_text::OutlineKey) -> Option<zgui_text::GlyphOutline> {
        None
    }
}

/// Places `positions` at `origin` and reports each glyph's rectangle beside the phase it was
/// rasterised for.
///
/// Every glyph is given a distinct index so that no two share a cache entry, which is what keeps
/// the recorded keys in step with the glyphs.
fn placed(origin: (f32, f32), positions: &[f32]) -> Vec<(PlacedGlyph, SubpixelOffset)> {
    let glyphs: Vec<ShapedGlyph> = positions
        .iter()
        .enumerate()
        .map(|(index, x)| ShapedGlyph {
            glyph: index as u16,
            x: *x,
            y: 20.0,
        })
        .collect();
    let run = ShapedRun {
        face: FaceId(1),
        size: 16.0,
        synthetic_bold: 0.0,
        synthetic_slant: 0.0,
        has_color: false,
        brush: PaintSlot(0),
        glyphs: &glyphs,
    };
    let raster = Recording::default();
    let mut cache = GlyphCache::default();
    let mut atlas = Atlas::new(AtlasLimits::default());
    let mut out = Vec::new();
    super::place(
        &mut Rasterising {
            glyphs: &mut cache,
            atlas: &mut atlas,
            named: Vec::new(),
        },
        &raster,
        &run,
        RasterStyle::Grayscale,
        Point::new(DevicePx(origin.0), DevicePx(origin.1)),
        &mut out,
    );
    let asked = raster.asked.lock().expect("no panic held the lock").clone();
    assert_eq!(out.len(), asked.len(), "one rasterisation per glyph");
    out.into_iter()
        .zip(asked.into_iter().map(|key| key.offset))
        .collect()
}

/// Where a placed glyph's ink actually starts, in absolute device pixels.
fn effective(glyph: &(PlacedGlyph, SubpixelOffset)) -> f32 {
    glyph.0.bounds.origin.x.0 - BEARING + glyph.1.to_pixels()
}

/// A tile is drawn on the pixel grid, whatever fraction the line box and the glyph sit at.
#[test]
fn every_rectangle_falls_on_whole_device_pixels() {
    for origin in [(0.0, 0.0), (10.5, 30.25), (7.3, 12.9)] {
        for glyph in placed(origin, &[0.0, 6.4, 13.1, 19.85, 26.6]) {
            assert_eq!(
                glyph.0.bounds.origin.x.0.fract(),
                0.0,
                "a coverage tile at {:?} is resampled by whatever draws it",
                glyph.0.bounds.origin
            );
            assert_eq!(glyph.0.bounds.origin.y.0.fract(), 0.0);
        }
    }
}

/// Each glyph lands within half a quantisation step of the position it was shaped at.
///
/// The line box is deliberately at a fraction of a pixel: a phase taken from the run-relative
/// position alone is the phase of a different number, and the glyph is rasterised for a shift it is
/// never drawn at.
#[test]
fn each_glyph_lands_where_its_own_position_asked() {
    for origin in [(0.0, 0.0), (10.5, 30.0), (7.3, 12.0), (100.75, 4.0)] {
        let positions = [0.0, 6.4, 13.1, 19.85, 26.6, 33.9, 41.05];
        for (glyph, x) in placed(origin, &positions).iter().zip(positions) {
            let wanted = origin.0 + x;
            assert!(
                (effective(glyph) - wanted).abs() <= TOLERANCE + 1e-4,
                "a glyph shaped at {wanted} was drawn at {}, which is {} px away",
                effective(glyph),
                (effective(glyph) - wanted).abs()
            );
        }
    }
}

/// The gap between two glyphs is the gap between their advances, to within the quantisation.
///
/// This is what the eye reads: a run whose every glyph is within a step of its own position still
/// looks even, and a run with one glyph a pixel out has one pair crowded and the next opened up.
#[test]
fn the_gap_between_two_glyphs_is_the_gap_between_their_advances() {
    // Fractions spread across the whole pixel, including the band above seven eighths that a
    // rounded phase wraps to zero while a floored pen stays on the pixel below.
    let positions: Vec<f32> = (0..64).map(|index| index as f32 * 6.9375).collect();
    for origin in [(0.0, 0.0), (10.5, 30.0), (7.3, 12.0)] {
        let glyphs = placed(origin, &positions);
        for (pair, advance) in glyphs.windows(2).zip(positions.windows(2)) {
            let measured = effective(&pair[1]) - effective(&pair[0]);
            let shaped = advance[1] - advance[0];
            assert!(
                (measured - shaped).abs() <= 2.0 * TOLERANCE + 1e-4,
                "an advance of {shaped} was drawn as {measured}"
            );
        }
    }
}

/// Error stays bounded along a run rather than growing with it.
#[test]
fn the_error_does_not_accumulate_along_a_run() {
    let positions: Vec<f32> = (0..400).map(|index| index as f32 * 7.3).collect();
    let glyphs = placed((3.5, 10.0), &positions);
    let worst = |range: std::ops::Range<usize>| {
        range
            .map(|index| (effective(&glyphs[index]) - (3.5 + positions[index])).abs())
            .fold(0.0f32, f32::max)
    };
    let (early, late) = (worst(0..20), worst(380..400));
    assert!(
        late <= TOLERANCE + 1e-4,
        "the run drifted to {late} px by its four hundredth glyph, from {early} px at its \
         twentieth"
    );
}

/// The same run placed twice is placed identically.
#[test]
fn the_same_run_is_placed_the_same_way_every_time() {
    let positions = [0.0, 6.4, 13.1, 19.85, 26.6];
    let first = placed((7.3, 12.0), &positions);
    let second = placed((7.3, 12.0), &positions);
    assert_eq!(
        first.iter().map(|glyph| glyph.0.bounds).collect::<Vec<_>>(),
        second
            .iter()
            .map(|glyph| glyph.0.bounds)
            .collect::<Vec<_>>(),
    );
}

/// A glyph whose fraction rounds up to the next pixel is drawn on that pixel.
///
/// The regression this pins: the phase wrapped to zero and the pen was left on the pixel below, so
/// this one glyph was drawn 0.9 px to the left of where it belongs while its neighbours were right.
#[test]
fn a_position_just_below_a_whole_pixel_is_not_dragged_back_a_pixel() {
    let glyph = &placed((0.0, 0.0), &[12.9])[0];
    assert_eq!(glyph.1, SubpixelOffset(0));
    assert_eq!(
        glyph.0.bounds.origin.x,
        DevicePx(13.0 + BEARING),
        "the tile went to pixel 12 while its pixels were rasterised for pixel 13"
    );
}

/// The vertical position of a run is one rounded baseline, shared by every glyph on it.
#[test]
fn a_line_of_glyphs_shares_one_rounded_baseline() {
    let glyphs = placed((0.0, 12.4), &[0.0, 6.4, 13.1]);
    for glyph in &glyphs {
        assert_eq!(
            glyph.0.bounds.origin.y,
            DevicePx((12.4f32 + 20.0).round() - 7.0),
        );
    }
}
