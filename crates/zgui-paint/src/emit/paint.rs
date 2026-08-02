//! Resolving a lowered gradient against the box it fills.
//!
//! This is the half of a gradient that could not be lowered: where its line runs, how long that
//! line is, and therefore where a stop written as a length falls. All three are properties of the
//! box, so they are settled here and the result is interned by content — two boxes of the same size
//! with the same gradient share one entry, which is what a table of a thousand identical rows
//! depends on.

use zgui_color::ColorSpace;
use zgui_geom::{CssPx, Device, DevicePx, Point, Rect};
use zgui_scene::{GradientKind, Paint, PaintRef, Scene};

use crate::lower::background::{GradientShape, GradientSpec, resolved_stops};

/// The paint one gradient layer becomes when it fills `bounds`, or nothing when it has no stops.
///
/// A gradient with a single stop is a flat colour and is interned as one, because a rasteriser
/// handed a one-stop ramp has no interval to interpolate over.
pub fn gradient_paint(
    scene: &mut Scene,
    spec: &GradientSpec,
    bounds: Rect<DevicePx, Device>,
    scale: f32,
) -> Option<PaintRef> {
    let (kind, length) = geometry(spec.shape, bounds, scale);
    let stops = resolved_stops(spec, length);
    match stops.len() {
        0 => None,
        1 => Some(scene.paints.add(Paint::Solid(stops[0].color))),
        _ => Some(scene.paints.add(Paint::Gradient {
            kind,
            stops,
            space: spec.interpolation.space,
            hue: spec.interpolation.hue,
            repeating: spec.repeating,
        })),
    }
}

/// Where a gradient's ramp runs across `bounds`, and how long its line is in CSS pixels.
///
/// The line's length is what a stop written as a length is a fraction of, which is why it is
/// returned rather than recomputed: a stop at `20px` along a 200-pixel line is at a tenth, and a
/// second derivation of that number is a second chance to disagree with the geometry.
fn geometry(
    shape: GradientShape,
    bounds: Rect<DevicePx, Device>,
    scale: f32,
) -> (GradientKind, CssPx) {
    let center = Point::new(
        DevicePx(bounds.origin.x.0 + bounds.size.width.0 * 0.5),
        DevicePx(bounds.origin.y.0 + bounds.size.height.0 * 0.5),
    );
    let width = bounds.size.width.0;
    let height = bounds.size.height.0;
    match shape {
        GradientShape::Linear { angle } => {
            // CSS measures the gradient line as the projection of the box onto the line's
            // direction, which is what makes `to bottom right` end exactly at the corner.
            let (sin, cos) = angle.sin_cos();
            let length = (width * sin).abs() + (height * cos).abs();
            let half = length * 0.5;
            let start = Point::new(
                DevicePx(center.x.0 - sin * half),
                DevicePx(center.y.0 + cos * half),
            );
            let end = Point::new(
                DevicePx(center.x.0 + sin * half),
                DevicePx(center.y.0 - cos * half),
            );
            (
                GradientKind::Linear { start, end },
                CssPx(length / scale.max(f32::MIN_POSITIVE)),
            )
        }
        GradientShape::Radial => {
            // `farthest-corner`, which is the initial extent: the ramp ends where the ellipse
            // through the farthest corner does.
            let radius_x = width * core::f32::consts::SQRT_2 * 0.5;
            let radius_y = height * core::f32::consts::SQRT_2 * 0.5;
            (
                GradientKind::Radial {
                    center,
                    radius_x,
                    radius_y,
                },
                CssPx(radius_x.max(radius_y) / scale.max(f32::MIN_POSITIVE)),
            )
        }
        GradientShape::Conic { from_angle } => (
            GradientKind::Conic { center, from_angle },
            // A conic ramp's parameter is an angle, so a stop written as a length has no meaning
            // along it; one turn is the whole ramp and that is what a fraction is taken of.
            CssPx(1.0),
        ),
    }
}

/// Whether a gradient's ramp has to be resolved into sRGB stops before a rasteriser can draw it.
///
/// Some rasterisers interpolate between the stops they are given, in sRGB, and cannot be told to
/// walk a ramp in Oklab or the long way round a hue circle. A caller drawing through one asks this
/// and densifies when the answer is yes; a caller whose shader interpolates in the named space
/// passes the stops through untouched, which keeps a two-stop gradient two stops.
pub fn needs_densifying(spec: &GradientSpec) -> bool {
    spec.interpolation.space != ColorSpace::Srgb
        || spec.interpolation.hue != zgui_color::HueInterpolation::Shorter
}

#[cfg(test)]
mod tests {
    use zgui_color::{ColorSpace, HueInterpolation, Interpolation};
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_scene::GradientKind;

    use super::{geometry, needs_densifying};
    use crate::lower::background::{GradientShape, GradientSpec};

    /// A 200 by 100 box at the origin.
    fn bounds() -> Rect<DevicePx, Device> {
        Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(200.0), DevicePx(100.0)),
        )
    }

    /// A gradient with no stops, in the given interpolation.
    fn spec(interpolation: Interpolation) -> GradientSpec {
        GradientSpec {
            shape: GradientShape::Linear { angle: 0.0 },
            stops: smallvec::smallvec![],
            interpolation,
            repeating: false,
        }
    }

    #[test]
    fn a_gradient_to_bottom_runs_the_height_of_the_box() {
        let (kind, length) = geometry(
            GradientShape::Linear {
                angle: core::f32::consts::PI,
            },
            bounds(),
            1.0,
        );
        assert!((length.0 - 100.0).abs() < 1e-3, "{length:?}");
        let GradientKind::Linear { start, end } = kind else {
            panic!("a linear gradient stays linear");
        };
        assert!(
            (start.y.0 - 0.0).abs() < 1e-3,
            "starts at the top: {start:?}"
        );
        assert!(
            (end.y.0 - 100.0).abs() < 1e-3,
            "ends at the bottom: {end:?}"
        );
    }

    #[test]
    fn a_gradient_to_the_right_runs_the_width_of_the_box() {
        let (kind, length) = geometry(
            GradientShape::Linear {
                angle: core::f32::consts::FRAC_PI_2,
            },
            bounds(),
            1.0,
        );
        assert!((length.0 - 200.0).abs() < 1e-3, "{length:?}");
        let GradientKind::Linear { start, end } = kind else {
            panic!("a linear gradient stays linear");
        };
        assert!((start.x.0 - 0.0).abs() < 1e-3, "{start:?}");
        assert!((end.x.0 - 200.0).abs() < 1e-3, "{end:?}");
    }

    #[test]
    fn the_line_length_is_in_css_pixels_whatever_the_device_scale_is() {
        let (_, at_one) = geometry(
            GradientShape::Linear {
                angle: core::f32::consts::PI,
            },
            bounds(),
            1.0,
        );
        let (_, at_two) = geometry(
            GradientShape::Linear {
                angle: core::f32::consts::PI,
            },
            bounds(),
            2.0,
        );
        assert!((at_one.0 - 100.0).abs() < 1e-3, "{at_one:?}");
        assert!(
            (at_two.0 - 50.0).abs() < 1e-3,
            "a stop written as 20px is 20 CSS pixels at any scale, so the line halves: {at_two:?}"
        );
    }

    #[test]
    fn only_a_ramp_outside_srgb_has_to_be_densified() {
        assert!(!needs_densifying(&spec(Interpolation::new(
            ColorSpace::Srgb
        ))));
        assert!(needs_densifying(&spec(Interpolation::new(
            ColorSpace::Oklab
        ))));
        assert!(needs_densifying(&spec(
            Interpolation::new(ColorSpace::Hsl).with_hue(HueInterpolation::Longer)
        )));
    }
}
