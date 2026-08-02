use proptest::prelude::*;

use crate::color::Color;
use crate::gradient::GradientStop;
use crate::gradient::densify::{DEFAULT_TOLERANCE, densify, densify_with_tolerance};
use crate::interpolate::{HueInterpolation, Interpolation, interpolate};
use crate::space::ColorSpace;

/// The error a densified ramp is judged by: one step of an eight-bit channel.
const ONE_EIGHTH_BIT_STEP: f32 = 1.0 / 255.0;

/// Samples the piecewise-linear ramp the densified stops describe, in premultiplied sRGB.
///
/// This is what a rasteriser does with the stops it is given, so measuring against it measures the
/// thing that reaches the screen rather than an intermediate.
fn evaluate(stops: &[GradientStop], offset: f32) -> [f32; 4] {
    let first = stops.first().expect("a ramp has at least one stop");
    if offset <= first.offset {
        return first.color.to_premultiplied_srgb();
    }
    let last = stops.last().expect("a ramp has at least one stop");
    if offset >= last.offset {
        return last.color.to_premultiplied_srgb();
    }

    let index = stops
        .windows(2)
        .position(|pair| offset >= pair[0].offset && offset <= pair[1].offset)
        .expect("the offset lies inside the ramp");
    let (start, end) = (stops[index], stops[index + 1]);
    let span = end.offset - start.offset;
    let t = if span > 0.0 {
        (offset - start.offset) / span
    } else {
        1.0
    };

    let start = start.color.to_premultiplied_srgb();
    let end = end.color.to_premultiplied_srgb();
    let mut sampled = [0.0; 4];
    for (channel, value) in sampled.iter_mut().enumerate() {
        *value = start[channel] + (end[channel] - start[channel]) * t;
    }
    sampled
}

/// The largest per-channel difference between the densified ramp and the true curve, over
/// `samples` evenly spaced positions.
fn worst_error(
    from: Color,
    to: Color,
    interpolation: Interpolation,
    stops: &[GradientStop],
    samples: usize,
) -> f32 {
    let mut worst: f32 = 0.0;
    for step in 0..samples {
        let offset = step as f32 / (samples - 1) as f32;
        let approximated = evaluate(stops, offset);
        let exact = interpolate(from, to, offset, interpolation).to_premultiplied_srgb();
        for channel in 0..4 {
            worst = worst.max((approximated[channel] - exact[channel]).abs());
        }
    }
    worst
}

/// A two-stop ramp from `from` to `to`.
fn ramp(from: Color, to: Color) -> [GradientStop; 2] {
    [GradientStop::new(0.0, from), GradientStop::new(1.0, to)]
}

#[test]
fn an_oklab_ramp_sampled_at_sixty_four_positions_is_within_one_step() {
    let from = Color::srgb(0.0, 0.0, 1.0, 1.0);
    let to = Color::srgb(1.0, 1.0, 0.0, 1.0);
    let interpolation = Interpolation::new(ColorSpace::Oklab);
    let stops = densify(&ramp(from, to), interpolation);

    let worst = worst_error(from, to, interpolation, &stops, 64);
    assert!(
        worst <= ONE_EIGHTH_BIT_STEP,
        "{} stops left an error of {worst}",
        stops.len(),
    );
    // Accuracy bought with unbounded stops would be no achievement: a ramp this long across the
    // hue circle is meant to cost a few dozen stops, not a few hundred.
    assert!(stops.len() <= 64, "{} stops is too many", stops.len());
}

#[test]
fn every_interpolation_space_stays_within_one_step() {
    let pairs = [
        (
            Color::srgb(0.0, 0.0, 1.0, 1.0),
            Color::srgb(1.0, 1.0, 0.0, 1.0),
        ),
        (Color::BLACK, Color::WHITE),
        (Color::srgb(1.0, 0.0, 0.0, 1.0), Color::TRANSPARENT),
        (
            Color::srgb(0.1, 0.7, 0.3, 0.25),
            Color::srgb(0.9, 0.2, 0.8, 1.0),
        ),
        (
            Color::new(ColorSpace::Oklch, [0.4, 0.2, 20.0], 1.0),
            Color::new(ColorSpace::Oklch, [0.9, 0.05, 300.0], 1.0),
        ),
    ];
    let arcs = [
        HueInterpolation::Shorter,
        HueInterpolation::Longer,
        HueInterpolation::Increasing,
        HueInterpolation::Decreasing,
    ];

    for space in ColorSpace::ALL {
        for arc in arcs {
            let interpolation = Interpolation::new(space).with_hue(arc);
            for (from, to) in pairs {
                let stops = densify(&ramp(from, to), interpolation);
                let worst = worst_error(from, to, interpolation, &stops, 257);
                assert!(
                    worst <= ONE_EIGHTH_BIT_STEP,
                    "{space:?} {arc:?}: {} stops left an error of {worst}",
                    stops.len(),
                );
            }
            if !space.is_polar() {
                break;
            }
        }
    }
}

#[test]
fn an_srgb_ramp_is_left_alone() {
    let stops = ramp(Color::BLACK, Color::WHITE);
    let densified = densify(&stops, Interpolation::new(ColorSpace::Srgb));
    assert_eq!(densified.len(), 2);
    assert_eq!(densified[0].color, Color::BLACK);
    assert_eq!(densified[1].color, Color::WHITE);
}

#[test]
fn a_ramp_between_similar_colours_needs_no_extra_stops() {
    let from = Color::srgb(0.50, 0.50, 0.50, 1.0);
    let to = Color::srgb(0.52, 0.50, 0.50, 1.0);
    let densified = densify(&ramp(from, to), Interpolation::new(ColorSpace::Oklab));
    assert_eq!(densified.len(), 2, "a near-straight curve was subdivided");
}

#[test]
fn the_authored_endpoints_are_reproduced_exactly() {
    let from = Color::new(ColorSpace::Lch, [40.0, 60.0, 20.0], 0.5);
    let to = Color::new(ColorSpace::Lch, [80.0, 20.0, 300.0], 1.0);
    let stops = densify(&ramp(from, to), Interpolation::new(ColorSpace::Lch));

    let start = stops.first().expect("stops are produced");
    let end = stops.last().expect("stops are produced");
    assert_eq!(start.offset, 0.0);
    assert_eq!(end.offset, 1.0);
    assert_eq!(start.color, from.to_space(ColorSpace::Srgb));
    assert_eq!(end.color, to.to_space(ColorSpace::Srgb));
}

#[test]
fn offsets_come_back_in_order_and_all_stops_are_srgb() {
    let stops = [
        GradientStop::new(0.0, Color::srgb(1.0, 0.0, 0.0, 1.0)),
        GradientStop::new(0.4, Color::srgb(0.0, 1.0, 0.0, 0.5)),
        GradientStop::new(0.4, Color::srgb(0.0, 0.0, 1.0, 1.0)),
        GradientStop::new(1.0, Color::WHITE),
    ];
    let densified = densify(&stops, Interpolation::new(ColorSpace::Oklch));

    for pair in densified.windows(2) {
        assert!(pair[0].offset <= pair[1].offset, "offsets went backwards");
    }
    for stop in &densified {
        assert_eq!(stop.color.space(), ColorSpace::Srgb);
    }
}

#[test]
fn a_hard_stop_stays_hard() {
    let stops = [
        GradientStop::new(0.0, Color::srgb(1.0, 0.0, 0.0, 1.0)),
        GradientStop::new(0.5, Color::srgb(1.0, 0.0, 0.0, 1.0)),
        GradientStop::new(0.5, Color::srgb(0.0, 0.0, 1.0, 1.0)),
        GradientStop::new(1.0, Color::srgb(0.0, 0.0, 1.0, 1.0)),
    ];
    let densified = densify(&stops, Interpolation::new(ColorSpace::Oklab));
    let at_half: Vec<_> = densified
        .iter()
        .filter(|stop| stop.offset == 0.5)
        .map(|stop| stop.color)
        .collect();
    assert_eq!(
        at_half,
        vec![
            Color::srgb(1.0, 0.0, 0.0, 1.0),
            Color::srgb(0.0, 0.0, 1.0, 1.0),
        ],
    );
}

#[test]
fn degenerate_ramps_are_carried_through() {
    let interpolation = Interpolation::new(ColorSpace::Oklab);
    assert!(densify(&[], interpolation).is_empty());

    let single = [GradientStop::new(0.3, Color::WHITE)];
    let densified = densify(&single, interpolation);
    assert_eq!(densified.len(), 1);
    assert_eq!(densified[0].offset, 0.3);
}

#[test]
fn a_tighter_tolerance_produces_more_stops() {
    let from = Color::srgb(0.0, 0.0, 1.0, 1.0);
    let to = Color::srgb(1.0, 1.0, 0.0, 1.0);
    let interpolation = Interpolation::new(ColorSpace::Oklab);
    let loose = densify_with_tolerance(&ramp(from, to), interpolation, 8.0 / 255.0);
    let tight = densify_with_tolerance(&ramp(from, to), interpolation, DEFAULT_TOLERANCE / 8.0);
    assert!(
        loose.len() < tight.len(),
        "{} stops loose, {} tight",
        loose.len(),
        tight.len(),
    );
}

/// A colour inside `space`'s own gamut, from four fractions.
///
/// For the wide-gamut RGB spaces that is a colour sRGB cannot represent, which is the case
/// densification finds hardest: bringing it back puts a corner in the curve.
fn in_gamut(space: ColorSpace, values: [f32; 4]) -> Color {
    match space {
        ColorSpace::DisplayP3
        | ColorSpace::A98Rgb
        | ColorSpace::ProPhotoRgb
        | ColorSpace::Rec2020 => Color::new(space, [values[0], values[1], values[2]], values[3]),
        _ => Color::srgb(values[0], values[1], values[2], values[3]).to_space(space),
    }
}

proptest! {
    /// Whatever the two colours and whatever the space, the approximation holds.
    #[test]
    fn any_two_stop_ramp_stays_within_one_step(
        from in prop::array::uniform4(0.0f32..=1.0),
        to in prop::array::uniform4(0.0f32..=1.0),
        authored in 0usize..14,
        space in 0usize..14,
        arc in 0usize..4,
    ) {
        let authored = ColorSpace::ALL[authored];
        let from = in_gamut(authored, from);
        let to = in_gamut(authored, to);
        let arc = [
            HueInterpolation::Shorter,
            HueInterpolation::Longer,
            HueInterpolation::Increasing,
            HueInterpolation::Decreasing,
        ][arc];
        let interpolation = Interpolation::new(ColorSpace::ALL[space]).with_hue(arc);

        let stops = densify(&ramp(from, to), interpolation);
        let worst = worst_error(from, to, interpolation, &stops, 65);
        prop_assert!(
            worst <= ONE_EIGHTH_BIT_STEP,
            "{:?} left an error of {worst} with {} stops",
            interpolation,
            stops.len(),
        );
    }
}
