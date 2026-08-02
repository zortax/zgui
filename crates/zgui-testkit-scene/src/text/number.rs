//! Rendering a number the same way every time, on every machine.
//!
//! Rust's default float formatting is already deterministic, but it is not *stable* in the sense a
//! golden needs: `0.1 + 0.2` prints seventeen significant digits, a negative zero prints with its
//! sign, and a value that arrived from a different but equivalent computation prints differently
//! from one that did not. Rounding to a fixed number of decimals and dropping the trailing zeros
//! gives a rendering in which two geometrically identical frames are textually identical.

/// How many decimals a coordinate keeps.
///
/// Four is a quarter of a thousandth of a device pixel, which is far below anything that could
/// change a rendered image and far above the noise two routes to the same coordinate accumulate.
pub const DECIMALS: usize = 4;

/// One number, rendered the way every transcript and every tree dump renders it.
///
/// ```
/// use zgui_testkit_scene::text::number::float;
///
/// assert_eq!(float(1.0), "1");
/// assert_eq!(float(-0.0), "0");
/// assert_eq!(float(0.1 + 0.2), "0.3");
/// assert_eq!(float(f32::NAN), "nan");
/// ```
pub fn float(value: f32) -> String {
    if value.is_nan() {
        return "nan".to_owned();
    }
    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-inf".to_owned()
        } else {
            "inf".to_owned()
        };
    }
    // A negative zero is arithmetically the same number as a positive one and must print the same,
    // or a rectangle that reached the origin by subtraction diffs against one that started there.
    let value = if value == 0.0 { 0.0 } else { value };
    let mut text = format!("{value:.DECIMALS$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    if text == "-0" { "0".to_owned() } else { text }
}

/// Four numbers as a rectangle, in `x y width height` order.
pub fn rect(values: [f32; 4]) -> String {
    format!(
        "rect({}, {}, {}, {})",
        float(values[0]),
        float(values[1]),
        float(values[2]),
        float(values[3])
    )
}

/// A list of numbers, comma separated.
pub fn list(values: &[f32]) -> String {
    let rendered: Vec<String> = values.iter().copied().map(float).collect();
    format!("[{}]", rendered.join(", "))
}

/// Whether every value in `values` is zero, which is what lets a field at its default be omitted.
pub fn all_zero(values: &[f32]) -> bool {
    values.iter().all(|value| *value == 0.0)
}

#[cfg(test)]
mod tests {
    use super::{all_zero, float, list, rect};

    #[test]
    fn a_negative_zero_prints_as_a_zero() {
        assert_eq!(float(-0.0), float(0.0));
    }

    #[test]
    fn accumulated_error_below_the_last_decimal_prints_the_same() {
        assert_eq!(float(0.1 + 0.2), float(0.3));
        assert_eq!(float(1.000_004), float(1.0));
    }

    #[test]
    fn a_difference_a_pixel_could_see_still_prints_differently() {
        // The control for the test above: rounding must not be so coarse that it hides movement.
        assert_ne!(float(1.0), float(1.001));
        assert_ne!(float(0.0), float(0.0001));
    }

    #[test]
    fn the_non_finite_values_have_names_rather_than_symbols() {
        assert_eq!(float(f32::INFINITY), "inf");
        assert_eq!(float(f32::NEG_INFINITY), "-inf");
        assert_eq!(float(f32::NAN), "nan");
    }

    #[test]
    fn a_rectangle_and_a_list_read_as_one_field() {
        assert_eq!(rect([0.0, 1.5, 64.0, 24.0]), "rect(0, 1.5, 64, 24)");
        assert_eq!(list(&[1.0, 2.0]), "[1, 2]");
        assert!(all_zero(&[0.0, -0.0]));
        assert!(!all_zero(&[0.0, 0.5]));
    }
}
