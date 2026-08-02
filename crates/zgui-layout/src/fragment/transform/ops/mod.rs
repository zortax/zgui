//! One transform function, as a matrix.
//!
//! Every function is written here in the same convention: a point is a column vector multiplied on
//! the right, so a translation lives in the last column and the composition of two functions is
//! the product of their matrices in application order.

use zgui_css::computed::style::style_structs;
use zgui_css::values::length::evaluate_at;
use zgui_css::values::transform::{
    RotateValue, ScaleValue, TransformOperationValue, TranslateValue,
};
use zgui_geom::{CssPx, Matrix4};

/// The matrices `translate`, `rotate` and `scale` contribute, in the order they apply: the box is
/// scaled, then rotated, then translated.
///
/// They are separate properties rather than entries of `transform` so that each can be animated on
/// its own, and the order between them is fixed by the specification rather than by the order they
/// were written in.
pub fn individual(box_: &style_structs::Box, width: f32, height: f32, scale: f32) -> [Matrix4; 3] {
    [
        scale_matrix(&box_.scale),
        rotate_matrix(&box_.rotate),
        translate_matrix(&box_.translate, width, height, scale),
    ]
}

/// The matrix for one entry of a `transform` list.
pub fn operation(value: &TransformOperationValue, width: f32, height: f32, scale: f32) -> Matrix4 {
    match value {
        TransformOperationValue::Matrix(matrix) => Matrix4::from_columns([
            [matrix.a, matrix.b, 0.0, 0.0],
            [matrix.c, matrix.d, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [matrix.e * scale, matrix.f * scale, 0.0, 1.0],
        ]),
        TransformOperationValue::Matrix3D(matrix) => Matrix4::from_columns([
            [matrix.m11, matrix.m12, matrix.m13, matrix.m14],
            [matrix.m21, matrix.m22, matrix.m23, matrix.m24],
            [matrix.m31, matrix.m32, matrix.m33, matrix.m34],
            [
                matrix.m41 * scale,
                matrix.m42 * scale,
                matrix.m43 * scale,
                matrix.m44,
            ],
        ]),
        TransformOperationValue::Skew(x, y) => skew(x.radians(), y.radians()),
        TransformOperationValue::SkewX(x) => skew(x.radians(), 0.0),
        TransformOperationValue::SkewY(y) => skew(0.0, y.radians()),
        TransformOperationValue::Translate(x, y) => {
            Matrix4::translation(length(x, width, scale), length(y, height, scale), 0.0)
        }
        TransformOperationValue::TranslateX(x) => {
            Matrix4::translation(length(x, width, scale), 0.0, 0.0)
        }
        TransformOperationValue::TranslateY(y) => {
            Matrix4::translation(0.0, length(y, height, scale), 0.0)
        }
        TransformOperationValue::TranslateZ(z) => Matrix4::translation(0.0, 0.0, z.px() * scale),
        TransformOperationValue::Translate3D(x, y, z) => Matrix4::translation(
            length(x, width, scale),
            length(y, height, scale),
            z.px() * scale,
        ),
        TransformOperationValue::Scale(x, y) => Matrix4::scale(*x, *y, 1.0),
        TransformOperationValue::ScaleX(x) => Matrix4::scale(*x, 1.0, 1.0),
        TransformOperationValue::ScaleY(y) => Matrix4::scale(1.0, *y, 1.0),
        TransformOperationValue::ScaleZ(z) => Matrix4::scale(1.0, 1.0, *z),
        TransformOperationValue::Scale3D(x, y, z) => Matrix4::scale(*x, *y, *z),
        TransformOperationValue::Rotate(angle) | TransformOperationValue::RotateZ(angle) => {
            rotate_z(angle.radians())
        }
        TransformOperationValue::RotateX(angle) => rotate_x(angle.radians()),
        TransformOperationValue::RotateY(angle) => rotate_y(angle.radians()),
        TransformOperationValue::Rotate3D(x, y, z, angle) => {
            rotate_axis(*x, *y, *z, angle.radians())
        }
        TransformOperationValue::Perspective(depth) => {
            Matrix4::perspective(depth.infinity_or(|length| length.px() * scale))
        }
        // An interpolation between two lists that could not be matched up function by function is
        // resolved by the animation that produced it before it ever reaches a fragment; there is
        // no geometry to read out of the unresolved form.
        TransformOperationValue::InterpolateMatrix { .. }
        | TransformOperationValue::AccumulateMatrix { .. } => Matrix4::IDENTITY,
    }
}

/// The `translate` property's matrix.
fn translate_matrix(value: &TranslateValue, width: f32, height: f32, scale: f32) -> Matrix4 {
    match value {
        TranslateValue::None => Matrix4::IDENTITY,
        TranslateValue::Translate(x, y, z) => Matrix4::translation(
            length(x, width, scale),
            length(y, height, scale),
            z.px() * scale,
        ),
    }
}

/// The `rotate` property's matrix.
fn rotate_matrix(value: &RotateValue) -> Matrix4 {
    match value {
        RotateValue::None => Matrix4::IDENTITY,
        RotateValue::Rotate(angle) => rotate_z(angle.radians()),
        RotateValue::Rotate3D(x, y, z, angle) => rotate_axis(*x, *y, *z, angle.radians()),
    }
}

/// The `scale` property's matrix.
fn scale_matrix(value: &ScaleValue) -> Matrix4 {
    match value {
        ScaleValue::None => Matrix4::IDENTITY,
        ScaleValue::Scale(x, y, z) => Matrix4::scale(*x, *y, *z),
    }
}

/// One translation component in device pixels, with a percentage taken of the box's own extent.
fn length(value: &zgui_css::values::length::LengthPercentage, basis: f32, scale: f32) -> f32 {
    evaluate_at(value, CssPx(basis / scale)).0 * scale
}

/// A skew by two angles, in radians.
fn skew(x_radians: f32, y_radians: f32) -> Matrix4 {
    Matrix4::from_rows([
        [1.0, x_radians.tan(), 0.0, 0.0],
        [y_radians.tan(), 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// A rotation about the z axis, which is the one a two-dimensional rotation means.
fn rotate_z(radians: f32) -> Matrix4 {
    let (sin, cos) = radians.sin_cos();
    Matrix4::from_rows([
        [cos, -sin, 0.0, 0.0],
        [sin, cos, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// A rotation about the x axis.
fn rotate_x(radians: f32) -> Matrix4 {
    let (sin, cos) = radians.sin_cos();
    Matrix4::from_rows([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, cos, -sin, 0.0],
        [0.0, sin, cos, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// A rotation about the y axis.
fn rotate_y(radians: f32) -> Matrix4 {
    let (sin, cos) = radians.sin_cos();
    Matrix4::from_rows([
        [cos, 0.0, sin, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [-sin, 0.0, cos, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// A rotation about an arbitrary axis, by Rodrigues' formula.
///
/// A degenerate axis rotates nothing, which is what the specification says for a zero vector.
fn rotate_axis(x: f32, y: f32, z: f32, radians: f32) -> Matrix4 {
    let magnitude = (x * x + y * y + z * z).sqrt();
    if magnitude == 0.0 || !magnitude.is_finite() {
        return Matrix4::IDENTITY;
    }
    let (x, y, z) = (x / magnitude, y / magnitude, z / magnitude);
    let (sin, cos) = radians.sin_cos();
    let one = 1.0 - cos;
    Matrix4::from_rows([
        [
            cos + x * x * one,
            x * y * one - z * sin,
            x * z * one + y * sin,
            0.0,
        ],
        [
            y * x * one + z * sin,
            cos + y * y * one,
            y * z * one - x * sin,
            0.0,
        ],
        [
            z * x * one - y * sin,
            z * y * one + x * sin,
            cos + z * z * one,
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

#[cfg(test)]
mod tests {
    use zgui_geom::Matrix4;

    use super::{rotate_axis, rotate_z, skew};

    #[test]
    fn a_quarter_turn_about_z_sends_the_x_axis_to_the_y_axis() {
        let matrix = rotate_z(core::f32::consts::FRAC_PI_2);
        let point = matrix.transform_point(1.0, 0.0, 0.0);
        assert!((point[0] - 0.0).abs() < 1.0e-6, "{point:?}");
        assert!((point[1] - 1.0).abs() < 1.0e-6, "{point:?}");
    }

    #[test]
    fn rotating_about_the_z_axis_by_name_and_by_vector_agree() {
        let by_name = rotate_z(0.7);
        let by_vector = rotate_axis(0.0, 0.0, 1.0, 0.7);
        for column in 0..4 {
            for row in 0..4 {
                let difference =
                    (by_name.columns[column][row] - by_vector.columns[column][row]).abs();
                assert!(difference < 1.0e-6, "column {column} row {row}");
            }
        }
    }

    #[test]
    fn a_degenerate_rotation_axis_rotates_nothing() {
        assert_eq!(rotate_axis(0.0, 0.0, 0.0, 1.0), Matrix4::IDENTITY);
    }

    #[test]
    fn a_horizontal_skew_moves_points_by_their_height() {
        let matrix = skew(core::f32::consts::FRAC_PI_4, 0.0);
        let point = matrix.transform_point(0.0, 2.0, 0.0);
        assert!((point[0] - 2.0).abs() < 1.0e-5, "{point:?}");
        assert!((point[1] - 2.0).abs() < 1.0e-6, "{point:?}");
    }
}
