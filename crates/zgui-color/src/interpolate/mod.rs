//! Interpolating between two colours.
//!
//! Every colour ramp in CSS — a gradient, a transition, `color-mix()` — is the same operation:
//! move both endpoints into a named space, premultiply, walk the channels linearly, and undo the
//! premultiplication. The space is the interesting part, because it decides what "half-way"
//! means. Half-way between blue and white is a washed-out lavender in sRGB and a clean pale blue
//! in Oklab, and the difference is not subtle.

pub(crate) mod channels;
pub mod hue;

pub use crate::interpolate::hue::HueInterpolation;

use crate::color::Color;
use crate::convert::polar::normalize_hue;
use crate::space::ColorSpace;

/// Where and how an interpolation happens: the space to interpolate in, and the way round the hue
/// circle to travel if that space has a hue.
///
/// ```
/// use zgui_color::{ColorSpace, HueInterpolation, Interpolation};
///
/// // What `in oklab` means in a gradient.
/// let oklab = Interpolation::new(ColorSpace::Oklab);
/// assert_eq!(oklab.hue, HueInterpolation::Shorter);
///
/// // What `in hsl longer hue` means.
/// let long_way = Interpolation::new(ColorSpace::Hsl).with_hue(HueInterpolation::Longer);
/// assert_eq!(long_way.space, ColorSpace::Hsl);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Interpolation {
    /// The space the channels are walked in.
    pub space: ColorSpace,
    /// Which arc of the hue circle to travel, ignored in spaces with no hue.
    pub hue: HueInterpolation,
}

impl Interpolation {
    /// Interpolation in `space`, taking the shorter arc round the hue circle.
    pub const fn new(space: ColorSpace) -> Self {
        Self {
            space,
            hue: HueInterpolation::Shorter,
        }
    }

    /// The same interpolation with a different way round the hue circle.
    pub const fn with_hue(self, hue: HueInterpolation) -> Self {
        Self { hue, ..self }
    }
}

impl Default for Interpolation {
    /// Oklab with the shorter hue arc, which is what CSS interpolates in when nothing says
    /// otherwise.
    fn default() -> Self {
        Self::new(ColorSpace::Oklab)
    }
}

/// The colour `t` of the way from `from` to `to`.
///
/// `t` of zero is `from` and one is `to`; values outside that range extrapolate rather than clamp,
/// which is what lets a gradient whose stops do not reach its ends fill them in. The result is in
/// the interpolation space, so a ramp is sampled by calling this repeatedly and converting once,
/// at the end.
///
/// ```
/// use zgui_color::{Color, ColorSpace, Interpolation, interpolate};
///
/// let red = Color::srgb(1.0, 0.0, 0.0, 1.0);
/// let blue = Color::srgb(0.0, 0.0, 1.0, 1.0);
/// let middle = interpolate(red, blue, 0.5, Interpolation::new(ColorSpace::Srgb));
///
/// assert_eq!(middle.components(), [0.5, 0.0, 0.5]);
/// ```
///
/// Transparency is handled by premultiplying, so a ramp to `transparent` keeps its colour instead
/// of darkening as it fades:
///
/// ```
/// use zgui_color::{Color, ColorSpace, Interpolation, interpolate};
///
/// let blue = Color::srgb(0.0, 0.0, 1.0, 1.0);
/// let middle = interpolate(Color::TRANSPARENT, blue, 0.5, Interpolation::new(ColorSpace::Srgb));
///
/// assert_eq!(middle.components(), [0.0, 0.0, 1.0]);
/// assert_eq!(middle.alpha(), 0.5);
/// ```
pub fn interpolate(from: Color, to: Color, t: f32, interpolation: Interpolation) -> Color {
    if interpolation.space == ColorSpace::SrgbLinear {
        // Linear light is exactly what `PremultipliedLinear` is: interpolating there through the
        // general path below would be the same arithmetic written twice.
        return from
            .to_premultiplied_linear()
            .lerp(to.to_premultiplied_linear(), t)
            .to_color();
    }
    general(from, to, t, interpolation)
}

/// Interpolation in any space, by the CSS rules: hue fixup, premultiply, mix, un-premultiply.
fn general(from: Color, to: Color, t: f32, interpolation: Interpolation) -> Color {
    let space = interpolation.space;
    let from = from.to_space(space);
    let to = to.to_space(space);

    let mut start = from.components();
    let mut end = to.components();
    if let Some(index) = space.hue_index() {
        // Both hues are brought into a single turn first. A hue is an angle and `540deg` names the
        // same direction as `180deg`, but the fixup below reads the difference between the two
        // numbers, so an endpoint written several turns round would send the interpolation the
        // wrong way and past hues neither endpoint has.
        let (fixed_from, fixed_to) = interpolation
            .hue
            .fixup(normalize_hue(start[index]), normalize_hue(end[index]));
        start[index] = fixed_from;
        end[index] = fixed_to;
    }

    let start = channels::premultiply(space, start, from.alpha());
    let end = channels::premultiply(space, end, to.alpha());

    let mix = |a: f32, b: f32| a + (b - a) * t;
    let alpha = mix(from.alpha(), to.alpha());
    let mut components = [0.0; 3];
    for (index, value) in components.iter_mut().enumerate() {
        *value = mix(start[index], end[index]);
    }

    let mut components = channels::unpremultiply(space, components, alpha);
    if let Some(index) = space.hue_index() {
        components[index] = normalize_hue(components[index]);
    }
    Color::new(space, components, alpha)
}

#[cfg(test)]
mod tests {
    use super::{Interpolation, general, interpolate};
    use crate::color::Color;
    use crate::interpolate::hue::HueInterpolation;
    use crate::space::ColorSpace;

    #[test]
    fn the_endpoints_are_reproduced_exactly() {
        let from = Color::srgb(0.2, 0.4, 0.6, 0.8);
        let to = Color::new(ColorSpace::Oklch, [0.7, 0.15, 200.0], 0.4);
        for space in ColorSpace::ALL {
            let interpolation = Interpolation::new(space);
            let start = interpolate(from, to, 0.0, interpolation);
            let end = interpolate(from, to, 1.0, interpolation);
            for channel in 0..3 {
                let expected = from.to_space(space).components()[channel];
                assert!(
                    (start.components()[channel] - expected).abs() < 1e-3,
                    "{space:?} channel {channel}: {} vs {expected}",
                    start.components()[channel],
                );
                let expected = to.to_space(space).components()[channel];
                assert!(
                    (end.components()[channel] - expected).abs() < 1e-3,
                    "{space:?} channel {channel}: {} vs {expected}",
                    end.components()[channel],
                );
            }
        }
    }

    #[test]
    fn the_linear_shortcut_agrees_with_the_general_path() {
        let interpolation = Interpolation::new(ColorSpace::SrgbLinear);
        let from = Color::srgb(0.9, 0.1, 0.3, 0.25);
        let to = Color::srgb(0.1, 0.8, 0.2, 1.0);
        for step in 0i16..=10 {
            let t = f32::from(step) / 10.0;
            let shortcut = interpolate(from, to, t, interpolation);
            let generic = general(from, to, t, interpolation);
            for channel in 0..3 {
                assert!(
                    (shortcut.components()[channel] - generic.components()[channel]).abs() < 1e-6,
                    "channel {channel} at t = {t}",
                );
            }
            assert!((shortcut.alpha() - generic.alpha()).abs() < 1e-6);
        }
    }

    #[test]
    fn interpolating_a_colour_with_itself_stands_still() {
        let color = Color::new(ColorSpace::Lch, [60.0, 40.0, 120.0], 0.5);
        for space in ColorSpace::ALL {
            let middle = interpolate(color, color, 0.5, Interpolation::new(space));
            let expected = color.to_space(space);
            for channel in 0..3 {
                assert!(
                    (middle.components()[channel] - expected.components()[channel]).abs() < 1e-3,
                    "{space:?} channel {channel}",
                );
            }
        }
    }

    #[test]
    fn the_hue_arc_is_the_one_that_was_asked_for() {
        let from = Color::new(ColorSpace::Hsl, [20.0, 0.5, 0.5], 1.0);
        let to = Color::new(ColorSpace::Hsl, [320.0, 0.5, 0.5], 1.0);
        let hue = |method| {
            interpolate(
                from,
                to,
                0.5,
                Interpolation::new(ColorSpace::Hsl).with_hue(method),
            )
            .components()[0]
        };
        assert!((hue(HueInterpolation::Shorter) - 350.0).abs() < 1e-3);
        assert!((hue(HueInterpolation::Longer) - 170.0).abs() < 1e-3);
        assert!((hue(HueInterpolation::Increasing) - 170.0).abs() < 1e-3);
        assert!((hue(HueInterpolation::Decreasing) - 350.0).abs() < 1e-3);
    }

    #[test]
    fn a_hue_written_several_turns_round_means_what_it_says() {
        // `hsl(720deg …)` is the same colour as `hsl(0deg …)`, so it has to interpolate the same
        // way: the fixup reads the difference between two numbers and would otherwise send this
        // one two turns backwards.
        let far = Color::new(ColorSpace::Hsl, [720.0, 0.5, 0.5], 1.0);
        let near = Color::new(ColorSpace::Hsl, [0.0, 0.5, 0.5], 1.0);
        let to = Color::new(ColorSpace::Hsl, [10.0, 0.5, 0.5], 1.0);
        for method in [
            HueInterpolation::Shorter,
            HueInterpolation::Longer,
            HueInterpolation::Increasing,
            HueInterpolation::Decreasing,
        ] {
            let interpolation = Interpolation::new(ColorSpace::Hsl).with_hue(method);
            let from_far = interpolate(far, to, 0.5, interpolation).components()[0];
            let from_near = interpolate(near, to, 0.5, interpolation).components()[0];
            assert!(
                (from_far - from_near).abs() < 1e-3,
                "{method:?}: {from_far} from 720°, {from_near} from 0°",
            );
        }
    }

    #[test]
    fn extrapolation_runs_past_the_endpoints() {
        let from = Color::srgb(0.0, 0.0, 0.0, 1.0);
        let to = Color::srgb(0.5, 0.5, 0.5, 1.0);
        let beyond = interpolate(from, to, 2.0, Interpolation::new(ColorSpace::Srgb));
        assert_eq!(beyond.components(), [1.0, 1.0, 1.0]);
    }

    #[test]
    fn the_default_interpolation_is_oklab() {
        assert_eq!(Interpolation::default().space, ColorSpace::Oklab);
    }
}
