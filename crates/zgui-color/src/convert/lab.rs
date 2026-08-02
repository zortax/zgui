//! CIE L\*a\*b\* and its D50-referenced XYZ.
//!
//! L\*a\*b\* is a perceptual space: equal steps in it are meant to look like equal steps, which is
//! why CSS offers it for interpolation. Its lightness runs `0..=100`, its `a` and `b` axes are
//! unbounded in principle and reach about ±160 for real colours, and its reference white is D50.

use crate::space::WhitePoint;

/// The CIE standard's `ε`, `216/24389`, where the cube-root segment of the curve begins.
const EPSILON: f32 = 216.0 / 24389.0;

/// The CIE standard's `κ`, `24389/27`, the slope of the linear segment.
const KAPPA: f32 = 24389.0 / 27.0;

/// The non-linearity applied to each white-point-relative channel.
fn forward(ratio: f32) -> f32 {
    if ratio > EPSILON {
        ratio.cbrt()
    } else {
        (KAPPA * ratio + 16.0) / 116.0
    }
}

/// Converts D50-referenced XYZ to L\*a\*b\*.
pub(crate) fn from_xyz_d50(xyz: [f32; 3]) -> [f32; 3] {
    let white = WhitePoint::D50.tristimulus();
    let [fx, fy, fz] = [0, 1, 2].map(|channel| forward(xyz[channel] / white[channel]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// Converts L\*a\*b\* to D50-referenced XYZ.
pub(crate) fn to_xyz_d50(lab: [f32; 3]) -> [f32; 3] {
    let [lightness, a, b] = lab;
    let fy = (lightness + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;

    let x = if fx.powi(3) > EPSILON {
        fx.powi(3)
    } else {
        (116.0 * fx - 16.0) / KAPPA
    };
    let y = if lightness > KAPPA * EPSILON {
        ((lightness + 16.0) / 116.0).powi(3)
    } else {
        lightness / KAPPA
    };
    let z = if fz.powi(3) > EPSILON {
        fz.powi(3)
    } else {
        (116.0 * fz - 16.0) / KAPPA
    };

    let white = WhitePoint::D50.tristimulus();
    [x * white[0], y * white[1], z * white[2]]
}

#[cfg(test)]
mod tests {
    use super::{from_xyz_d50, to_xyz_d50};
    use crate::space::WhitePoint;

    #[test]
    fn the_white_point_is_lightness_one_hundred() {
        let lab = from_xyz_d50(WhitePoint::D50.tristimulus());
        assert!((lab[0] - 100.0).abs() < 1e-3, "lightness is {}", lab[0]);
        assert!(lab[1].abs() < 1e-3, "a is {}", lab[1]);
        assert!(lab[2].abs() < 1e-3, "b is {}", lab[2]);
    }

    #[test]
    fn black_is_the_origin() {
        let lab = from_xyz_d50([0.0, 0.0, 0.0]);
        assert_eq!(lab, [0.0, 0.0, 0.0]);
        assert_eq!(to_xyz_d50([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn a_mid_grey_round_trips() {
        let xyz = [0.2, 0.21, 0.17];
        let back = to_xyz_d50(from_xyz_d50(xyz));
        for channel in 0..3 {
            assert!((back[channel] - xyz[channel]).abs() < 1e-5);
        }
    }

    #[test]
    fn the_linear_segment_round_trips() {
        // Below `ε` the curve is a straight line, and it is the segment a naive implementation
        // gets wrong: values this dark are where banding in a dark gradient would show.
        let xyz = [0.000_5, 0.000_4, 0.000_6];
        let back = to_xyz_d50(from_xyz_d50(xyz));
        for channel in 0..3 {
            assert!((back[channel] - xyz[channel]).abs() < 1e-7);
        }
    }
}
