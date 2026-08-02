//! Ramps, and the space each one is walked in.
//!
//! A ramp asked for in Oklab is not a straight line in sRGB, and drawing it as one is the
//! difference between a blue-to-yellow gradient that stays saturated and one that passes through
//! grey. Three spaces are walked in the shader because each converts back with a handful of
//! arithmetic; every other space is approximated on the processor by adding stops along the true
//! curve. Both mechanisms are checked here against the same reference implementation of the
//! interpolation itself.

mod support;

use zgui_bits::DamageSet;
use zgui_color::{Color, ColorSpace, GradientStop, HueInterpolation, Interpolation, interpolate};
use zgui_geom::{DevicePx, Point, Size};
use zgui_scene::{GradientKind, Paint, Quad, Scene};

use support::{SIDE, plain_renderer, present, rect};

/// The two ends of every ramp here: blue to yellow, which is the pair the spaces disagree most on.
const START: Color = Color::srgb(0.0, 0.0, 1.0, 1.0);
/// The far end.
const END: Color = Color::srgb(1.0, 1.0, 0.0, 1.0);

/// Draws a horizontal ramp across the surface in `space` and reads its midpoint back.
fn midpoint(space: ColorSpace) -> Option<[u8; 4]> {
    let mut renderer = plain_renderer()?;
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene.paints.add(Paint::Gradient {
        kind: GradientKind::Linear {
            start: Point::new(DevicePx(0.0), DevicePx(0.0)),
            end: Point::new(DevicePx(SIDE as f32), DevicePx(0.0)),
        },
        stops: [GradientStop::new(0.0, START), GradientStop::new(1.0, END)]
            .into_iter()
            .collect(),
        space,
        hue: HueInterpolation::Shorter,
        repeating: false,
    });
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        paint,
    ));
    scene.finish(&DamageSet::full());
    Some(present(&mut renderer, &scene).rgba(SIDE / 2, SIDE / 2))
}

/// What the midpoint of a ramp in `space` should be, computed with no device at all.
fn expected(space: ColorSpace) -> [u8; 4] {
    let colour = interpolate(START, END, 0.5, Interpolation::new(space));
    let [red, green, blue, alpha] = colour.to_premultiplied_srgb();
    [
        (red * 255.0).round() as u8,
        (green * 255.0).round() as u8,
        (blue * 255.0).round() as u8,
        (alpha * 255.0).round() as u8,
    ]
}

/// The largest per-channel difference between two colours.
fn difference(left: [u8; 4], right: [u8; 4]) -> u8 {
    (0..4)
        .map(|channel| left[channel].abs_diff(right[channel]))
        .max()
        .unwrap_or(0)
}

#[test]
fn a_ramp_walked_in_the_shader_matches_the_interpolation_it_names() {
    // The three spaces the shader converts back from itself.
    for space in [ColorSpace::Srgb, ColorSpace::Oklab, ColorSpace::SrgbLinear] {
        let Some(drawn) = midpoint(space) else {
            return;
        };
        let wanted = expected(space);
        assert!(
            difference(drawn, wanted) <= 2,
            "{space:?}: drew {drawn:?}, expected {wanted:?}"
        );
    }
}

#[test]
fn a_ramp_in_a_space_the_shader_cannot_walk_is_approximated_to_within_a_step() {
    // Everything else is densified on the processor into stops the shader can walk in sRGB, to
    // within an eight-bit step of the true curve. Oklch is the interesting one: it is polar, so a
    // straight line in it is an arc anywhere else.
    for space in [ColorSpace::Oklch, ColorSpace::Lab, ColorSpace::Hsl] {
        let Some(drawn) = midpoint(space) else {
            return;
        };
        let wanted = expected(space);
        assert!(
            difference(drawn, wanted) <= 3,
            "{space:?}: drew {drawn:?}, expected {wanted:?}"
        );
    }
}

#[test]
fn the_spaces_genuinely_disagree_about_the_midpoint() {
    // Without this, the two assertions above would both pass on a renderer that ignored the space
    // entirely and interpolated in sRGB throughout.
    let (Some(srgb), Some(oklab)) = (midpoint(ColorSpace::Srgb), midpoint(ColorSpace::Oklab))
    else {
        return;
    };
    assert!(
        difference(srgb, oklab) > 20,
        "an Oklab ramp is not an sRGB ramp: {srgb:?} against {oklab:?}"
    );
}

#[test]
fn a_ramp_runs_from_one_end_to_the_other_across_the_shape_it_fills() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene.paints.add(Paint::Gradient {
        kind: GradientKind::Linear {
            start: Point::new(DevicePx(0.0), DevicePx(0.0)),
            end: Point::new(DevicePx(SIDE as f32), DevicePx(0.0)),
        },
        stops: [GradientStop::new(0.0, START), GradientStop::new(1.0, END)]
            .into_iter()
            .collect(),
        space: ColorSpace::Srgb,
        hue: HueInterpolation::Shorter,
        repeating: false,
    });
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        paint,
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    // A fragment is sampled at the centre of its pixel, so the first and last columns are half a
    // pixel inside the ramp rather than at its ends.
    assert!(
        difference(pixels.rgba(0, 64), [0, 0, 255, 255]) <= 4,
        "the near end: {:?}",
        pixels.rgba(0, 64)
    );
    assert!(
        difference(pixels.rgba(SIDE - 1, 64), [255, 255, 0, 255]) <= 4,
        "the far end: {:?}",
        pixels.rgba(SIDE - 1, 64)
    );
    let row: Vec<u8> = (0..SIDE).map(|x| pixels.rgba(x, 64)[0]).collect();
    for pair in row.windows(2) {
        assert!(pair[0] <= pair[1], "the ramp is monotone: {row:?}");
    }
}

/// A ramp's geometry is device space, so the same paint under two shapes is the same ramp.
#[test]
fn a_radial_ramp_is_centred_where_its_geometry_says_and_not_where_its_shape_is() {
    let Some(mut renderer) = plain_renderer() else {
        return;
    };
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    let paint = scene.paints.add(Paint::Gradient {
        kind: GradientKind::Radial {
            center: Point::new(DevicePx(32.0), DevicePx(32.0)),
            radius_x: 32.0,
            radius_y: 32.0,
        },
        stops: [GradientStop::new(0.0, START), GradientStop::new(1.0, END)]
            .into_iter()
            .collect(),
        space: ColorSpace::Srgb,
        hue: HueInterpolation::Shorter,
        repeating: false,
    });
    scene.push_quad(Quad::filled(
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        paint,
    ));
    scene.finish(&DamageSet::full());
    let pixels = present(&mut renderer, &scene);

    assert!(
        difference(pixels.rgba(32, 32), [0, 0, 255, 255]) <= 8,
        "the centre: {:?}",
        pixels.rgba(32, 32)
    );
    assert_eq!(
        pixels.rgba(96, 96),
        [255, 255, 0, 255],
        "past the far radius the ramp is clamped"
    );
}

/// A ramp lives in a side table addressed by index, so the instance that uses it carries an index
/// and not a ramp. This is what says so: the same quad filled two ways is the same number of bytes
/// in the buffer, however many stops the ramp has.
#[test]
fn a_gradient_costs_a_quad_no_more_instance_bytes_than_a_flat_colour() {
    /// One quad filling the surface, painted by `paint`.
    fn quad_bytes(paint: Paint) -> usize {
        let mut scene = Scene::new();
        scene.begin_frame(Size::new(SIDE, SIDE));
        let id = scene.paints.add(paint);
        scene.push_quad(Quad::filled(rect(0.0, 0.0, SIDE as f32, SIDE as f32), id));
        scene.finish(&DamageSet::full());
        size_of_val(&scene.primitives.quads[..])
    }

    let stops: Vec<GradientStop> = (0..16)
        .map(|step| GradientStop::new(step as f32 / 15.0, START))
        .collect();
    let ramp = Paint::Gradient {
        kind: GradientKind::Linear {
            start: Point::new(DevicePx(0.0), DevicePx(0.0)),
            end: Point::new(DevicePx(SIDE as f32), DevicePx(0.0)),
        },
        stops: stops.into_iter().collect(),
        space: ColorSpace::Srgb,
        hue: HueInterpolation::Shorter,
        repeating: false,
    };
    assert_eq!(
        quad_bytes(ramp),
        quad_bytes(Paint::Solid(START)),
        "a sixteen-stop ramp costs its quad exactly what a flat colour does"
    );
    // And a quad is one fixed-size record, so the equality above is about the ramp rather than
    // about two empty buffers.
    assert_eq!(quad_bytes(Paint::Solid(START)), size_of::<Quad>());
}
