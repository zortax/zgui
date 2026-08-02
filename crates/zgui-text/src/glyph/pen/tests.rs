//! The pen and the phase describe one position, at every position.

use zgui_geom::{DevicePx, Point, Size};

use crate::glyph::key::SubpixelOffset;
use crate::glyph::pen::PenPosition;

/// Half a quantisation step, which is the most a position may move.
const TOLERANCE: f32 = 0.5 / SubpixelOffset::STEPS as f32;

/// The two halves never describe different pixels, however the fraction falls.
///
/// The failure this rules out is the one a floored pen and a rounded phase produce: at a fraction
/// above seven eighths the phase wraps to zero while the floor stays on the pixel below, so the
/// tile is drawn very nearly a whole pixel to the left of the position that was asked for. Sweeping
/// the fraction in thousandths reaches that band from both sides.
#[test]
fn the_pen_and_the_phase_agree_at_every_fraction() {
    for whole in [-3.0f32, 0.0, 7.0, 1024.0] {
        for thousandth in 0..1000 {
            let position = whole + thousandth as f32 / 1000.0;
            let split = PenPosition::of(position);
            assert_eq!(
                split.quantised(),
                split.pen() + split.offset().to_pixels(),
                "the halves of {position} do not compose"
            );
            assert!(
                (split.quantised() - position).abs() <= TOLERANCE + 1e-4,
                "{position} was moved to {} — further than one half step",
                split.quantised()
            );
            assert!(
                split.pen().fract() == 0.0,
                "a tile drawn at {} is a tile resampled",
                split.pen()
            );
        }
    }
}

/// A fraction that rounds up to the next pixel takes the pen with it.
#[test]
fn a_fraction_that_rounds_up_moves_the_pen_rather_than_wrapping_alone() {
    let split = PenPosition::of(12.9);
    assert_eq!(split.offset(), SubpixelOffset(0));
    assert_eq!(
        split.pen(),
        13.0,
        "the phase wrapped to zero and the pen was left on the pixel below, which is the whole \
         defect: the glyph lands 0.9 px early"
    );
}

/// Error does not accumulate: the hundredth glyph of a run is as well placed as the first.
///
/// A pipeline that advanced the pen by a rounded advance per glyph rather than splitting each
/// absolute position drifts without bound, and an advance of a third of a pixel is what finds it.
#[test]
fn the_error_does_not_grow_along_a_run() {
    for advance in [7.333_33f32, 9.1, 4.05, 11.875] {
        let mut worst_early = 0.0f32;
        let mut worst_late = 0.0f32;
        for index in 0..500 {
            let position = index as f32 * advance;
            let error = PenPosition::of(position).quantised() - position;
            if index < 10 {
                worst_early = worst_early.max(error.abs());
            } else {
                worst_late = worst_late.max(error.abs());
            }
        }
        assert!(
            worst_late <= TOLERANCE + 1e-3,
            "an advance of {advance} drifted to {worst_late} px by the five hundredth glyph, from \
             {worst_early} px at the tenth"
        );
    }
}

/// The phase is in the tile's pixels, so the rectangle it is drawn into is on whole pixels.
#[test]
fn the_phase_is_not_added_to_the_rectangle_as_well() {
    let bounds = PenPosition::of(10.5).bounds(
        20.0,
        Point::new(DevicePx(1.0), DevicePx(8.0)),
        Size::new(6, 9),
    );
    assert_eq!(
        bounds.origin.x,
        DevicePx(11.0),
        "adding the half pixel here as well as in the raster shifts the glyph twice"
    );
    assert_eq!(bounds.origin.y, DevicePx(12.0), "20 - 8");
}

/// A baseline at a fraction of a pixel still puts the tile on the pixel grid.
#[test]
fn a_fractional_baseline_is_rounded_rather_than_carried_into_the_rectangle() {
    let bounds = PenPosition::of(4.0).bounds(
        20.4,
        Point::new(DevicePx(0.0), DevicePx(8.0)),
        Size::new(6, 9),
    );
    assert_eq!(
        bounds.origin.y,
        DevicePx(12.0),
        "a tile whose edges are not on the grid is resampled by whatever draws it"
    );
}
