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

use zgui_css::values::length::evaluate_at;

use crate::lower::background::{
    Extent, GradientCenter, GradientShape, GradientSpec, RadialEnding, resolved_stops,
};

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
    let (mut kind, length) = geometry(&spec.shape, bounds, scale);
    let mut stops = resolved_stops(spec, length);
    if spec.repeating {
        period(&mut kind, &mut stops);
    }
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

/// Fits a repeating ramp to its period: the span from the first stop to the last. The rasteriser
/// repeats the ramp over the whole parameter range, so the line is cut to the span and the stops
/// are spread over it. A radial or conic ramp whose first stop is past zero keeps its geometry.
fn period(kind: &mut GradientKind, stops: &mut smallvec::SmallVec<[zgui_color::GradientStop; 4]>) {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return;
    };
    let (first, last) = (first.offset, last.offset);
    let span = last - first;
    if span <= f32::EPSILON || (first == 0.0 && last == 1.0) {
        return;
    }
    match kind {
        GradientKind::Linear { start, end } => {
            let (dx, dy) = (end.x.0 - start.x.0, end.y.0 - start.y.0);
            let from = Point::new(
                DevicePx(start.x.0 + dx * first),
                DevicePx(start.y.0 + dy * first),
            );
            *end = Point::new(
                DevicePx(start.x.0 + dx * last),
                DevicePx(start.y.0 + dy * last),
            );
            *start = from;
        }
        GradientKind::Radial {
            radius_x, radius_y, ..
        } if first == 0.0 => {
            *radius_x *= last;
            *radius_y *= last;
        }
        _ => return,
    }
    for stop in stops.iter_mut() {
        stop.offset = (stop.offset - first) / span;
    }
}

/// Where a gradient's ramp runs across `bounds`, and how long its line is in CSS pixels.
///
/// The line's length is what a stop written as a length is a fraction of, which is why it is
/// returned rather than recomputed: a stop at `20px` along a 200-pixel line is at a tenth, and a
/// second derivation of that number is a second chance to disagree with the geometry.
fn geometry(
    shape: &GradientShape,
    bounds: Rect<DevicePx, Device>,
    scale: f32,
) -> (GradientKind, CssPx) {
    let width = bounds.size.width.0;
    let height = bounds.size.height.0;
    let scale = scale.max(f32::MIN_POSITIVE);
    match shape {
        GradientShape::Linear { angle } => {
            let center: Point<DevicePx, Device> = Point::new(
                DevicePx(bounds.origin.x.0 + width * 0.5),
                DevicePx(bounds.origin.y.0 + height * 0.5),
            );
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
            (GradientKind::Linear { start, end }, CssPx(length / scale))
        }
        GradientShape::Radial { ending, at } => {
            let (x, y) = offset(at, width, height, scale);
            let (radius_x, radius_y) = radii(ending, (x, y), (width, height), scale);
            (
                GradientKind::Radial {
                    center: Point::new(
                        DevicePx(bounds.origin.x.0 + x),
                        DevicePx(bounds.origin.y.0 + y),
                    ),
                    radius_x,
                    radius_y,
                },
                // The gradient ray runs right from the centre, so a stop written as a length is a
                // fraction of the horizontal radius.
                CssPx(radius_x / scale),
            )
        }
        GradientShape::Conic { from_angle, at } => {
            let (x, y) = offset(at, width, height, scale);
            let center = Point::new(
                DevicePx(bounds.origin.x.0 + x),
                DevicePx(bounds.origin.y.0 + y),
            );
            (
                GradientKind::Conic {
                    center,
                    from_angle: *from_angle,
                },
                // A conic ramp's parameter is an angle, so a stop written as a length has no meaning
                // along it; one turn is the whole ramp and that is what a fraction is taken of.
                CssPx(1.0),
            )
        }
    }
}

/// A radius that stands in for zero, in device pixels.
///
/// CSS draws a degenerate ending shape as one of a very small size, so that the ramp keeps its
/// direction.
const TINY_RADIUS: f32 = 1e-3;

/// A radius that stands in for an unbounded one, in device pixels.
const HUGE_RADIUS: f32 = 1e6;

/// The centre of a gradient as an offset from the box's top left corner, in device pixels.
fn offset(at: &GradientCenter, width: f32, height: f32, scale: f32) -> (f32, f32) {
    (
        evaluate_at(&at.x, CssPx(width / scale)).0 * scale,
        evaluate_at(&at.y, CssPx(height / scale)).0 * scale,
    )
}

/// The radii of a radial gradient's ending shape, in device pixels, per CSS Images 3.
///
/// `center` is the offset from the box's top left corner and `size` the box's size.
fn radii(ending: &RadialEnding, center: (f32, f32), size: (f32, f32), scale: f32) -> (f32, f32) {
    let (x, y) = center;
    let (width, height) = size;
    // The distances from the centre to the nearer and the farther side, along each axis.
    let (left, right) = (x.abs(), (width - x).abs());
    let (top, bottom) = (y.abs(), (height - y).abs());
    let near = (left.min(right), top.min(bottom));
    let far = (left.max(right), top.max(bottom));
    let (radius_x, radius_y) = match *ending {
        RadialEnding::CircleRadius(radius) => {
            let radius = radius.0 * scale;
            (radius, radius)
        }
        RadialEnding::CircleExtent(extent) => {
            let radius = match extent {
                Extent::ClosestSide => near.0.min(near.1),
                Extent::FarthestSide => far.0.max(far.1),
                Extent::ClosestCorner => near.0.hypot(near.1),
                Extent::FarthestCorner => far.0.hypot(far.1),
            };
            (radius, radius)
        }
        RadialEnding::EllipseRadii(ref radius_x, ref radius_y) => (
            evaluate_at(radius_x, CssPx(width / scale)).0 * scale,
            evaluate_at(radius_y, CssPx(height / scale)).0 * scale,
        ),
        RadialEnding::EllipseExtent(extent) => match extent {
            Extent::ClosestSide => near,
            Extent::FarthestSide => far,
            // The ellipse through the corner keeps the aspect ratio of the matching side keyword.
            // Through the corner `(dx, dy)` with sides `(dx, dy)`, that is the sides scaled by √2.
            Extent::ClosestCorner => (
                near.0 * core::f32::consts::SQRT_2,
                near.1 * core::f32::consts::SQRT_2,
            ),
            Extent::FarthestCorner => (
                far.0 * core::f32::consts::SQRT_2,
                far.1 * core::f32::consts::SQRT_2,
            ),
        },
    };
    let circle = matches!(
        ending,
        RadialEnding::CircleRadius(_) | RadialEnding::CircleExtent(_)
    );
    degenerate(radius_x, radius_y, circle)
}

/// Radii with the degenerate cases of CSS Images 3 replaced.
///
/// A circle of zero radius: a very small circle. Zero width: very narrow and very tall, so the
/// ramp runs horizontally. Zero height only: very wide and very flat, so the ramp runs vertically.
fn degenerate(radius_x: f32, radius_y: f32, circle: bool) -> (f32, f32) {
    // A NaN radius compares false and is degenerate too.
    match (radius_x > 0.0, radius_y > 0.0) {
        (true, true) => (radius_x, radius_y),
        (false, _) if circle => (TINY_RADIUS, TINY_RADIUS),
        (false, _) => (TINY_RADIUS, HUGE_RADIUS),
        (true, false) => (HUGE_RADIUS, TINY_RADIUS),
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

    use zgui_css::values::length::{Length, LengthPercentage, percent};
    use zgui_geom::CssPx;

    use super::{geometry, needs_densifying, period};
    use crate::lower::background::{
        Extent, GradientCenter, GradientShape, GradientSpec, RadialEnding,
    };

    /// A centre at the given fractions of the box.
    fn at(x: f32, y: f32) -> GradientCenter {
        GradientCenter {
            x: percent(x),
            y: percent(y),
        }
    }

    /// The centre and radii a radial gradient resolves to over [`bounds`], and its ray length.
    fn radial(ending: RadialEnding, at: GradientCenter) -> ((f32, f32), (f32, f32), f32) {
        let (kind, length) = geometry(&GradientShape::Radial { ending, at }, bounds(), 1.0);
        let GradientKind::Radial {
            center,
            radius_x,
            radius_y,
        } = kind
        else {
            panic!("a radial gradient stays radial");
        };
        ((center.x.0, center.y.0), (radius_x, radius_y), length.0)
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn the_initial_radial_gradient_is_the_farthest_corner_ellipse_at_the_centre() {
        let (center, (rx, ry), length) = radial(
            RadialEnding::EllipseExtent(Extent::FarthestCorner),
            GradientCenter::middle(),
        );
        assert_eq!(center, (100.0, 50.0));
        assert!(close(rx, 100.0 * core::f32::consts::SQRT_2), "{rx}");
        assert!(close(ry, 50.0 * core::f32::consts::SQRT_2), "{ry}");
        assert!(
            close(length, rx),
            "the ray runs along the horizontal radius"
        );
    }

    #[test]
    fn a_circle_at_the_top_left_corner_reaches_the_farthest_corner() {
        let (center, (rx, ry), _) = radial(
            RadialEnding::CircleExtent(Extent::FarthestCorner),
            at(0.0, 0.0),
        );
        assert_eq!(center, (0.0, 0.0));
        assert!(close(rx, 200.0f32.hypot(100.0)) && rx == ry, "{rx} {ry}");
    }

    #[test]
    fn the_extent_keywords_measure_from_the_centre() {
        // A centre at (50, 25): sides at 50 and 150 across, 25 and 75 down.
        let center = at(0.25, 0.25);
        let (_, circle, _) = radial(
            RadialEnding::CircleExtent(Extent::ClosestSide),
            center.clone(),
        );
        assert_eq!(circle, (25.0, 25.0));
        let (_, circle, _) = radial(
            RadialEnding::CircleExtent(Extent::FarthestSide),
            center.clone(),
        );
        assert_eq!(circle, (150.0, 150.0));
        let (_, (r, _), _) = radial(
            RadialEnding::CircleExtent(Extent::ClosestCorner),
            center.clone(),
        );
        assert!(close(r, 50.0f32.hypot(25.0)), "{r}");
        let (_, ellipse, _) = radial(
            RadialEnding::EllipseExtent(Extent::ClosestSide),
            center.clone(),
        );
        assert_eq!(ellipse, (50.0, 25.0));
        let (_, ellipse, _) = radial(
            RadialEnding::EllipseExtent(Extent::FarthestSide),
            center.clone(),
        );
        assert_eq!(ellipse, (150.0, 75.0));
        // Through the nearest corner (50, 25), with the closest-side aspect ratio.
        let (_, (rx, ry), _) = radial(RadialEnding::EllipseExtent(Extent::ClosestCorner), center);
        assert!(close(
            50.0f32.powi(2) / rx.powi(2) + 25.0f32.powi(2) / ry.powi(2),
            1.0
        ));
        assert!(close(rx / ry, 2.0));
    }

    #[test]
    fn explicit_radii_resolve_against_the_box() {
        let (_, radii, length) = radial(
            RadialEnding::EllipseRadii(
                percent(0.1),
                LengthPercentage::new_length(Length::new(30.0)),
            ),
            GradientCenter::middle(),
        );
        assert_eq!(radii, (20.0, 30.0));
        assert_eq!(length, 20.0);
        let (_, radii, _) = radial(RadialEnding::CircleRadius(CssPx(12.0)), at(1.0, 1.0));
        assert_eq!(radii, (12.0, 12.0));
    }

    #[test]
    fn a_degenerate_ending_shape_keeps_its_direction() {
        let (_, (rx, ry), _) = radial(RadialEnding::CircleRadius(CssPx(0.0)), at(0.5, 0.5));
        assert!(
            rx > 0.0 && rx == ry && rx < 0.01,
            "a very small circle: {rx}"
        );
        // A centre on the left edge: the closest side is at zero across.
        let (_, (rx, ry), _) = radial(
            RadialEnding::EllipseExtent(Extent::ClosestSide),
            at(0.0, 0.5),
        );
        assert!(
            rx > 0.0 && rx < 0.01 && ry > 1e5,
            "very narrow and very tall: {rx} {ry}"
        );
    }

    #[test]
    fn a_conic_gradient_sweeps_around_its_centre() {
        let (kind, _) = geometry(
            &GradientShape::Conic {
                from_angle: 0.5,
                at: at(0.25, 1.0),
            },
            bounds(),
            2.0,
        );
        let GradientKind::Conic { center, from_angle } = kind else {
            panic!("a conic gradient stays conic");
        };
        assert_eq!((center.x.0, center.y.0), (50.0, 100.0));
        assert_eq!(from_angle, 0.5);
    }

    #[test]
    fn a_repeating_ramp_runs_over_its_stop_span() {
        use smallvec::smallvec;
        use zgui_color::{Color, GradientStop};

        let mut kind = GradientKind::Linear {
            start: Point::new(DevicePx(0.0), DevicePx(0.0)),
            end: Point::new(DevicePx(160.0), DevicePx(0.0)),
        };
        let black = Color::BLACK;
        let mut stops: smallvec::SmallVec<[GradientStop; 4]> = smallvec![
            GradientStop::new(0.0, black),
            GradientStop::new(0.05, black),
            GradientStop::new(0.1, black),
        ];
        period(&mut kind, &mut stops);
        let GradientKind::Linear { start, end } = kind else {
            panic!("the ramp stays linear");
        };
        assert_eq!((start.x.0, end.x.0), (0.0, 16.0));
        let offsets: Vec<f32> = stops.iter().map(|it| it.offset).collect();
        assert_eq!(offsets, [0.0, 0.5, 1.0]);
    }

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
            &GradientShape::Linear {
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
            &GradientShape::Linear {
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
            &GradientShape::Linear {
                angle: core::f32::consts::PI,
            },
            bounds(),
            1.0,
        );
        let (_, at_two) = geometry(
            &GradientShape::Linear {
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
