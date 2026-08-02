//! The primary matrices that take each RGB space's linear-light values to CIE XYZ, and back.
//!
//! Each matrix is derived from its space's primaries and its own white point, so the XYZ it
//! produces is referenced to that white point: the ProPhoto matrices below are D50-referenced and
//! every other one here is D65-referenced. Crossing between the two is
//! [`adapt`](crate::convert::adapt)'s job, not theirs.

use crate::convert::matrix::Matrix3;

/// Linear sRGB to XYZ, D65-referenced.
pub(crate) const SRGB_TO_XYZ: Matrix3 = [
    [0.412_390_8, 0.357_584_33, 0.180_480_8],
    [0.212_639, 0.715_168_7, 0.072_192_32],
    [0.019_330_818, 0.119_194_78, 0.950_532_2],
];

/// XYZ, D65-referenced, to linear sRGB.
pub(crate) const XYZ_TO_SRGB: Matrix3 = [
    [3.240_97, -1.537_383_2, -0.498_610_76],
    [-0.969_243_65, 1.875_967_5, 0.041_555_06],
    [0.055_630_08, -0.203_976_96, 1.056_971_5],
];

/// Linear Display P3 to XYZ, D65-referenced.
pub(crate) const DISPLAY_P3_TO_XYZ: Matrix3 = [
    [0.486_570_95, 0.265_667_7, 0.198_217_29],
    [0.228_974_56, 0.691_738_5, 0.079_286_91],
    [0.0, 0.045_113_38, 1.043_944_4],
];

/// XYZ, D65-referenced, to linear Display P3.
pub(crate) const XYZ_TO_DISPLAY_P3: Matrix3 = [
    [2.493_497, -0.931_383_6, -0.402_710_8],
    [-0.829_489, 1.762_664_1, 0.023_624_687],
    [0.035_845_83, -0.076_172_39, 0.956_884_5],
];

/// Linear Adobe RGB (1998) to XYZ, D65-referenced.
pub(crate) const A98_TO_XYZ: Matrix3 = [
    [0.576_669, 0.185_558_24, 0.188_228_65],
    [0.297_344_98, 0.627_363_6, 0.075_291_46],
    [0.027_031_36, 0.070_688_85, 0.991_337_5],
];

/// XYZ, D65-referenced, to linear Adobe RGB (1998).
pub(crate) const XYZ_TO_A98: Matrix3 = [
    [2.041_588, -0.565_007, -0.344_731_35],
    [-0.969_243_65, 1.875_967_5, 0.041_555_06],
    [0.013_444_281, -0.118_362_39, 1.015_175],
];

/// Linear ProPhoto RGB to XYZ, D50-referenced.
pub(crate) const PROPHOTO_TO_XYZ: Matrix3 = [
    [0.797_760_5, 0.135_185_84, 0.031_349_35],
    [0.288_071_13, 0.711_843_2, 0.000_085_653_96],
    [0.0, 0.0, 0.825_104_6],
];

/// XYZ, D50-referenced, to linear ProPhoto RGB.
pub(crate) const XYZ_TO_PROPHOTO: Matrix3 = [
    [1.345_799, -0.255_580_1, -0.051_106_285],
    [-0.544_622_5, 1.508_232_7, 0.020_536_033],
    [0.0, 0.0, 1.211_967_5],
];

/// Linear BT.2020 to XYZ, D65-referenced.
pub(crate) const REC2020_TO_XYZ: Matrix3 = [
    [0.636_958_05, 0.144_616_9, 0.168_880_98],
    [0.262_700_2, 0.677_998_07, 0.059_301_715],
    [0.0, 0.028_072_693, 1.060_985],
];

/// XYZ, D65-referenced, to linear BT.2020.
pub(crate) const XYZ_TO_REC2020: Matrix3 = [
    [1.716_651_2, -0.355_670_78, -0.253_366_3],
    [-0.666_684_4, 1.616_481_2, 0.015_768_546],
    [0.017_639_857, -0.042_770_613, 0.942_103_1],
];

#[cfg(test)]
mod tests {
    use super::{
        A98_TO_XYZ, DISPLAY_P3_TO_XYZ, PROPHOTO_TO_XYZ, REC2020_TO_XYZ, SRGB_TO_XYZ, XYZ_TO_A98,
        XYZ_TO_DISPLAY_P3, XYZ_TO_PROPHOTO, XYZ_TO_REC2020, XYZ_TO_SRGB,
    };
    use crate::convert::matrix::Matrix3;
    use crate::convert::matrix::tests::assert_inverse;
    use crate::space::WhitePoint;

    /// Every matrix pair with the white point its XYZ is referenced to.
    const PAIRS: [(&Matrix3, &Matrix3, WhitePoint); 5] = [
        (&SRGB_TO_XYZ, &XYZ_TO_SRGB, WhitePoint::D65),
        (&DISPLAY_P3_TO_XYZ, &XYZ_TO_DISPLAY_P3, WhitePoint::D65),
        (&A98_TO_XYZ, &XYZ_TO_A98, WhitePoint::D65),
        (&PROPHOTO_TO_XYZ, &XYZ_TO_PROPHOTO, WhitePoint::D50),
        (&REC2020_TO_XYZ, &XYZ_TO_REC2020, WhitePoint::D65),
    ];

    #[test]
    fn every_pair_is_a_matched_inverse() {
        for (forward, backward, _) in PAIRS {
            assert_inverse(forward, backward, 1e-4);
        }
    }

    #[test]
    fn white_maps_to_the_white_point() {
        for (forward, _, white_point) in PAIRS {
            let white = crate::convert::matrix::apply(forward, [1.0, 1.0, 1.0]);
            let expected = white_point.tristimulus();
            for channel in 0..3 {
                assert!(
                    (white[channel] - expected[channel]).abs() < 1e-4,
                    "channel {channel} is {}, expected {}",
                    white[channel],
                    expected[channel],
                );
            }
        }
    }
}
