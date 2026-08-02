//! The 4x4 transform matrix CSS 3D transforms produce.

use core::ops::Mul;

use crate::transform::Decomposed;

/// A 4x4 transform matrix in column-major order.
///
/// `columns[c][r]` is the entry in column `c` and row `r`, which is the layout a shader receives a
/// `mat4x4<f32>` in, so the matrix can be copied into a buffer without rearranging anything. The
/// convention is that a column vector is multiplied on the right — `v' = M * v` — so the
/// translation lives in the last column and the perspective terms in the last row.
///
/// ```
/// use zgui_geom::Matrix4;
///
/// let matrix = Matrix4::translation(1.0, 2.0, 3.0);
/// assert_eq!(matrix.columns[3], [1.0, 2.0, 3.0, 1.0]);
/// assert_eq!(matrix.transform_point(0.0, 0.0, 0.0), [1.0, 2.0, 3.0]);
/// ```
///
/// Animating between two of these means taking them apart first; see [`Decomposed`].
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix4 {
    /// The four columns, each holding four rows.
    pub columns: [[f32; 4]; 4],
}

impl Matrix4 {
    /// The transform that changes nothing.
    pub const IDENTITY: Self = Self::from_columns([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    /// A matrix from its columns.
    pub const fn from_columns(columns: [[f32; 4]; 4]) -> Self {
        Self { columns }
    }

    /// A matrix from its rows, which is the order a matrix is normally written on paper.
    pub const fn from_rows(rows: [[f32; 4]; 4]) -> Self {
        Self::from_columns([
            [rows[0][0], rows[1][0], rows[2][0], rows[3][0]],
            [rows[0][1], rows[1][1], rows[2][1], rows[3][1]],
            [rows[0][2], rows[1][2], rows[2][2], rows[3][2]],
            [rows[0][3], rows[1][3], rows[2][3], rows[3][3]],
        ])
    }

    /// A pure translation.
    pub const fn translation(x: f32, y: f32, z: f32) -> Self {
        let mut matrix = Self::IDENTITY;
        matrix.columns[3] = [x, y, z, 1.0];
        matrix
    }

    /// A pure scale about the origin.
    pub const fn scale(x: f32, y: f32, z: f32) -> Self {
        Self::from_columns([
            [x, 0.0, 0.0, 0.0],
            [0.0, y, 0.0, 0.0],
            [0.0, 0.0, z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// The perspective matrix for a viewer `depth` units in front of the z = 0 plane.
    ///
    /// This is CSS `perspective(depth)`. A depth of zero or less produces the identity, matching
    /// the rule that a non-positive perspective disables the effect.
    pub fn perspective(depth: f32) -> Self {
        let mut matrix = Self::IDENTITY;
        if depth > 0.0 {
            matrix.columns[2][3] = -depth.recip();
        }
        matrix
    }

    /// The same transform with its homogeneous scale divided out, so that `get(3, 3)` is one.
    ///
    /// Two matrices that differ only by an overall factor describe the same transform, because the
    /// factor cancels in the perspective divide. Normalising picks the one representative of that
    /// family that can be compared entry by entry. Returns `None` when `get(3, 3)` is zero, which
    /// is a matrix with no normalised form.
    pub fn normalized(&self) -> Option<Self> {
        let homogeneous = self.get(3, 3);
        if homogeneous == 0.0 || !homogeneous.is_finite() {
            return None;
        }
        let inverse = homogeneous.recip();
        let mut result = *self;
        for column in &mut result.columns {
            for entry in column.iter_mut() {
                *entry *= inverse;
            }
        }
        Some(result)
    }

    /// The entry in row `row`, column `column`.
    pub const fn get(&self, row: usize, column: usize) -> f32 {
        self.columns[column][row]
    }

    /// The matrix with rows and columns exchanged.
    pub fn transpose(&self) -> Self {
        let mut result = *self;
        for row in 0..4 {
            for column in 0..4 {
                result.columns[column][row] = self.columns[row][column];
            }
        }
        result
    }

    /// This transform followed by `next`, which is the matrix product `next * self`.
    pub fn then(&self, next: &Self) -> Self {
        let mut result = Self::from_columns([[0.0; 4]; 4]);
        for column in 0..4 {
            for row in 0..4 {
                let mut sum = 0.0;
                for index in 0..4 {
                    sum += next.columns[index][row] * self.columns[column][index];
                }
                result.columns[column][row] = sum;
            }
        }
        result
    }

    /// Applies the transform to a point, dividing through by the resulting `w`.
    ///
    /// A `w` of zero means the point is on the horizon of the projection and has no finite image;
    /// the coordinates are returned undivided in that case rather than as infinities.
    pub fn transform_point(&self, x: f32, y: f32, z: f32) -> [f32; 3] {
        let [out_x, out_y, out_z, w] = self.transform_vector4([x, y, z, 1.0]);
        if w == 0.0 || w == 1.0 {
            [out_x, out_y, out_z]
        } else {
            let inverse = w.recip();
            [out_x * inverse, out_y * inverse, out_z * inverse]
        }
    }

    /// Applies the transform to a homogeneous vector, without any division.
    pub fn transform_vector4(&self, vector: [f32; 4]) -> [f32; 4] {
        let mut result = [0.0; 4];
        for (row, entry) in result.iter_mut().enumerate() {
            *entry = self
                .columns
                .iter()
                .zip(vector.iter())
                .map(|(column, component)| column[row] * component)
                .sum();
        }
        result
    }

    /// Whether the matrix only moves things within the z = 0 plane.
    ///
    /// A transform that passes this can be flattened to an [`Affine2`](crate::Affine2) and skip
    /// the machinery a 3D transform needs.
    pub fn is_2d(&self) -> bool {
        let m = &self.columns;
        m[0][2] == 0.0
            && m[0][3] == 0.0
            && m[1][2] == 0.0
            && m[1][3] == 0.0
            && m[2] == [0.0, 0.0, 1.0, 0.0]
            && m[3][2] == 0.0
            && m[3][3] == 1.0
    }

    /// The two-dimensional transform this embeds, or nothing when it moves out of the z = 0 plane.
    ///
    /// ```
    /// use zgui_geom::{Affine2, Matrix4};
    ///
    /// let turned = Affine2::rotation(0.4);
    /// assert_eq!(turned.to_matrix4().to_affine2(), Some(turned));
    /// assert_eq!(Matrix4::perspective(400.0).to_affine2(), None);
    /// ```
    pub fn to_affine2(&self) -> Option<crate::Affine2> {
        let m = &self.columns;
        self.is_2d()
            .then(|| crate::Affine2::new(m[0][0], m[0][1], m[1][0], m[1][1], m[3][0], m[3][1]))
    }

    /// The determinant.
    pub fn determinant(&self) -> f32 {
        (0..4)
            .map(|column| self.get(0, column) * self.cofactor(0, column))
            .sum()
    }

    /// The signed cofactor of the entry at `row`, `column`.
    fn cofactor(&self, row: usize, column: usize) -> f32 {
        let mut minor = [[0.0_f32; 3]; 3];
        let mut target_row = 0;
        for source_row in 0..4 {
            if source_row == row {
                continue;
            }
            let mut target_column = 0;
            for source_column in 0..4 {
                if source_column == column {
                    continue;
                }
                minor[target_row][target_column] = self.get(source_row, source_column);
                target_column += 1;
            }
            target_row += 1;
        }
        let determinant = minor[0][0] * (minor[1][1] * minor[2][2] - minor[1][2] * minor[2][1])
            - minor[0][1] * (minor[1][0] * minor[2][2] - minor[1][2] * minor[2][0])
            + minor[0][2] * (minor[1][0] * minor[2][1] - minor[1][1] * minor[2][0]);
        if (row + column).is_multiple_of(2) {
            determinant
        } else {
            -determinant
        }
    }

    /// The matrix that undoes this one, or `None` when it is singular.
    pub fn invert(&self) -> Option<Self> {
        let determinant = self.determinant();
        if determinant == 0.0 || !determinant.is_finite() {
            return None;
        }
        let inverse = determinant.recip();
        let mut result = Self::from_columns([[0.0; 4]; 4]);
        for row in 0..4 {
            for column in 0..4 {
                // The adjugate is the transpose of the cofactor matrix.
                result.columns[column][row] = self.cofactor(column, row) * inverse;
            }
        }
        Some(result)
    }

    /// Splits the matrix into the parts an animation interpolates.
    ///
    /// Returns `None` for a matrix that cannot be decomposed, which means one that collapses
    /// space: a zero scale on some axis, or a degenerate perspective.
    pub fn decompose(&self) -> Option<Decomposed> {
        Decomposed::of(self)
    }
}

impl Default for Matrix4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for Matrix4 {
    type Output = Self;

    /// The matrix product, so `self` is applied *after* `other`.
    ///
    /// [`Matrix4::then`] reads in the order things happen and is usually clearer.
    fn mul(self, other: Self) -> Self {
        other.then(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::Matrix4;

    fn close(left: [f32; 3], right: [f32; 3]) -> bool {
        left.iter()
            .zip(right.iter())
            .all(|(a, b)| (a - b).abs() < 1e-5)
    }

    #[test]
    fn rows_and_columns_are_two_views_of_one_matrix() {
        let matrix = Matrix4::from_rows([
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ]);
        assert_eq!(matrix.get(0, 3), 4.0);
        assert_eq!(matrix.columns[3], [4.0, 8.0, 12.0, 16.0]);
        assert_eq!(matrix.transpose().get(3, 0), 4.0);
    }

    #[test]
    fn composition_applies_the_left_matrix_first() {
        let compound = Matrix4::translation(1.0, 0.0, 0.0).then(&Matrix4::scale(10.0, 1.0, 1.0));
        assert!(close(
            compound.transform_point(0.0, 0.0, 0.0),
            [10.0, 0.0, 0.0]
        ));
    }

    #[test]
    fn the_identity_is_two_dimensional_and_perspective_is_not() {
        assert!(Matrix4::IDENTITY.is_2d());
        assert!(Matrix4::translation(1.0, 2.0, 0.0).is_2d());
        assert!(!Matrix4::translation(0.0, 0.0, 1.0).is_2d());
        assert!(!Matrix4::perspective(500.0).is_2d());
    }

    #[test]
    fn a_matrix_and_its_inverse_cancel() {
        let matrix = Matrix4::translation(3.0, -4.0, 5.0)
            .then(&Matrix4::scale(2.0, 4.0, 8.0))
            .then(&Matrix4::perspective(400.0));
        let inverse = matrix.invert().expect("invertible");
        let identity = matrix.then(&inverse);
        for column in 0..4 {
            for row in 0..4 {
                let expected = Matrix4::IDENTITY.get(row, column);
                let found = identity.get(row, column);
                assert!((found - expected).abs() < 1e-5, "{identity:?}");
            }
        }
    }

    #[test]
    fn a_singular_matrix_has_no_inverse() {
        assert_eq!(Matrix4::scale(1.0, 0.0, 1.0).invert(), None);
    }

    #[test]
    fn a_non_positive_perspective_does_nothing() {
        assert_eq!(Matrix4::perspective(0.0), Matrix4::IDENTITY);
        assert_eq!(Matrix4::perspective(-10.0), Matrix4::IDENTITY);
    }
}
