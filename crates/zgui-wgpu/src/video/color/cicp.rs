//! H.273 code points to the colour-space enums.

use super::{ColorMatrix, ColorPrimaries, TransferFunction};

/// `colour_primaries`.
pub(super) fn primaries(code: u8) -> ColorPrimaries {
    match code {
        5 => ColorPrimaries::Bt470Bg,
        6 | 7 => ColorPrimaries::Bt601,
        9 => ColorPrimaries::Bt2020,
        12 => ColorPrimaries::DisplayP3,
        _ => ColorPrimaries::Bt709,
    }
}

/// `transfer_characteristics`.
pub(super) fn transfer(code: u8) -> TransferFunction {
    match code {
        4 => TransferFunction::Gamma22,
        5 => TransferFunction::Gamma28,
        8 => TransferFunction::Linear,
        13 => TransferFunction::Srgb,
        16 => TransferFunction::Pq,
        18 => TransferFunction::Hlg,
        _ => TransferFunction::Bt709,
    }
}

/// `matrix_coefficients`.
pub(super) fn matrix(code: u8) -> ColorMatrix {
    match code {
        0 => ColorMatrix::Identity,
        4 => ColorMatrix::Fcc,
        5 | 6 => ColorMatrix::Bt601,
        7 => ColorMatrix::Smpte240,
        9 | 10 => ColorMatrix::Bt2020,
        _ => ColorMatrix::Bt709,
    }
}

#[cfg(test)]
mod tests {
    use crate::video::{ColorRange, ColorSpace};

    #[test]
    fn the_common_code_points_name_the_common_spaces() {
        assert_eq!(ColorSpace::from_cicp(1, 1, 1, false), ColorSpace::BT709);
        assert_eq!(ColorSpace::from_cicp(6, 6, 6, false), ColorSpace::BT601);
        assert_eq!(
            ColorSpace::from_cicp(9, 16, 9, false),
            ColorSpace::BT2100_PQ
        );
        assert_eq!(
            ColorSpace::from_cicp(9, 18, 9, false),
            ColorSpace::BT2100_HLG
        );
        assert_eq!(
            ColorSpace::from_cicp(1, 13, 0, true).range,
            ColorRange::Full
        );
    }

    #[test]
    fn unspecified_and_reserved_code_points_read_as_bt709() {
        assert_eq!(ColorSpace::from_cicp(2, 2, 2, false), ColorSpace::BT709);
        assert_eq!(
            ColorSpace::from_cicp(200, 200, 200, false),
            ColorSpace::BT709
        );
    }
}
