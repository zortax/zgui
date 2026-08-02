//! The colour spaces a colour can be expressed in.
//!
//! A [`Color`](crate::Color) is three numbers, an alpha and a space. The space says what the three
//! numbers *mean*, and there is no default: `[0.5, 0.5, 0.5]` is a mid grey in
//! [`ColorSpace::Srgb`], a dark desaturated red in [`ColorSpace::Hsl`] and a colour well outside
//! any display's gamut in [`ColorSpace::Lab`].
//!
//! # Channel ranges
//!
//! Ranges below are the *nominal* ones. Nothing clamps to them: a colour outside a space's
//! nominal range is a perfectly ordinary intermediate value, and forcing it into range early is
//! how gradients lose their smoothness. The one place values are brought into range is
//! [`Color::to_premultiplied_srgb`](crate::Color::to_premultiplied_srgb), at the very end.
//!
//! | Space | Channel 0 | Channel 1 | Channel 2 | White point |
//! |---|---|---|---|---|
//! | [`Srgb`](ColorSpace::Srgb) | red `0..=1` | green `0..=1` | blue `0..=1` | D65 |
//! | [`SrgbLinear`](ColorSpace::SrgbLinear) | red `0..=1` | green `0..=1` | blue `0..=1` | D65 |
//! | [`Hsl`](ColorSpace::Hsl) | hue, degrees | saturation `0..=1` | lightness `0..=1` | D65 |
//! | [`Hwb`](ColorSpace::Hwb) | hue, degrees | whiteness `0..=1` | blackness `0..=1` | D65 |
//! | [`Lab`](ColorSpace::Lab) | lightness `0..=100` | a `≈ -125..=125` | b `≈ -125..=125` | D50 |
//! | [`Lch`](ColorSpace::Lch) | lightness `0..=100` | chroma `0..=150` | hue, degrees | D50 |
//! | [`Oklab`](ColorSpace::Oklab) | lightness `0..=1` | a `≈ -0.4..=0.4` | b `≈ -0.4..=0.4` | D65 |
//! | [`Oklch`](ColorSpace::Oklch) | lightness `0..=1` | chroma `0..=0.4` | hue, degrees | D65 |
//! | [`DisplayP3`](ColorSpace::DisplayP3) | red `0..=1` | green `0..=1` | blue `0..=1` | D65 |
//! | [`A98Rgb`](ColorSpace::A98Rgb) | red `0..=1` | green `0..=1` | blue `0..=1` | D65 |
//! | [`ProPhotoRgb`](ColorSpace::ProPhotoRgb) | red `0..=1` | green `0..=1` | blue `0..=1` | D50 |
//! | [`Rec2020`](ColorSpace::Rec2020) | red `0..=1` | green `0..=1` | blue `0..=1` | D65 |
//! | [`XyzD50`](ColorSpace::XyzD50) | X | Y `0..=1` | Z | D50 |
//! | [`XyzD65`](ColorSpace::XyzD65) | X | Y `0..=1` | Z | D65 |
//!
//! Hue is always in degrees and is not confined to a turn: interpolation deliberately produces
//! hues outside `0..360` (see [`HueInterpolation`](crate::HueInterpolation)), and a hue read back
//! out of [`Color::to_space`](crate::Color::to_space) is normalised into `0..360` only because a
//! converted colour has no history to preserve.

pub mod white_point;

pub use crate::space::white_point::WhitePoint;

/// A colour space.
///
/// The names match the CSS Color 4 keywords: the variant a `color(display-p3 …)` value carries is
/// [`ColorSpace::DisplayP3`], and the space named by `in oklab` in a gradient or `color-mix()` is
/// [`ColorSpace::Oklab`].
///
/// See the [module documentation](crate::space) for what each space's three channels mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorSpace {
    /// Gamma-encoded sRGB — the space a display expects and the space every renderer path ends in.
    Srgb,
    /// sRGB primaries with a linear transfer function, so its values are proportional to light.
    SrgbLinear,
    /// Hue, saturation and lightness over the sRGB gamut.
    Hsl,
    /// Hue, whiteness and blackness over the sRGB gamut.
    Hwb,
    /// CIE L\*a\*b\*, a perceptual space with a D50 white point.
    Lab,
    /// The cylindrical form of [`ColorSpace::Lab`].
    Lch,
    /// Oklab, a perceptual space with better hue uniformity than [`ColorSpace::Lab`].
    Oklab,
    /// The cylindrical form of [`ColorSpace::Oklab`].
    Oklch,
    /// The Display P3 gamut with the sRGB transfer function.
    DisplayP3,
    /// Adobe RGB (1998).
    A98Rgb,
    /// ProPhoto RGB, a very wide gamut with a D50 white point.
    ProPhotoRgb,
    /// ITU-R BT.2020, the ultra-high-definition television gamut.
    Rec2020,
    /// CIE XYZ with a D50 white point.
    XyzD50,
    /// CIE XYZ with a D65 white point.
    XyzD65,
}

impl ColorSpace {
    /// Every colour space, in the order they are declared.
    ///
    /// Useful for exhaustive tests and for iterating the interpolation spaces a gradient may name.
    pub const ALL: [Self; 14] = [
        Self::Srgb,
        Self::SrgbLinear,
        Self::Hsl,
        Self::Hwb,
        Self::Lab,
        Self::Lch,
        Self::Oklab,
        Self::Oklch,
        Self::DisplayP3,
        Self::A98Rgb,
        Self::ProPhotoRgb,
        Self::Rec2020,
        Self::XyzD50,
        Self::XyzD65,
    ];

    /// The CSS keyword for this space.
    ///
    /// ```
    /// use zgui_color::ColorSpace;
    ///
    /// assert_eq!(ColorSpace::ProPhotoRgb.keyword(), "prophoto-rgb");
    /// ```
    pub const fn keyword(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::SrgbLinear => "srgb-linear",
            Self::Hsl => "hsl",
            Self::Hwb => "hwb",
            Self::Lab => "lab",
            Self::Lch => "lch",
            Self::Oklab => "oklab",
            Self::Oklch => "oklch",
            Self::DisplayP3 => "display-p3",
            Self::A98Rgb => "a98-rgb",
            Self::ProPhotoRgb => "prophoto-rgb",
            Self::Rec2020 => "rec2020",
            Self::XyzD50 => "xyz-d50",
            Self::XyzD65 => "xyz-d65",
        }
    }

    /// The names of the three channels, in order.
    ///
    /// ```
    /// use zgui_color::ColorSpace;
    ///
    /// assert_eq!(ColorSpace::Oklch.channels(), ["lightness", "chroma", "hue"]);
    /// ```
    pub const fn channels(self) -> [&'static str; 3] {
        match self {
            Self::Srgb | Self::SrgbLinear | Self::DisplayP3 => ["red", "green", "blue"],
            Self::A98Rgb | Self::ProPhotoRgb | Self::Rec2020 => ["red", "green", "blue"],
            Self::Hsl => ["hue", "saturation", "lightness"],
            Self::Hwb => ["hue", "whiteness", "blackness"],
            Self::Lab | Self::Oklab => ["lightness", "a", "b"],
            Self::Lch | Self::Oklch => ["lightness", "chroma", "hue"],
            Self::XyzD50 | Self::XyzD65 => ["x", "y", "z"],
        }
    }

    /// The index of the hue channel, for the spaces that have one.
    ///
    /// Hue is an angle, so it is the one channel that is neither premultiplied by alpha nor
    /// interpolated by simple subtraction; everything that walks a colour's channels has to ask.
    ///
    /// ```
    /// use zgui_color::ColorSpace;
    ///
    /// assert_eq!(ColorSpace::Hsl.hue_index(), Some(0));
    /// assert_eq!(ColorSpace::Lch.hue_index(), Some(2));
    /// assert_eq!(ColorSpace::Lab.hue_index(), None);
    /// ```
    pub const fn hue_index(self) -> Option<usize> {
        match self {
            Self::Hsl | Self::Hwb => Some(0),
            Self::Lch | Self::Oklch => Some(2),
            _ => None,
        }
    }

    /// Whether this space is cylindrical, meaning one of its channels is an angle.
    ///
    /// ```
    /// use zgui_color::ColorSpace;
    ///
    /// assert!(ColorSpace::Oklch.is_polar());
    /// assert!(!ColorSpace::Oklab.is_polar());
    /// ```
    pub const fn is_polar(self) -> bool {
        self.hue_index().is_some()
    }

    /// The illuminant this space's values are referenced to.
    ///
    /// ```
    /// use zgui_color::{ColorSpace, WhitePoint};
    ///
    /// assert_eq!(ColorSpace::Lab.white_point(), WhitePoint::D50);
    /// assert_eq!(ColorSpace::Oklab.white_point(), WhitePoint::D65);
    /// ```
    pub const fn white_point(self) -> WhitePoint {
        match self {
            Self::Lab | Self::Lch | Self::ProPhotoRgb | Self::XyzD50 => WhitePoint::D50,
            _ => WhitePoint::D65,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ColorSpace;

    #[test]
    fn every_space_is_listed_once() {
        for space in ColorSpace::ALL {
            let occurrences = ColorSpace::ALL.iter().filter(|it| **it == space).count();
            assert_eq!(occurrences, 1, "{space:?} appears more than once in ALL");
        }
    }

    #[test]
    fn keywords_are_unique() {
        for (index, space) in ColorSpace::ALL.iter().enumerate() {
            for other in &ColorSpace::ALL[index + 1..] {
                assert_ne!(space.keyword(), other.keyword());
            }
        }
    }

    #[test]
    fn hue_channels_are_named_hue() {
        for space in ColorSpace::ALL {
            match space.hue_index() {
                Some(index) => assert_eq!(space.channels()[index], "hue"),
                None => assert!(!space.channels().contains(&"hue")),
            }
        }
    }
}
