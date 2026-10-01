//! How a video frame's luma and chroma samples map to colour.
//!
//! A decoder hands out Y′CbCr samples. Their meaning depends on two facts the bitstream states:
//! the matrix that derived them from R′G′B′, and the range the samples occupy. [`ColorSpace`]
//! holds both, and [`ColorSpace::transform`] folds them into one affine map. The map produces
//! gamma-encoded R′G′B′, the encoding the whole compositor works in.

/// The matrix that derived luma and chroma from gamma-encoded R′G′B′.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorMatrix {
    /// ITU-R BT.601: standard-definition video and most JPEG-derived content.
    Bt601,
    /// ITU-R BT.709: high-definition video and screen content.
    #[default]
    Bt709,
    /// ITU-R BT.2020, non-constant luminance: ultra-high-definition video.
    Bt2020,
}

impl ColorMatrix {
    /// The red and blue luma weights, `Kr` and `Kb`.
    fn weights(self) -> (f32, f32) {
        match self {
            Self::Bt601 => (0.299, 0.114),
            Self::Bt709 => (0.2126, 0.0722),
            Self::Bt2020 => (0.2627, 0.0593),
        }
    }
}

/// Which part of the sample range holds the signal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorRange {
    /// Luma in 16–235 and chroma in 16–240, scaled to the bit depth. Most video uses it.
    #[default]
    Limited,
    /// Every code value carries signal. JPEG and most screen capture use it.
    Full,
}

/// The matrix and the range of one frame's samples.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ColorSpace {
    /// The matrix the samples were derived with.
    pub matrix: ColorMatrix,
    /// The range the samples occupy.
    pub range: ColorRange,
}

impl ColorSpace {
    /// BT.601 with limited range.
    pub const BT601: Self = Self::new(ColorMatrix::Bt601, ColorRange::Limited);
    /// BT.709 with limited range.
    pub const BT709: Self = Self::new(ColorMatrix::Bt709, ColorRange::Limited);
    /// BT.2020 with limited range.
    pub const BT2020: Self = Self::new(ColorMatrix::Bt2020, ColorRange::Limited);

    /// A colour space of `matrix` and `range`.
    pub const fn new(matrix: ColorMatrix, range: ColorRange) -> Self {
        Self { matrix, range }
    }

    /// The affine map from normalised `(Y′, Cb, Cr)` samples to R′G′B′.
    ///
    /// Each row is one output channel: three weights for Y′, Cb and Cr, then an offset. Samples are
    /// normalised the way a texture of a unorm format returns them, so the map holds for 8-bit
    /// planes exactly and for 10-bit planes stored in the high bits of 16 within a code value.
    pub fn transform(self) -> [[f32; 4]; 3] {
        let (kr, kb) = self.matrix.weights();
        let kg = 1.0 - kr - kb;
        // Y′ = luma_scale · (sample − luma_offset); Cb, Cr = chroma_scale · (sample − ½).
        let (luma_scale, luma_offset, chroma_scale) = match self.range {
            ColorRange::Limited => (255.0 / 219.0, 16.0 / 255.0, 255.0 / 224.0),
            ColorRange::Full => (1.0, 0.0, 1.0),
        };
        let chroma_offset = 128.0 / 255.0;

        let r_cr = 2.0 * (1.0 - kr) * chroma_scale;
        let g_cb = -2.0 * kb * (1.0 - kb) / kg * chroma_scale;
        let g_cr = -2.0 * kr * (1.0 - kr) / kg * chroma_scale;
        let b_cb = 2.0 * (1.0 - kb) * chroma_scale;
        let luma_base = -luma_scale * luma_offset;

        [
            [luma_scale, 0.0, r_cr, luma_base - r_cr * chroma_offset],
            [
                luma_scale,
                g_cb,
                g_cr,
                luma_base - (g_cb + g_cr) * chroma_offset,
            ],
            [luma_scale, b_cb, 0.0, luma_base - b_cb * chroma_offset],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies `space`'s map to 8-bit samples and returns 8-bit R′G′B′.
    fn rgb(space: ColorSpace, y: u8, cb: u8, cr: u8) -> [i32; 3] {
        let sample = [y, cb, cr].map(|v| f32::from(v) / 255.0);
        space.transform().map(|row| {
            let v = row[0] * sample[0] + row[1] * sample[1] + row[2] * sample[2] + row[3];
            (v.clamp(0.0, 1.0) * 255.0).round() as i32
        })
    }

    fn near(actual: [i32; 3], expected: [i32; 3]) -> bool {
        actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= 1)
    }

    #[test]
    fn limited_range_black_and_white_land_on_the_ends() {
        for matrix in [ColorMatrix::Bt601, ColorMatrix::Bt709, ColorMatrix::Bt2020] {
            let space = ColorSpace::new(matrix, ColorRange::Limited);
            assert_eq!(rgb(space, 16, 128, 128), [0, 0, 0], "{matrix:?}");
            assert_eq!(rgb(space, 235, 128, 128), [255, 255, 255], "{matrix:?}");
        }
    }

    #[test]
    fn full_range_uses_every_code_value() {
        let space = ColorSpace::new(ColorMatrix::Bt709, ColorRange::Full);
        assert_eq!(rgb(space, 0, 128, 128), [0, 0, 0]);
        assert_eq!(rgb(space, 255, 128, 128), [255, 255, 255]);
    }

    #[test]
    fn primaries_round_trip_through_published_sample_values() {
        // Limited-range code values of the 100% primaries, from the matrices' published tables.
        assert!(near(rgb(ColorSpace::BT601, 81, 90, 240), [255, 0, 0]));
        assert!(near(rgb(ColorSpace::BT601, 145, 54, 34), [0, 255, 0]));
        assert!(near(rgb(ColorSpace::BT601, 41, 240, 110), [0, 0, 255]));
        assert!(near(rgb(ColorSpace::BT709, 63, 102, 240), [255, 0, 0]));
        assert!(near(rgb(ColorSpace::BT709, 173, 42, 26), [0, 255, 0]));
        assert!(near(rgb(ColorSpace::BT709, 32, 240, 118), [0, 0, 255]));
    }

    #[test]
    fn the_matrices_disagree_on_a_saturated_colour() {
        assert_ne!(
            rgb(ColorSpace::BT601, 63, 102, 240),
            rgb(ColorSpace::BT709, 63, 102, 240)
        );
    }
}
