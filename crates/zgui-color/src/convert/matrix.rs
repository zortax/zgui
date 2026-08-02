//! Three-by-three matrices, in row-major order, applied to a colour's three channels.

/// A row-major 3×3 matrix.
pub(crate) type Matrix3 = [[f32; 3]; 3];

/// Applies `matrix` to `vector`.
pub(crate) fn apply(matrix: &Matrix3, vector: [f32; 3]) -> [f32; 3] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{Matrix3, apply};

    /// The product of two matrices, for checking that a pair of them are inverses.
    pub(crate) fn multiply(left: &Matrix3, right: &Matrix3) -> Matrix3 {
        let mut out = [[0.0; 3]; 3];
        for (row, entry) in out.iter_mut().enumerate() {
            for (column, cell) in entry.iter_mut().enumerate() {
                *cell = (0..3).map(|k| left[row][k] * right[k][column]).sum();
            }
        }
        out
    }

    /// Asserts that two matrices are inverses of one another.
    pub(crate) fn assert_inverse(forward: &Matrix3, backward: &Matrix3, tolerance: f32) {
        let product = multiply(forward, backward);
        for (row, entry) in product.iter().enumerate() {
            for (column, cell) in entry.iter().enumerate() {
                let expected = if row == column { 1.0 } else { 0.0 };
                assert!(
                    (cell - expected).abs() < tolerance,
                    "row {row} column {column} is {cell}, expected {expected}",
                );
            }
        }
    }

    #[test]
    fn identity_leaves_a_vector_alone() {
        let identity: Matrix3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert_eq!(apply(&identity, [1.0, 2.0, 3.0]), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn rows_are_dotted_with_the_vector() {
        let matrix: Matrix3 = [[1.0, 2.0, 3.0], [0.0, 1.0, 0.0], [0.0, 0.0, 2.0]];
        assert_eq!(apply(&matrix, [1.0, 1.0, 1.0]), [6.0, 1.0, 2.0]);
    }
}
