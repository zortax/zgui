//! The matrix that derived Y′CbCr from R′G′B′, and the affine map that undoes it.

use super::ColorRange;

/// How luma and chroma were derived from gamma-encoded R′G′B′.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorMatrix {
    /// No matrix: the planes hold G′, B′ and R′ in the luma, Cb and Cr positions.
    Identity,
    /// ITU-R BT.601: standard-definition video and most JPEG-derived content.
    Bt601,
    /// ITU-R BT.709: high-definition video and screen content.
    #[default]
    Bt709,
    /// ITU-R BT.2020 non-constant luminance: wide-gamut and HDR video.
    Bt2020,
    /// SMPTE 240M: early high-definition video.
    Smpte240,
    /// FCC 73.682: the original NTSC weights.
    Fcc,
}

impl ColorMatrix {
    /// The red and blue luma weights, `Kr` and `Kb`.
    pub(crate) fn weights(self) -> (f32, f32) {
        match self {
            Self::Identity | Self::Bt709 => (0.2126, 0.0722),
            Self::Bt601 => (0.299, 0.114),
            Self::Bt2020 => (0.2627, 0.0593),
            Self::Smpte240 => (0.212, 0.087),
            Self::Fcc => (0.30, 0.11),
        }
    }
}

/// The affine map from normalised `(Y′, Cb, Cr)` codes to R′G′B′.
///
/// A normalised code is the code divided by `2^bits − 1`. Each row is one output channel: three
/// weights for the luma, Cb and Cr planes, then an offset.
pub(crate) fn affine(matrix: ColorMatrix, range: ColorRange, bits: u8) -> [[f32; 4]; 3] {
    let max = f64::from((1u32 << bits) - 1);
    let step = f64::from(1u32 << (bits - 8));
    // Each plane decodes as `scale · code + offset`, with codes normalised to [0, 1].
    let ((luma_scale, luma_offset), (chroma_scale, chroma_offset)) = match range {
        ColorRange::Limited => (
            (max / (219.0 * step), -16.0 / 219.0),
            (max / (224.0 * step), -128.0 / 224.0),
        ),
        ColorRange::Full => ((1.0, 0.0), (1.0, -f64::from(1u32 << (bits - 1)) / max)),
    };

    let row = |[y, cb, cr]: [f64; 3]| -> [f32; 4] {
        [
            (y * luma_scale) as f32,
            (cb * chroma_scale) as f32,
            (cr * chroma_scale) as f32,
            (y * luma_offset + (cb + cr) * chroma_offset) as f32,
        ]
    };

    if matrix == ColorMatrix::Identity {
        // G′B′R′ planes: every plane decodes as luma does.
        let plane = |index: usize| -> [f32; 4] {
            let mut out = [0.0; 4];
            out[index] = luma_scale as f32;
            out[3] = luma_offset as f32;
            out
        };
        return [plane(2), plane(0), plane(1)];
    }

    let (kr, kb) = matrix.weights();
    let (kr, kb) = (f64::from(kr), f64::from(kb));
    let kg = 1.0 - kr - kb;
    [
        row([1.0, 0.0, 2.0 * (1.0 - kr)]),
        row([
            1.0,
            -2.0 * kb * (1.0 - kb) / kg,
            -2.0 * kr * (1.0 - kr) / kg,
        ]),
        row([1.0, 2.0 * (1.0 - kb), 0.0]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies the map to codes of depth `bits` and returns 8-bit R′G′B′.
    fn rgb(matrix: ColorMatrix, range: ColorRange, bits: u8, codes: [u32; 3]) -> [i32; 3] {
        let max = ((1u32 << bits) - 1) as f32;
        let sample = codes.map(|c| c as f32 / max);
        affine(matrix, range, bits).map(|row| {
            let v = row[0] * sample[0] + row[1] * sample[1] + row[2] * sample[2] + row[3];
            (v.clamp(0.0, 1.0) * 255.0).round() as i32
        })
    }

    fn near(actual: [i32; 3], expected: [i32; 3]) -> bool {
        actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= 1)
    }

    use ColorMatrix::*;
    use ColorRange::*;

    #[test]
    fn limited_range_black_and_white_land_on_the_ends_at_every_depth() {
        for matrix in [Bt601, Bt709, Bt2020, Smpte240, Fcc] {
            assert_eq!(rgb(matrix, Limited, 8, [16, 128, 128]), [0, 0, 0]);
            assert_eq!(rgb(matrix, Limited, 8, [235, 128, 128]), [255; 3]);
            assert_eq!(rgb(matrix, Limited, 10, [64, 512, 512]), [0, 0, 0]);
            assert_eq!(rgb(matrix, Limited, 10, [940, 512, 512]), [255; 3]);
            assert_eq!(rgb(matrix, Limited, 12, [256, 2048, 2048]), [0, 0, 0]);
            assert_eq!(rgb(matrix, Limited, 12, [3760, 2048, 2048]), [255; 3]);
        }
    }

    #[test]
    fn full_range_uses_every_code_value() {
        assert_eq!(rgb(Bt709, Full, 8, [0, 128, 128]), [0, 0, 0]);
        assert_eq!(rgb(Bt709, Full, 8, [255, 128, 128]), [255; 3]);
        assert_eq!(rgb(Bt709, Full, 10, [1023, 512, 512]), [255; 3]);
    }

    #[test]
    fn primaries_round_trip_through_published_code_values() {
        assert!(near(rgb(Bt601, Limited, 8, [81, 90, 240]), [255, 0, 0]));
        assert!(near(rgb(Bt601, Limited, 8, [145, 54, 34]), [0, 255, 0]));
        assert!(near(rgb(Bt601, Limited, 8, [41, 240, 110]), [0, 0, 255]));
        assert!(near(rgb(Bt709, Limited, 8, [63, 102, 240]), [255, 0, 0]));
        assert!(near(rgb(Bt709, Limited, 8, [173, 42, 26]), [0, 255, 0]));
        assert!(near(rgb(Bt709, Limited, 8, [32, 240, 118]), [0, 0, 255]));
        assert!(near(rgb(Bt709, Limited, 10, [250, 409, 960]), [255, 0, 0]));
    }

    #[test]
    fn the_identity_matrix_reads_gbr_planes() {
        assert_eq!(rgb(Identity, Full, 8, [10, 20, 30]), [30, 10, 20]);
        assert_eq!(rgb(Identity, Limited, 8, [16, 235, 16]), [0, 0, 255]);
    }

    #[test]
    fn the_matrices_disagree_on_a_saturated_colour() {
        assert_ne!(
            rgb(Bt601, Limited, 8, [63, 102, 240]),
            rgb(Bt709, Limited, 8, [63, 102, 240])
        );
    }
}
