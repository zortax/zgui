//! Taking a 4x4 transform apart and putting it back together.

use crate::transform::Matrix4;

/// A [`Matrix4`] split into the parts a transform animation interpolates.
///
/// Interpolating two matrices entry by entry is wrong: halfway between a 0-degree and a
/// 180-degree rotation it produces a matrix that collapses the content to a line rather than one
/// that has turned 90 degrees. Splitting the transform into translation, scale, skew, perspective
/// and a rotation quaternion, interpolating those, and multiplying them back together is what
/// produces the motion an author expects.
///
/// ```
/// use zgui_geom::Matrix4;
///
/// let matrix = Matrix4::translation(10.0, 20.0, 0.0).then(&Matrix4::scale(2.0, 3.0, 1.0));
/// let parts = matrix.decompose().expect("decomposable");
/// assert_eq!(parts.scale, [2.0, 3.0, 1.0]);
/// assert_eq!(parts.translation, [20.0, 60.0, 0.0]);
/// ```
///
/// [`Decomposed::recompose`] inverts [`Matrix4::decompose`] up to floating-point rounding. It
/// reproduces the matrix in its normalised form — see [`Matrix4::normalized`] — because an overall
/// factor on a homogeneous matrix cancels in the perspective divide and so carries no information
/// about the transform.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Decomposed {
    /// The translation along x, y and z.
    pub translation: [f32; 3],
    /// The scale factors along x, y and z. A negative factor is a reflection.
    pub scale: [f32; 3],
    /// The shear factors, in the order xy, xz, yz.
    pub skew: [f32; 3],
    /// The perspective row, as `[x, y, z, w]`. Without perspective this is `[0, 0, 0, 1]`.
    pub perspective: [f32; 4],
    /// The rotation as a unit quaternion, in the order `[x, y, z, w]`.
    pub rotation: [f32; 4],
}

impl Decomposed {
    /// The decomposition that recomposes to the identity.
    pub const IDENTITY: Self = Self {
        translation: [0.0; 3],
        scale: [1.0; 3],
        skew: [0.0; 3],
        perspective: [0.0, 0.0, 0.0, 1.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
    };

    /// Splits `matrix` into its parts, or returns `None` when it collapses space.
    pub(crate) fn of(matrix: &Matrix4) -> Option<Self> {
        // Work with the matrix normalised so that its homogeneous scale is one; the factor that
        // divides out does not change the transform, only the numbers that express it.
        let m = matrix.normalized()?.columns;

        // The same matrix with the perspective row removed. It is what is left once perspective
        // is factored out, and its invertibility is the test for a usable decomposition.
        let mut without_perspective = m;
        for column in without_perspective.iter_mut().take(3) {
            column[3] = 0.0;
        }
        without_perspective[3][3] = 1.0;
        let rest = Matrix4::from_columns(without_perspective);
        let inverse_rest = rest.invert()?;

        let perspective = if m[0][3] != 0.0 || m[1][3] != 0.0 || m[2][3] != 0.0 {
            let right_hand_side = [m[0][3], m[1][3], m[2][3], m[3][3]];
            inverse_rest.transpose().transform_vector4(right_hand_side)
        } else {
            [0.0, 0.0, 0.0, 1.0]
        };

        let translation = [m[3][0], m[3][1], m[3][2]];

        // The three basis vectors, which carry the scale, the skew and the rotation between them.
        let mut basis = [
            [m[0][0], m[0][1], m[0][2]],
            [m[1][0], m[1][1], m[1][2]],
            [m[2][0], m[2][1], m[2][2]],
        ];
        let mut scale = [0.0_f32; 3];
        let mut skew = [0.0_f32; 3];

        scale[0] = length(basis[0]);
        basis[0] = unit_vector(basis[0], scale[0])?;

        skew[0] = dot(basis[0], basis[1]);
        basis[1] = combine(basis[1], basis[0], -skew[0]);
        scale[1] = length(basis[1]);
        basis[1] = unit_vector(basis[1], scale[1])?;
        skew[0] /= scale[1];

        skew[1] = dot(basis[0], basis[2]);
        basis[2] = combine(basis[2], basis[0], -skew[1]);
        skew[2] = dot(basis[1], basis[2]);
        basis[2] = combine(basis[2], basis[1], -skew[2]);
        scale[2] = length(basis[2]);
        basis[2] = unit_vector(basis[2], scale[2])?;
        skew[1] /= scale[2];
        skew[2] /= scale[2];

        // A left-handed basis means the transform reflects; move the reflection into the scale so
        // that what is left is a pure rotation.
        if dot(basis[0], cross(basis[1], basis[2])) < 0.0 {
            for axis in 0..3 {
                scale[axis] = -scale[axis];
                basis[axis] = [-basis[axis][0], -basis[axis][1], -basis[axis][2]];
            }
        }

        let rotation = quaternion_of(&basis);

        Some(Self {
            translation,
            scale,
            skew,
            perspective,
            rotation,
        })
    }

    /// Multiplies the parts back into a single matrix.
    ///
    /// The result is normalised, so it equals [`Matrix4::normalized`] of the matrix the parts came
    /// from rather than that matrix itself.
    ///
    /// ```
    /// use zgui_geom::Matrix4;
    ///
    /// let matrix = Matrix4::translation(4.0, 0.0, 0.0).then(&Matrix4::scale(1.0, 2.0, 3.0));
    /// let round_tripped = matrix.decompose().expect("decomposable").recompose();
    /// for column in 0..4 {
    ///     for row in 0..4 {
    ///         let difference = matrix.get(row, column) - round_tripped.get(row, column);
    ///         assert!(difference.abs() < 1e-5);
    ///     }
    /// }
    /// ```
    pub fn recompose(&self) -> Matrix4 {
        let mut matrix = Matrix4::IDENTITY;
        for column in 0..4 {
            matrix.columns[column][3] = self.perspective[column];
        }

        let mut translation = Matrix4::IDENTITY;
        translation.columns[3] = [
            self.translation[0],
            self.translation[1],
            self.translation[2],
            1.0,
        ];
        matrix = multiply(&matrix, &translation);
        matrix = multiply(&matrix, &rotation_matrix(self.rotation));

        let mut shear = Matrix4::IDENTITY;
        shear.columns[1][0] = self.skew[0];
        shear.columns[2][0] = self.skew[1];
        shear.columns[2][1] = self.skew[2];
        matrix = multiply(&matrix, &shear);

        for axis in 0..3 {
            for row in 0..4 {
                matrix.columns[axis][row] *= self.scale[axis];
            }
        }
        matrix
    }
}

impl Default for Decomposed {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// The matrix product `left * right`.
fn multiply(left: &Matrix4, right: &Matrix4) -> Matrix4 {
    right.then(left)
}

/// The rotation matrix of a unit quaternion `[x, y, z, w]`.
fn rotation_matrix([x, y, z, w]: [f32; 4]) -> Matrix4 {
    Matrix4::from_columns([
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y + z * w),
            2.0 * (x * z - y * w),
            0.0,
        ],
        [
            2.0 * (x * y - z * w),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z + x * w),
            0.0,
        ],
        [
            2.0 * (x * z + y * w),
            2.0 * (y * z - x * w),
            1.0 - 2.0 * (x * x + y * y),
            0.0,
        ],
        [0.0, 0.0, 0.0, 1.0],
    ])
}

/// The unit quaternion of an orthonormal basis held as three column vectors.
///
/// Only the largest component is taken from the matrix diagonal; the other three come from
/// off-diagonal sums and differences. Taking every component from the diagonal loses a small one
/// to cancellation — a rotation of 1e-4 radians has `1 - cos` below f32's resolution at one, and
/// a quaternion built that way drops the rotation entirely.
fn quaternion_of(basis: &[[f32; 3]; 3]) -> [f32; 4] {
    let trace = basis[0][0] + basis[1][1] + basis[2][2];
    if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        return [
            (basis[1][2] - basis[2][1]) / s,
            (basis[2][0] - basis[0][2]) / s,
            (basis[0][1] - basis[1][0]) / s,
            0.25 * s,
        ];
    }
    if basis[0][0] > basis[1][1] && basis[0][0] > basis[2][2] {
        let s = (1.0 + basis[0][0] - basis[1][1] - basis[2][2]).sqrt() * 2.0;
        return [
            0.25 * s,
            (basis[1][0] + basis[0][1]) / s,
            (basis[2][0] + basis[0][2]) / s,
            (basis[1][2] - basis[2][1]) / s,
        ];
    }
    if basis[1][1] > basis[2][2] {
        let s = (1.0 + basis[1][1] - basis[0][0] - basis[2][2]).sqrt() * 2.0;
        return [
            (basis[1][0] + basis[0][1]) / s,
            0.25 * s,
            (basis[2][1] + basis[1][2]) / s,
            (basis[2][0] - basis[0][2]) / s,
        ];
    }
    let s = (1.0 + basis[2][2] - basis[0][0] - basis[1][1]).sqrt() * 2.0;
    [
        (basis[2][0] + basis[0][2]) / s,
        (basis[2][1] + basis[1][2]) / s,
        0.25 * s,
        (basis[0][1] - basis[1][0]) / s,
    ]
}

/// The Euclidean length of a vector.
fn length(vector: [f32; 3]) -> f32 {
    dot(vector, vector).sqrt()
}

/// The dot product of two vectors.
fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

/// The cross product of two vectors.
fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

/// `vector + factor * other`.
fn combine(vector: [f32; 3], other: [f32; 3], factor: f32) -> [f32; 3] {
    [
        vector[0] + other[0] * factor,
        vector[1] + other[1] * factor,
        vector[2] + other[2] * factor,
    ]
}

/// The vector divided by `length`, or `None` when that length is zero.
fn unit_vector(vector: [f32; 3], length: f32) -> Option<[f32; 3]> {
    if length == 0.0 || !length.is_finite() {
        return None;
    }
    let inverse = length.recip();
    Some([
        vector[0] * inverse,
        vector[1] * inverse,
        vector[2] * inverse,
    ])
}

#[cfg(test)]
mod tests {
    use core::f32::consts::FRAC_PI_3;

    use proptest::prelude::*;

    use super::Decomposed;
    use crate::transform::Matrix4;

    /// The largest entrywise difference between two matrices.
    fn difference(left: &Matrix4, right: &Matrix4) -> f32 {
        let mut worst = 0.0_f32;
        for column in 0..4 {
            for row in 0..4 {
                worst = worst.max((left.get(row, column) - right.get(row, column)).abs());
            }
        }
        worst
    }

    /// The largest absolute entry of a matrix, which is the scale errors are relative to.
    fn largest_entry(matrix: &Matrix4) -> f32 {
        matrix
            .columns
            .iter()
            .flatten()
            .fold(0.0_f32, |worst, entry| worst.max(entry.abs()))
    }

    /// A rotation of `radians` about a unit axis, built the long way round.
    fn rotation(axis: [f32; 3], radians: f32) -> Matrix4 {
        let half = radians * 0.5;
        let sin = half.sin();
        super::rotation_matrix([axis[0] * sin, axis[1] * sin, axis[2] * sin, half.cos()])
    }

    #[test]
    fn the_identity_decomposes_to_nothing() {
        let parts = Matrix4::IDENTITY.decompose().expect("decomposable");
        assert_eq!(parts, Decomposed::IDENTITY);
    }

    #[test]
    fn a_singular_matrix_cannot_be_decomposed() {
        assert_eq!(Matrix4::scale(1.0, 0.0, 1.0).decompose(), None);
    }

    #[test]
    fn a_reflection_shows_up_as_a_negative_scale() {
        let parts = Matrix4::scale(-1.0, 1.0, 1.0)
            .decompose()
            .expect("decomposable");
        assert!(parts.scale.iter().filter(|factor| **factor < 0.0).count() % 2 == 1);
        assert!(difference(&parts.recompose(), &Matrix4::scale(-1.0, 1.0, 1.0)) < 1e-5);
    }

    #[test]
    fn a_rotation_survives_the_round_trip() {
        let matrix = rotation([0.0, 0.0, 1.0], FRAC_PI_3);
        let round_tripped = matrix.decompose().expect("decomposable").recompose();
        assert!(difference(&matrix, &round_tripped) < 1e-5);
    }

    #[test]
    fn perspective_survives_the_round_trip() {
        let matrix = Matrix4::translation(30.0, 10.0, -20.0).then(&Matrix4::perspective(400.0));
        let round_tripped = matrix.decompose().expect("decomposable").recompose();
        let expected = matrix.normalized().expect("normalisable");
        assert!(
            difference(&expected, &round_tripped) < 1e-4,
            "{round_tripped:?}"
        );
    }

    proptest! {
        /// Decomposing and recomposing reproduces the matrix to within 1e-5.
        #[test]
        fn decompose_and_recompose_agree(
            translation in prop::array::uniform3(-500.0_f32..500.0),
            scale in prop::array::uniform3(0.25_f32..4.0),
            angle in -3.0_f32..3.0,
            axis in prop::array::uniform3(-1.0_f32..1.0),
            depth in 200.0_f32..2000.0,
            skew_angle in -0.8_f32..0.8,
        ) {
            let axis_length = (axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2]).sqrt();
            prop_assume!(axis_length > 0.1);
            let unit = [
                axis[0] / axis_length,
                axis[1] / axis_length,
                axis[2] / axis_length,
            ];

            let mut shear = Matrix4::IDENTITY;
            shear.columns[1][0] = skew_angle.tan();

            let matrix = Matrix4::scale(scale[0], scale[1], scale[2])
                .then(&shear)
                .then(&rotation(unit, angle))
                .then(&Matrix4::translation(translation[0], translation[1], translation[2]))
                .then(&Matrix4::perspective(depth));

            let expected = matrix.normalized().expect("normalisable");
            // A matrix normalised by a factor near zero is numerically meaningless; that is the
            // viewer sitting on the plane the content was pushed onto, not a decomposition bug.
            prop_assume!(matrix.get(3, 3).abs() > 0.25);

            let parts = matrix.decompose().expect("decomposable");
            let round_tripped = parts.recompose();
            let worst = difference(&expected, &round_tripped);
            let magnitude = largest_entry(&expected).max(1.0);
            prop_assert!(worst <= 1e-5 * magnitude, "differs by {worst} on {expected:?}");
        }
    }
}
