//! Oklab and its D65-referenced XYZ.
//!
//! Oklab is the perceptual space CSS reaches for by default when a gradient has to interpolate:
//! its hue lines stay straight where L\*a\*b\*'s bend, so a blue-to-white ramp does not swing
//! through purple. Its lightness runs `0..=1` and its `a` and `b` axes reach about ±0.4.
//!
//! The conversion is two matrices with a cube root between them: XYZ into a cone-response space,
//! a cube root there, and a second matrix into the opponent axes.

use crate::convert::matrix::{Matrix3, apply};

/// D65-referenced XYZ to the cone-response space.
const XYZ_TO_LMS: Matrix3 = [
    [0.819_022_4, 0.361_906_26, -0.128_873_78],
    [0.032_983_654, 0.929_286_85, 0.036_144_667],
    [0.048_177_19, 0.264_239_53, 0.633_547_8],
];

/// The cone-response space to D65-referenced XYZ.
const LMS_TO_XYZ: Matrix3 = [
    [1.226_88, -0.557_815, 0.281_391_05],
    [-0.040_575_745, 1.112_286_8, -0.071_711_06],
    [-0.076_372_94, -0.421_493_33, 1.586_924],
];

/// The cube-rooted cone responses to Oklab's opponent axes.
const LMS_TO_OKLAB: Matrix3 = [
    [0.210_454_27, 0.793_617_8, -0.004_072_043],
    [1.977_998_5, -2.428_592_2, 0.450_593_7],
    [0.025_904_042, 0.782_771_7, -0.808_675_75],
];

/// Oklab's opponent axes to the cube-rooted cone responses.
const OKLAB_TO_LMS: Matrix3 = [
    [1.0, 0.396_337_78, 0.215_803_76],
    [1.0, -0.105_561_346, -0.063_854_17],
    [1.0, -0.089_484_18, -1.291_485_5],
];

/// Converts D65-referenced XYZ to Oklab.
pub(crate) fn from_xyz_d65(xyz: [f32; 3]) -> [f32; 3] {
    // `cbrt` is odd, so a negative cone response — which an out-of-gamut colour produces — keeps
    // its sign instead of becoming a NaN.
    let cone = apply(&XYZ_TO_LMS, xyz).map(f32::cbrt);
    apply(&LMS_TO_OKLAB, cone)
}

/// Converts Oklab to D65-referenced XYZ.
pub(crate) fn to_xyz_d65(oklab: [f32; 3]) -> [f32; 3] {
    let cone = apply(&OKLAB_TO_LMS, oklab).map(|value| value * value * value);
    apply(&LMS_TO_XYZ, cone)
}

#[cfg(test)]
mod tests {
    use super::{LMS_TO_OKLAB, LMS_TO_XYZ, OKLAB_TO_LMS, XYZ_TO_LMS, from_xyz_d65, to_xyz_d65};
    use crate::convert::matrix::tests::assert_inverse;
    use crate::space::WhitePoint;

    #[test]
    fn both_matrix_pairs_are_inverses() {
        assert_inverse(&XYZ_TO_LMS, &LMS_TO_XYZ, 1e-4);
        assert_inverse(&LMS_TO_OKLAB, &OKLAB_TO_LMS, 1e-4);
    }

    #[test]
    fn the_white_point_is_lightness_one() {
        let oklab = from_xyz_d65(WhitePoint::D65.tristimulus());
        assert!((oklab[0] - 1.0).abs() < 1e-4, "lightness is {}", oklab[0]);
        assert!(oklab[1].abs() < 1e-4, "a is {}", oklab[1]);
        assert!(oklab[2].abs() < 1e-4, "b is {}", oklab[2]);
    }

    #[test]
    fn black_is_the_origin() {
        for channel in from_xyz_d65([0.0, 0.0, 0.0]) {
            assert!(channel.abs() < 1e-6);
        }
    }

    #[test]
    fn out_of_gamut_values_round_trip_rather_than_becoming_nan() {
        let xyz = [-0.05, 0.4, 1.2];
        let back = to_xyz_d65(from_xyz_d65(xyz));
        for channel in 0..3 {
            assert!(
                (back[channel] - xyz[channel]).abs() < 1e-5,
                "channel {channel} came back as {}",
                back[channel],
            );
        }
    }
}
