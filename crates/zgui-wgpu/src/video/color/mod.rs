//! What a video frame's samples mean, in the terms every codec signals.
//!
//! AV1, H.264, HEVC and VP9 describe colour with the four code points of ITU-T H.273: the colour
//! primaries, the transfer characteristics, the matrix coefficients and the range. A
//! [`ColorSpace`] holds the same four, and [`ColorSpace::from_cicp`] reads them as a bitstream
//! states them. The conversion turns any of them into the gamma-encoded BT.709 colour the
//! compositor works in.

mod cicp;
mod matrix;
mod primaries;
pub(crate) mod transfer;

pub use matrix::ColorMatrix;
pub use primaries::ColorPrimaries;
pub use transfer::TransferFunction;

pub(crate) use matrix::affine;
pub(crate) use primaries::Gamut;

/// Which part of the code range holds the signal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorRange {
    /// Luma in 16–235 and chroma in 16–240, scaled to the bit depth. Most video uses it.
    #[default]
    Limited,
    /// Every code value carries signal. JPEG and most screen capture use it.
    Full,
}

/// The primaries, transfer, matrix and range of one frame's samples.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColorSpace {
    /// The red, green and blue the samples are mixed from.
    pub primaries: ColorPrimaries,
    /// How light is encoded in R′G′B′.
    pub transfer: TransferFunction,
    /// How Y′CbCr was derived from R′G′B′.
    pub matrix: ColorMatrix,
    /// The part of the code range that holds the signal.
    pub range: ColorRange,
}

impl ColorSpace {
    /// Standard-definition video: BT.601 matrix and primaries, limited range.
    pub const BT601: Self = Self {
        primaries: ColorPrimaries::Bt601,
        transfer: TransferFunction::Bt709,
        matrix: ColorMatrix::Bt601,
        range: ColorRange::Limited,
    };
    /// High-definition video: BT.709 throughout, limited range.
    pub const BT709: Self = Self {
        primaries: ColorPrimaries::Bt709,
        transfer: TransferFunction::Bt709,
        matrix: ColorMatrix::Bt709,
        range: ColorRange::Limited,
    };
    /// Wide-gamut standard-dynamic-range video: BT.2020, limited range.
    pub const BT2020: Self = Self {
        primaries: ColorPrimaries::Bt2020,
        transfer: TransferFunction::Bt709,
        matrix: ColorMatrix::Bt2020,
        range: ColorRange::Limited,
    };
    /// HDR10: BT.2020 with the perceptual quantizer, limited range.
    pub const BT2100_PQ: Self = Self {
        transfer: TransferFunction::Pq,
        ..Self::BT2020
    };
    /// Broadcast HDR: BT.2020 with hybrid log-gamma, limited range.
    pub const BT2100_HLG: Self = Self {
        transfer: TransferFunction::Hlg,
        ..Self::BT2020
    };

    /// The colour space a bitstream states as H.273 code points.
    ///
    /// `primaries`, `transfer` and `matrix` are the `colour_primaries`,
    /// `transfer_characteristics` and `matrix_coefficients` fields; `full_range` is the
    /// `video_full_range_flag`. A code point that is unspecified, reserved or unsupported reads as
    /// its BT.709 value.
    pub fn from_cicp(primaries: u8, transfer: u8, matrix: u8, full_range: bool) -> Self {
        Self {
            primaries: cicp::primaries(primaries),
            transfer: cicp::transfer(transfer),
            matrix: cicp::matrix(matrix),
            range: if full_range {
                ColorRange::Full
            } else {
                ColorRange::Limited
            },
        }
    }

    /// The same colour space with `range`.
    #[must_use]
    pub const fn with_range(self, range: ColorRange) -> Self {
        Self { range, ..self }
    }

    /// Whether the transfer function carries high dynamic range.
    pub fn is_hdr(self) -> bool {
        self.transfer.is_hdr()
    }

    /// Whether the decoded R′G′B′ is already the compositor's colour.
    ///
    /// True for BT.709 or sRGB encoding on BT.709 primaries. Such a frame skips linearisation,
    /// gamut mapping and tone mapping.
    pub(crate) fn is_native(self) -> bool {
        self.primaries == ColorPrimaries::Bt709
            && matches!(
                self.transfer,
                TransferFunction::Bt709 | TransferFunction::Srgb
            )
    }
}

impl Default for ColorSpace {
    fn default() -> Self {
        Self::BT709
    }
}
