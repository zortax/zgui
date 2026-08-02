//! The colour value itself.

pub mod premultiply;

pub use crate::color::premultiply::PremultipliedLinear;

use crate::convert::convert;
use crate::space::ColorSpace;

/// A colour: three channel values, an alpha, and the space the three values are expressed in.
///
/// What the channels mean depends entirely on the space; see the
/// [space documentation](crate::space) for the table. Values are not clamped to their nominal
/// ranges, because an intermediate colour part-way through a gradient routinely leaves them and
/// clamping early is what makes a wide-gamut ramp band.
///
/// The one place a colour becomes a fixed set of numbers a renderer can use is
/// [`Color::to_premultiplied_srgb`].
///
/// ```
/// use zgui_color::{Color, ColorSpace};
///
/// let red = Color::srgb_u8(255, 0, 0, 255);
/// let wide = red.to_space(ColorSpace::DisplayP3);
///
/// assert_eq!(wide.space(), ColorSpace::DisplayP3);
/// // sRGB red is inside the Display P3 gamut, so it needs less of that space's red primary.
/// assert!(wide.components()[0] < 0.95);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// The three channel values, meaningful only together with `space`.
    components: [f32; 3],
    /// The alpha channel, nominally `0..=1`.
    alpha: f32,
    /// The space `components` are expressed in.
    space: ColorSpace,
}

impl Color {
    /// Fully transparent black, in sRGB — the `transparent` keyword.
    pub const TRANSPARENT: Self = Self::srgb(0.0, 0.0, 0.0, 0.0);

    /// Opaque black, in sRGB.
    pub const BLACK: Self = Self::srgb(0.0, 0.0, 0.0, 1.0);

    /// Opaque white, in sRGB.
    pub const WHITE: Self = Self::srgb(1.0, 1.0, 1.0, 1.0);

    /// A colour from its three channel values, an alpha and a space.
    ///
    /// ```
    /// use zgui_color::{Color, ColorSpace};
    ///
    /// let plum = Color::new(ColorSpace::Oklch, [0.7, 0.1, 320.0], 1.0);
    /// assert_eq!(plum.components()[2], 320.0);
    /// ```
    pub const fn new(space: ColorSpace, components: [f32; 3], alpha: f32) -> Self {
        Self {
            components,
            alpha,
            space,
        }
    }

    /// A gamma-encoded sRGB colour from four fractions.
    ///
    /// ```
    /// use zgui_color::Color;
    ///
    /// let half_red = Color::srgb(1.0, 0.0, 0.0, 0.5);
    /// assert_eq!(half_red.alpha(), 0.5);
    /// ```
    pub const fn srgb(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self::new(ColorSpace::Srgb, [red, green, blue], alpha)
    }

    /// A gamma-encoded sRGB colour from four bytes, the form `#rrggbbaa` notation parses to.
    ///
    /// ```
    /// use zgui_color::Color;
    ///
    /// assert_eq!(Color::srgb_u8(255, 255, 255, 255), Color::WHITE);
    /// ```
    pub fn srgb_u8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        let scale = |value: u8| f32::from(value) / 255.0;
        Self::srgb(scale(red), scale(green), scale(blue), scale(alpha))
    }

    /// The three channel values, in the colour's own space.
    ///
    /// These are not renderer input: what they mean depends on [`Color::space`], and a renderer
    /// wants [`Color::to_premultiplied_srgb`].
    pub const fn components(self) -> [f32; 3] {
        self.components
    }

    /// The alpha channel, where zero is fully transparent and one fully opaque.
    pub const fn alpha(self) -> f32 {
        self.alpha
    }

    /// The space [`Color::components`] are expressed in.
    pub const fn space(self) -> ColorSpace {
        self.space
    }

    /// The same colour with a different alpha.
    ///
    /// ```
    /// use zgui_color::Color;
    ///
    /// assert_eq!(Color::BLACK.with_alpha(0.0).alpha(), 0.0);
    /// ```
    pub const fn with_alpha(self, alpha: f32) -> Self {
        Self { alpha, ..self }
    }

    /// Whether the colour is fully opaque.
    ///
    /// An opaque colour composites as a plain replacement, which is worth knowing before choosing
    /// a blend path.
    pub fn is_opaque(self) -> bool {
        self.alpha >= 1.0
    }

    /// Whether the colour is fully transparent, and so contributes nothing to what is drawn.
    pub fn is_transparent(self) -> bool {
        self.alpha <= 0.0
    }

    /// The same colour expressed in another space.
    ///
    /// Conversion is lossless in the sense that matters — nothing is clamped, and a colour outside
    /// the target space's gamut simply has channel values outside its nominal range, so converting
    /// back returns where it started. A hue channel produced by this method is normalised into
    /// `0..360`, and the hue of a grey, which names nothing, is reported as zero.
    ///
    /// ```
    /// use zgui_color::{Color, ColorSpace};
    ///
    /// let red = Color::srgb(1.0, 0.0, 0.0, 1.0);
    /// let oklch = red.to_space(ColorSpace::Oklch);
    /// let back = oklch.to_space(ColorSpace::Srgb);
    ///
    /// assert!((back.components()[0] - 1.0).abs() < 1e-4);
    /// assert!(back.components()[1].abs() < 1e-4);
    /// ```
    pub fn to_space(self, space: ColorSpace) -> Self {
        Self {
            components: convert(self.components, self.space, space),
            alpha: self.alpha,
            space,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Color;
    use crate::space::ColorSpace;

    #[test]
    fn bytes_scale_to_fractions() {
        let grey = Color::srgb_u8(128, 128, 128, 51);
        assert!((grey.components()[0] - 128.0 / 255.0).abs() < 1e-6);
        assert!((grey.alpha() - 0.2).abs() < 1e-3);
    }

    #[test]
    fn converting_to_the_same_space_changes_nothing() {
        let color = Color::new(ColorSpace::Lch, [50.0, 30.0, 200.0], 0.5);
        assert_eq!(color.to_space(ColorSpace::Lch), color);
    }

    #[test]
    fn conversion_leaves_alpha_alone() {
        let color = Color::srgb(0.2, 0.4, 0.6, 0.25);
        for space in ColorSpace::ALL {
            assert_eq!(color.to_space(space).alpha(), 0.25, "{space:?}");
        }
    }

    #[test]
    fn opacity_is_reported_at_the_extremes() {
        assert!(Color::WHITE.is_opaque());
        assert!(!Color::WHITE.is_transparent());
        assert!(Color::TRANSPARENT.is_transparent());
        assert!(!Color::TRANSPARENT.is_opaque());
    }
}
