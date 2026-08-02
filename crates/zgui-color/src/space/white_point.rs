//! The two illuminants CSS colour spaces are referenced to.
//!
//! A colour space says which light "white" is. CIE L\*a\*b\* and ProPhoto RGB are defined against
//! D50, everything else against D65, and a conversion that crosses between them has to adapt the
//! values rather than copy them — otherwise a white in one space arrives slightly blue or slightly
//! yellow in the other.

/// A standard illuminant.
///
/// The tristimulus values are the CIE 2-degree standard observer values CSS Color 4 uses, scaled
/// so that `Y` is exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WhitePoint {
    /// Horizon light, roughly 5000 K. The reference white of [`ColorSpace::Lab`] and
    /// [`ColorSpace::ProPhotoRgb`].
    ///
    /// [`ColorSpace::Lab`]: crate::ColorSpace::Lab
    /// [`ColorSpace::ProPhotoRgb`]: crate::ColorSpace::ProPhotoRgb
    D50,
    /// Noon daylight, roughly 6500 K. The reference white of sRGB and of every other space here.
    D65,
}

impl WhitePoint {
    /// The CIE XYZ tristimulus values of this illuminant.
    ///
    /// ```
    /// use zgui_color::WhitePoint;
    ///
    /// assert_eq!(WhitePoint::D65.tristimulus()[1], 1.0);
    /// ```
    pub const fn tristimulus(self) -> [f32; 3] {
        match self {
            Self::D50 => D50_TRISTIMULUS,
            Self::D65 => D65_TRISTIMULUS,
        }
    }
}

/// The D50 tristimulus values, `x = 0.3457`, `y = 0.3585`, normalised to `Y = 1`.
const D50_TRISTIMULUS: [f32; 3] = [0.964_295_7, 1.0, 0.825_104_6];

/// The D65 tristimulus values, `x = 0.3127`, `y = 0.3290`, normalised to `Y = 1`.
const D65_TRISTIMULUS: [f32; 3] = [0.950_455_9, 1.0, 1.089_057_8];

#[cfg(test)]
mod tests {
    use super::WhitePoint;

    /// `x` and `y` are the chromaticity coordinates the tristimulus values are derived from.
    fn chromaticity(tristimulus: [f32; 3]) -> (f32, f32) {
        let sum = tristimulus[0] + tristimulus[1] + tristimulus[2];
        (tristimulus[0] / sum, tristimulus[1] / sum)
    }

    #[test]
    fn tristimulus_values_match_their_chromaticities() {
        let (x, y) = chromaticity(WhitePoint::D50.tristimulus());
        assert!((x - 0.3457).abs() < 1e-4, "D50 x is {x}");
        assert!((y - 0.3585).abs() < 1e-4, "D50 y is {y}");

        let (x, y) = chromaticity(WhitePoint::D65.tristimulus());
        assert!((x - 0.3127).abs() < 1e-4, "D65 x is {x}");
        assert!((y - 0.3290).abs() < 1e-4, "D65 y is {y}");
    }
}
