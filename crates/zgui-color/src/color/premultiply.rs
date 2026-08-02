//! The one conversion a renderer performs, and the linear-light form that is not it.
//!
//! # Why premultiplied
//!
//! Compositing multiplies a source colour by its coverage and adds it to what is already there.
//! If the colour is stored straight — full-intensity red beside an alpha of `0.5` — every soft
//! edge blooms, because a filtered sample of the edge averages the *colour* as well as the alpha
//! and produces a red that was never drawn. Premultiplying folds the alpha into the channels once,
//! at the point the colour is produced, so every later blend, filter and downsample is a plain
//! weighted sum.
//!
//! # Why sRGB rather than linear light
//!
//! Blending in linear light is the physically honest choice and it is not what CSS specifies.
//! Filter functions are defined to operate in the sRGB colour space, gradients and compositing are
//! specified on the encoded values, and every browser blends in gamma space; a framework that
//! blended in linear light would render correct-looking images that do not match any other
//! implementation. So [`Color::to_premultiplied_srgb`] is the single conversion on the renderer
//! path, and everything a renderer holds — surfaces, intermediate targets, atlas tiles — holds
//! premultiplied, gamma-encoded sRGB.
//!
//! [`Color::to_premultiplied_linear`] exists for the opposite reason: interpolation that a CSS
//! author asked for in a linear space genuinely has to happen there. Its result is a
//! [`PremultipliedLinear`], which cannot be turned into an array of floats — it converts back to a
//! [`Color`], and reaching a renderer still means going through
//! [`Color::to_premultiplied_srgb`].

use crate::color::Color;
use crate::space::ColorSpace;

impl Color {
    /// The colour as premultiplied, gamma-encoded sRGB, ready for a renderer.
    ///
    /// This is the only function in this crate that produces renderer-facing values, and every
    /// path that draws anything goes through it, so the choice of encoding is made in exactly one
    /// place. The result is `[red, green, blue, alpha]` with the three channels already multiplied
    /// by alpha.
    ///
    /// Channels are brought into `0..=1` before premultiplication. A colour outside the sRGB gamut
    /// — a Display P3 green, say — has no representation on an sRGB surface, and a channel that
    /// stayed outside the range would wrap or saturate unpredictably once it reached an 8-bit
    /// target. An infinite channel therefore arrives at the end of the range it ran off, and a
    /// channel that is not a number at all becomes zero, because it names no direction to clamp in.
    ///
    /// ```
    /// use zgui_color::Color;
    ///
    /// let half_red = Color::srgb(1.0, 0.0, 0.0, 0.5);
    /// assert_eq!(half_red.to_premultiplied_srgb(), [0.5, 0.0, 0.0, 0.5]);
    ///
    /// // Transparent colours contribute nothing, whatever their channels say.
    /// assert_eq!(Color::WHITE.with_alpha(0.0).to_premultiplied_srgb(), [0.0; 4]);
    /// ```
    pub fn to_premultiplied_srgb(self) -> [f32; 4] {
        let srgb = self.to_space(ColorSpace::Srgb);
        let alpha = unit(srgb.alpha);
        let [red, green, blue] = srgb.components.map(unit);
        [red * alpha, green * alpha, blue * alpha, alpha]
    }

    /// The colour as premultiplied linear-light sRGB, for interpolation that must happen in linear
    /// light.
    ///
    /// This is **not** a renderer path: see [`PremultipliedLinear`] for what it is for, and
    /// [`Color::to_premultiplied_srgb`] for what a renderer wants.
    ///
    /// ```
    /// use zgui_color::{Color, ColorSpace};
    ///
    /// let grey = Color::srgb(0.5, 0.5, 0.5, 1.0);
    /// let linear = grey.to_premultiplied_linear().to_color();
    ///
    /// assert_eq!(linear.space(), ColorSpace::SrgbLinear);
    /// // Half-way up the encoded ramp is about a fifth of the light.
    /// assert!((linear.components()[0] - 0.2140).abs() < 1e-3);
    /// ```
    pub fn to_premultiplied_linear(self) -> PremultipliedLinear {
        let linear = self.to_space(ColorSpace::SrgbLinear);
        let alpha = linear.alpha;
        let [red, green, blue] = linear.components;
        PremultipliedLinear {
            rgba: [red * alpha, green * alpha, blue * alpha, alpha],
        }
    }
}

/// Brings a channel into `0..=1`, mapping a value that is not a number to zero.
fn unit(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// A colour in premultiplied linear-light sRGB.
///
/// Interpolating in linear light is what a CSS author asks for with `in srgb-linear`, and it is
/// the form the light-proportional half of colour arithmetic works in. It is deliberately opaque:
/// there is no way to read its four numbers out, because those numbers are not what any surface,
/// texture or vertex buffer holds. Convert it back with [`PremultipliedLinear::to_color`], and let
/// [`Color::to_premultiplied_srgb`] produce renderer input.
///
/// ```
/// use zgui_color::Color;
///
/// let black = Color::BLACK.to_premultiplied_linear();
/// let white = Color::WHITE.to_premultiplied_linear();
/// let middle = black.lerp(white, 0.5).to_color();
///
/// // Half the *light*, which is a good deal brighter than half the encoded value.
/// assert!((middle.to_space(zgui_color::ColorSpace::Srgb).components()[0] - 0.7354).abs() < 1e-3);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PremultipliedLinear {
    /// Premultiplied linear-light red, green and blue, then alpha.
    rgba: [f32; 4],
}

impl PremultipliedLinear {
    /// Interpolates towards `other`, where `t` of zero is `self` and one is `other`.
    ///
    /// Interpolating premultiplied values is what makes a ramp to transparency keep its colour: a
    /// straight-alpha midpoint between opaque red and transparent black is a muddy dark red, while
    /// the premultiplied midpoint is red at half alpha, which is what a gradient to `transparent`
    /// is meant to look like.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let mut rgba = [0.0; 4];
        for (channel, value) in rgba.iter_mut().enumerate() {
            *value = self.rgba[channel] + (other.rgba[channel] - self.rgba[channel]) * t;
        }
        Self { rgba }
    }

    /// The colour these premultiplied values describe, in
    /// [`ColorSpace::SrgbLinear`] with straight alpha.
    ///
    /// A fully transparent value keeps its channels rather than dividing by zero, which is what
    /// lets an interpolation pass through transparency and come out the other side with its hue
    /// intact.
    pub fn to_color(self) -> Color {
        let [red, green, blue, alpha] = self.rgba;
        let components = if alpha == 0.0 {
            [red, green, blue]
        } else {
            [red / alpha, green / alpha, blue / alpha]
        };
        Color::new(ColorSpace::SrgbLinear, components, alpha)
    }
}

#[cfg(test)]
mod tests {
    use crate::color::Color;
    use crate::space::ColorSpace;

    #[test]
    fn premultiplication_scales_the_channels_by_alpha() {
        let color = Color::srgb(0.8, 0.4, 0.2, 0.25);
        let [red, green, blue, alpha] = color.to_premultiplied_srgb();
        assert!((red - 0.2).abs() < 1e-6);
        assert!((green - 0.1).abs() < 1e-6);
        assert!((blue - 0.05).abs() < 1e-6);
        assert_eq!(alpha, 0.25);
    }

    #[test]
    fn out_of_gamut_channels_are_brought_into_range() {
        let wide = Color::new(ColorSpace::DisplayP3, [0.0, 1.0, 0.0], 1.0);
        for channel in wide.to_premultiplied_srgb() {
            assert!((0.0..=1.0).contains(&channel), "channel is {channel}");
        }
    }

    #[test]
    fn a_channel_that_is_not_a_number_becomes_zero() {
        let broken = Color::srgb(f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0);
        assert_eq!(broken.to_premultiplied_srgb(), [0.0, 1.0, 0.0, 1.0]);
    }

    #[test]
    fn every_space_reaches_the_same_premultiplied_value() {
        // The conversion is defined by the colour, not by the spelling of it: the same colour
        // written in any of the fourteen spaces has to reach the renderer as the same four floats.
        let color = Color::srgb(0.6, 0.3, 0.1, 0.75);
        let expected = color.to_premultiplied_srgb();
        for space in ColorSpace::ALL {
            let round_tripped = color.to_space(space).to_premultiplied_srgb();
            for channel in 0..4 {
                assert!(
                    (round_tripped[channel] - expected[channel]).abs() < 1e-4,
                    "{space:?} channel {channel} is {}, expected {}",
                    round_tripped[channel],
                    expected[channel],
                );
            }
        }
    }

    #[test]
    fn linear_premultiplication_round_trips_through_its_colour() {
        let color = Color::srgb(0.6, 0.3, 0.1, 0.75);
        let back = color.to_premultiplied_linear().to_color();
        let expected = color.to_space(ColorSpace::SrgbLinear);
        for channel in 0..3 {
            assert!((back.components()[channel] - expected.components()[channel]).abs() < 1e-6);
        }
        assert!((back.alpha() - 0.75).abs() < 1e-6);
    }

    #[test]
    fn a_transparent_linear_value_keeps_its_channels() {
        let from = Color::srgb(1.0, 0.0, 0.0, 0.0).to_premultiplied_linear();
        let to = Color::srgb(0.0, 0.0, 1.0, 0.0).to_premultiplied_linear();
        let middle = from.lerp(to, 0.5).to_color();
        assert_eq!(middle.alpha(), 0.0);
        assert_eq!(middle.components(), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_ramp_to_transparent_keeps_its_hue() {
        let red = Color::srgb(1.0, 0.0, 0.0, 1.0).to_premultiplied_linear();
        let clear = Color::TRANSPARENT.to_premultiplied_linear();
        let middle = red.lerp(clear, 0.5).to_color();
        assert!((middle.alpha() - 0.5).abs() < 1e-6);
        assert!((middle.components()[0] - 1.0).abs() < 1e-6);
    }
}
