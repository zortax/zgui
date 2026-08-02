//! The per-pixel CSS filter functions, as one matrix.

use zgui_scene::Filter;

/// A CSS filter function's effect on one colour, as an affine map.
///
/// Every per-pixel filter — `brightness`, `contrast`, `grayscale`, `hue-rotate`, `invert`,
/// `opacity`, `saturate`, `sepia` — is defined by the specification as a colour matrix, and a run
/// of them is the product of theirs. Folding a chain into one matrix is therefore exact rather
/// than an approximation, and it is what keeps a stack of five filters at one pass over the
/// content instead of five.
///
/// The maps act on colour that is **not** scaled by its alpha, in the same gamma-encoded space
/// everything else in a frame is composited in. That is what the specification says: the filter
/// functions operate on sRGB values, and `color-interpolation-filters` — which selects linear
/// light — applies to SVG filter *primitives* rather than to these.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorMatrix {
    /// The linear part, row by row: `rows[i]` produces output channel `i`.
    rows: [[f32; 4]; 4],
    /// The constant term.
    offset: [f32; 4],
}

impl ColorMatrix {
    /// The map that changes nothing.
    pub fn identity() -> Self {
        Self {
            rows: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            offset: [0.0; 4],
        }
    }

    /// Whether this map changes nothing.
    pub fn is_identity(&self) -> bool {
        *self == Self::identity()
    }

    /// The map for one filter function, or `None` for one that is not per-pixel.
    ///
    /// A blur and a drop shadow read a neighbourhood rather than a pixel, so neither has a matrix
    /// and both are executed as passes of their own.
    pub fn of(filter: Filter) -> Option<Self> {
        match filter {
            Filter::Blur(_) | Filter::DropShadow { .. } => None,
            Filter::Brightness(amount) => Some(Self::brightness(amount)),
            Filter::Contrast(amount) => Some(Self::contrast(amount)),
            Filter::Grayscale(amount) => Some(Self::saturate(1.0 - amount.clamp(0.0, 1.0))),
            Filter::HueRotate(radians) => Some(Self::hue_rotate(radians)),
            Filter::Invert(amount) => Some(Self::invert(amount)),
            Filter::Opacity(amount) => Some(Self::opacity(amount)),
            Filter::Saturate(amount) => Some(Self::saturate(amount)),
            Filter::Sepia(amount) => Some(Self::sepia(amount)),
        }
    }

    /// Scales every channel's intensity.
    pub fn scale_rgb(factor: f32) -> Self {
        let mut matrix = Self::identity();
        for channel in 0..3 {
            matrix.rows[channel][channel] = factor;
        }
        matrix
    }

    /// Scales luminance.
    pub fn brightness(amount: f32) -> Self {
        Self::scale_rgb(amount.max(0.0))
    }

    /// Scales each channel's distance from mid grey.
    pub fn contrast(amount: f32) -> Self {
        let amount = amount.max(0.0);
        let mut matrix = Self::scale_rgb(amount);
        let shift = 0.5 - 0.5 * amount;
        matrix.offset = [shift, shift, shift, 0.0];
        matrix
    }

    /// Interpolates each channel towards its complement.
    pub fn invert(amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let mut matrix = Self::scale_rgb(1.0 - 2.0 * amount);
        matrix.offset = [amount, amount, amount, 0.0];
        matrix
    }

    /// Scales alpha.
    pub fn opacity(amount: f32) -> Self {
        let mut matrix = Self::identity();
        matrix.rows[3][3] = amount.clamp(0.0, 1.0);
        matrix
    }

    /// Scales each channel's distance from the colour's own luminance.
    ///
    /// At zero this is `grayscale(1)` and at one it changes nothing, which is exactly why
    /// `grayscale` is expressed through it rather than given a second definition to keep in step.
    pub fn saturate(amount: f32) -> Self {
        let s = amount.max(0.0);
        let (r, g, b) = (0.213, 0.715, 0.072);
        Self {
            rows: [
                [r + (1.0 - r) * s, g - g * s, b - b * s, 0.0],
                [r - r * s, g + (1.0 - g) * s, b - b * s, 0.0],
                [r - r * s, g - g * s, b + (1.0 - b) * s, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            offset: [0.0; 4],
        }
    }

    /// Rotates hue by `radians`.
    pub fn hue_rotate(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            rows: [
                [
                    0.213 + cos * 0.787 - sin * 0.213,
                    0.715 - cos * 0.715 - sin * 0.715,
                    0.072 - cos * 0.072 + sin * 0.928,
                    0.0,
                ],
                [
                    0.213 - cos * 0.213 + sin * 0.143,
                    0.715 + cos * 0.285 + sin * 0.140,
                    0.072 - cos * 0.072 - sin * 0.283,
                    0.0,
                ],
                [
                    0.213 - cos * 0.213 - sin * 0.787,
                    0.715 - cos * 0.715 + sin * 0.715,
                    0.072 + cos * 0.928 + sin * 0.072,
                    0.0,
                ],
                [0.0, 0.0, 0.0, 1.0],
            ],
            offset: [0.0; 4],
        }
    }

    /// Interpolates towards a sepia-toned copy.
    pub fn sepia(amount: f32) -> Self {
        let a = amount.clamp(0.0, 1.0);
        Self {
            rows: [
                [
                    0.393 + 0.607 * (1.0 - a),
                    0.769 - 0.769 * (1.0 - a),
                    0.189 - 0.189 * (1.0 - a),
                    0.0,
                ],
                [
                    0.349 - 0.349 * (1.0 - a),
                    0.686 + 0.314 * (1.0 - a),
                    0.168 - 0.168 * (1.0 - a),
                    0.0,
                ],
                [
                    0.272 - 0.272 * (1.0 - a),
                    0.534 - 0.534 * (1.0 - a),
                    0.131 + 0.869 * (1.0 - a),
                    0.0,
                ],
                [0.0, 0.0, 0.0, 1.0],
            ],
            offset: [0.0; 4],
        }
    }

    /// This map followed by `next`, which is what a chain of filter functions is.
    pub fn then(self, next: Self) -> Self {
        let mut rows = [[0.0f32; 4]; 4];
        for (out, row) in rows.iter_mut().enumerate() {
            for (input, cell) in row.iter_mut().enumerate() {
                *cell = (0..4)
                    .map(|middle| next.rows[out][middle] * self.rows[middle][input])
                    .sum();
            }
        }
        let mut offset = next.offset;
        for (out, value) in offset.iter_mut().enumerate() {
            *value += (0..4)
                .map(|middle| next.rows[out][middle] * self.offset[middle])
                .sum::<f32>();
        }
        Self { rows, offset }
    }

    /// Applies the map to one unpremultiplied colour, clamped as the shader clamps it.
    pub fn apply(&self, color: [f32; 4]) -> [f32; 4] {
        let mut out = self.offset;
        for (channel, value) in out.iter_mut().enumerate() {
            *value += (0..4)
                .map(|input| self.rows[channel][input] * color[input])
                .sum::<f32>();
            *value = value.clamp(0.0, 1.0);
        }
        out
    }

    /// The map as the shader reads it: four columns, then the constant term.
    ///
    /// Columns rather than rows because a shader multiplies by scaling each column by one input
    /// channel and adding them, which is one multiply-add per channel and no transpose.
    pub fn columns(&self) -> [[f32; 4]; 5] {
        let mut columns = [[0.0f32; 4]; 5];
        for (input, column) in columns.iter_mut().take(4).enumerate() {
            for (output, cell) in column.iter_mut().enumerate() {
                *cell = self.rows[output][input];
            }
        }
        columns[4] = self.offset;
        columns
    }
}

#[cfg(test)]
mod tests {
    use super::ColorMatrix;
    use zgui_scene::Filter;

    /// Asserts two colours agree to within a level at eight bits.
    fn close(left: [f32; 4], right: [f32; 4]) {
        for channel in 0..4 {
            assert!(
                (left[channel] - right[channel]).abs() < 1.0 / 255.0,
                "{left:?} against {right:?}"
            );
        }
    }

    #[test]
    fn a_neighbourhood_filter_has_no_matrix_and_every_other_one_does() {
        assert!(ColorMatrix::of(Filter::Blur(4.0)).is_none());
        assert!(
            ColorMatrix::of(Filter::DropShadow {
                offset_x: 1.0,
                offset_y: 1.0,
                blur: 2.0,
                color: [0.0; 4],
            })
            .is_none()
        );
        for filter in [
            Filter::Brightness(0.5),
            Filter::Contrast(2.0),
            Filter::Grayscale(1.0),
            Filter::HueRotate(1.0),
            Filter::Invert(1.0),
            Filter::Opacity(0.5),
            Filter::Saturate(0.0),
            Filter::Sepia(1.0),
        ] {
            assert!(filter.is_per_pixel());
            assert!(ColorMatrix::of(filter).is_some(), "{filter:?}");
        }
    }

    #[test]
    fn the_specified_extremes_land_where_the_specification_says() {
        let red = [1.0, 0.0, 0.0, 1.0];
        close(ColorMatrix::invert(1.0).apply(red), [0.0, 1.0, 1.0, 1.0]);
        close(
            ColorMatrix::brightness(0.0).apply(red),
            [0.0, 0.0, 0.0, 1.0],
        );
        close(ColorMatrix::opacity(0.25).apply(red), [1.0, 0.0, 0.0, 0.25]);
        // grayscale(1) is saturate(0), and the luminance of pure red is its own coefficient.
        close(
            ColorMatrix::of(Filter::Grayscale(1.0)).unwrap().apply(red),
            [0.213, 0.213, 0.213, 1.0],
        );
        // A full rotation is the identity, up to the accumulated rounding of eight coefficients.
        close(
            ColorMatrix::hue_rotate(core::f32::consts::TAU).apply(red),
            red,
        );
    }

    #[test]
    fn composing_two_maps_is_applying_them_in_turn() {
        let first = ColorMatrix::saturate(0.3);
        let second = ColorMatrix::contrast(1.7);
        let color = [0.2, 0.6, 0.9, 0.8];
        close(
            first.then(second).apply(color),
            second.apply(first.apply(color)),
        );
    }

    #[test]
    fn composition_is_ordered_and_the_test_can_tell() {
        // Contrast has a constant term and brightness does not, so the two do not commute — which
        // is the whole reason a chain is folded in order rather than as a set.
        let first = ColorMatrix::contrast(2.0);
        let second = ColorMatrix::brightness(0.5);
        let color = [0.6, 0.3, 0.1, 1.0];
        assert_ne!(
            first.then(second).apply(color),
            second.then(first).apply(color)
        );
    }

    #[test]
    fn the_identity_composes_with_anything_without_changing_it() {
        let sepia = ColorMatrix::sepia(1.0);
        assert!(ColorMatrix::identity().is_identity());
        assert_eq!(sepia.then(ColorMatrix::identity()), sepia);
        assert_eq!(ColorMatrix::identity().then(sepia), sepia);
    }

    #[test]
    fn the_columns_the_shader_reads_are_the_transpose_of_the_rows() {
        let matrix = ColorMatrix::sepia(1.0);
        let columns = matrix.columns();
        let color = [0.4, 0.7, 0.2, 1.0];
        let mut by_column = columns[4];
        for (input, column) in columns.iter().take(4).enumerate() {
            for (output, cell) in column.iter().enumerate() {
                by_column[output] += cell * color[input];
            }
        }
        close(
            [
                by_column[0].clamp(0.0, 1.0),
                by_column[1].clamp(0.0, 1.0),
                by_column[2].clamp(0.0, 1.0),
                by_column[3].clamp(0.0, 1.0),
            ],
            matrix.apply(color),
        );
    }
}
