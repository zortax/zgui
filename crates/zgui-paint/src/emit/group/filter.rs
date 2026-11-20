//! CSS filter functions as compositor filters.
//!
//! The per-pixel functions are colour matrices over straight colour. A run of them is the product
//! of their matrices, so it becomes one matrix.

use compositor_types::{Color, Filter};
use zgui_scene::peniko::{BlendMode, Compose};

/// A 4x5 colour matrix in row-major order, the constant last in each row.
type Matrix = [f32; 20];

const IDENTITY: Matrix = [
    1.0, 0.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, 0.0,
];

/// The luminance weights CSS's filter matrices use.
const LUMA: [f32; 3] = [0.213, 0.715, 0.072];

/// Appends the compositor chain for `filters` to `out`, and returns how many filters it has no
/// counterpart for.
///
/// A filter that changes nothing adds no step.
pub(crate) fn convert(filters: &[zgui_scene::Filter], out: &mut Vec<Filter>) -> usize {
    let mut omitted = 0;
    let mut run: Option<Matrix> = None;
    for filter in filters {
        if let Some(matrix) = matrix(*filter) {
            run = Some(run.map_or(matrix, |before| then(before, matrix)));
            continue;
        }
        flush(&mut run, out);
        match *filter {
            zgui_scene::Filter::Blur(deviation) if deviation > 0.0 => {
                out.push(Filter::Blur(deviation));
            }
            zgui_scene::Filter::DropShadow {
                offset_x,
                offset_y,
                blur,
                color,
            } => out.push(Filter::DropShadow {
                offset: [offset_x, offset_y],
                std_dev: blur,
                color: Color(color),
            }),
            // Application shaders have no compositor shader id yet.
            zgui_scene::Filter::Custom { .. } => omitted += 1,
            _ => {}
        }
    }
    flush(&mut run, out);
    omitted
}

/// Returns the compositor's mode for `blend`.
pub(crate) fn blend(blend: BlendMode) -> compositor_types::BlendMode {
    if blend.compose == Compose::Plus {
        return compositor_types::BlendMode::PLUS_LIGHTER;
    }
    compositor_types::BlendMode(blend.mix as u8)
}

/// Pushes the run of matrices so far, unless it changes nothing.
fn flush(run: &mut Option<Matrix>, out: &mut Vec<Filter>) {
    if let Some(matrix) = run.take()
        && matrix != IDENTITY
    {
        out.push(Filter::ColorMatrix(matrix));
    }
}

/// Returns the matrix of a per-pixel filter function, or `None` for one that reads its
/// neighbourhood.
fn matrix(filter: zgui_scene::Filter) -> Option<Matrix> {
    use zgui_scene::Filter as F;
    Some(match filter {
        F::Brightness(amount) => scale(amount.max(0.0), 0.0),
        F::Contrast(amount) => {
            let amount = amount.max(0.0);
            scale(amount, 0.5 - 0.5 * amount)
        }
        F::Grayscale(amount) => saturate(1.0 - amount.clamp(0.0, 1.0)),
        F::HueRotate(radians) => hue_rotate(radians),
        F::Invert(amount) => {
            let amount = amount.clamp(0.0, 1.0);
            scale(1.0 - 2.0 * amount, amount)
        }
        F::Opacity(amount) => {
            let mut matrix = IDENTITY;
            matrix[18] = amount.clamp(0.0, 1.0);
            matrix
        }
        F::Saturate(amount) => saturate(amount.max(0.0)),
        F::Sepia(amount) => sepia(amount.clamp(0.0, 1.0)),
        F::Blur(_) | F::DropShadow { .. } | F::Custom { .. } => return None,
    })
}

/// Returns the matrix that scales the colour channels by `factor` and adds `offset` to them.
fn scale(factor: f32, offset: f32) -> Matrix {
    let mut matrix = IDENTITY;
    for channel in 0..3 {
        matrix[channel * 5 + channel] = factor;
        matrix[channel * 5 + 4] = offset;
    }
    matrix
}

fn saturate(s: f32) -> Matrix {
    let [r, g, b] = LUMA;
    [
        r + (1.0 - r) * s,
        g - g * s,
        b - b * s,
        0.0,
        0.0,
        r - r * s,
        g + (1.0 - g) * s,
        b - b * s,
        0.0,
        0.0,
        r - r * s,
        g - g * s,
        b + (1.0 - b) * s,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
    ]
}

fn hue_rotate(radians: f32) -> Matrix {
    let (sin, cos) = radians.sin_cos();
    [
        0.213 + cos * 0.787 - sin * 0.213,
        0.715 - cos * 0.715 - sin * 0.715,
        0.072 - cos * 0.072 + sin * 0.928,
        0.0,
        0.0,
        0.213 - cos * 0.213 + sin * 0.143,
        0.715 + cos * 0.285 + sin * 0.140,
        0.072 - cos * 0.072 - sin * 0.283,
        0.0,
        0.0,
        0.213 - cos * 0.213 - sin * 0.787,
        0.715 - cos * 0.715 + sin * 0.715,
        0.072 + cos * 0.928 + sin * 0.072,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
    ]
}

fn sepia(amount: f32) -> Matrix {
    let rest = 1.0 - amount;
    [
        0.393 + 0.607 * rest,
        0.769 - 0.769 * rest,
        0.189 - 0.189 * rest,
        0.0,
        0.0,
        0.349 - 0.349 * rest,
        0.686 + 0.314 * rest,
        0.168 - 0.168 * rest,
        0.0,
        0.0,
        0.272 - 0.272 * rest,
        0.534 - 0.534 * rest,
        0.131 + 0.869 * rest,
        0.0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
    ]
}

/// Returns the matrix that applies `first` and then `next`.
fn then(first: Matrix, next: Matrix) -> Matrix {
    let mut out = [0.0; 20];
    for row in 0..4 {
        for column in 0..5 {
            let mut value: f32 = (0..4)
                .map(|middle| next[row * 5 + middle] * first[middle * 5 + column])
                .sum();
            if column == 4 {
                value += next[row * 5 + 4];
            }
            out[row * 5 + column] = value;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{IDENTITY, Matrix, convert, then};
    use compositor_types::Filter;

    /// Applies a row-major matrix to a straight colour.
    fn apply(matrix: &Matrix, color: [f32; 4]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for (row, value) in out.iter_mut().enumerate() {
            *value = (0..4)
                .map(|at| matrix[row * 5 + at] * color[at])
                .sum::<f32>()
                + matrix[row * 5 + 4];
        }
        out
    }

    #[test]
    fn a_run_of_functions_is_one_matrix_applied_in_order() {
        let mut out = Vec::new();
        convert(
            &[
                zgui_scene::Filter::Invert(1.0),
                zgui_scene::Filter::Brightness(0.5),
            ],
            &mut out,
        );
        let [Filter::ColorMatrix(matrix)] = out[..] else {
            panic!("one matrix: {out:?}");
        };
        // Inverting 0.2 gives 0.8, and halving that gives 0.4.
        let [red, ..] = apply(&matrix, [0.2, 0.2, 0.2, 1.0]);
        assert!((red - 0.4).abs() < 1e-6, "{red}");
    }

    #[test]
    fn a_blur_keeps_the_runs_on_either_side_apart() {
        let mut out = Vec::new();
        convert(
            &[
                zgui_scene::Filter::Sepia(1.0),
                zgui_scene::Filter::Blur(2.0),
                zgui_scene::Filter::Opacity(1.0),
                zgui_scene::Filter::Grayscale(1.0),
            ],
            &mut out,
        );
        assert!(matches!(
            out[..],
            [
                Filter::ColorMatrix(_),
                Filter::Blur(2.0),
                Filter::ColorMatrix(_)
            ]
        ));
    }

    #[test]
    fn the_identity_is_the_unit_of_the_product() {
        let mut out = Vec::new();
        convert(&[zgui_scene::Filter::Brightness(1.0)], &mut out);
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(then(IDENTITY, IDENTITY), IDENTITY);
    }
}
