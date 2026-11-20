//! How many device pixels one CSS pixel of a canvas covers.

/// The scale of a canvas that is `css` CSS pixels wide and was observed to be `device` device
/// pixels wide, on a page whose device pixel ratio is `ratio`.
///
/// The ratio is the answer wherever it agrees with what was observed. A width in device pixels is
/// a whole number, so the two differ by a rounding and no more. A browser that emulates another
/// display reports that display's ratio and this display's pixels, and there the two differ by a
/// factor. The observed pixels are the ones the canvas is shown with, so they decide: an
/// application that drew for the ratio would lay out for a canvas of another size than the one
/// the pointer is measured in.
pub(crate) fn scale(ratio: f64, css: f64, device: u32) -> f64 {
    let ratio = if ratio.is_finite() && ratio > 0.0 {
        ratio
    } else {
        1.0
    };
    if css < 32.0 || device == 0 {
        return ratio;
    }
    let observed = f64::from(device) / css;
    if (observed - ratio).abs() / ratio > 0.05 {
        observed
    } else {
        ratio
    }
}

#[cfg(test)]
mod tests {
    use super::scale;

    #[test]
    fn the_ratio_decides_where_the_observed_width_agrees_with_it() {
        assert_eq!(scale(2.0, 600.0, 1200), 2.0);
        // A fractional ratio rounds the width, and the ratio is still the answer.
        assert_eq!(scale(1.25, 601.0, 751), 1.25);
        assert_eq!(scale(1.5, 333.0, 500), 1.5);
    }

    #[test]
    fn an_emulated_ratio_gives_way_to_the_pixels_the_canvas_has() {
        assert_eq!(scale(2.0, 600.0, 600), 1.0);
        assert_eq!(scale(3.0, 400.0, 400), 1.0);
    }

    #[test]
    fn a_canvas_too_small_to_measure_keeps_the_ratio() {
        assert_eq!(scale(2.0, 1.0, 1), 2.0);
        assert_eq!(scale(f64::NAN, 600.0, 600), 1.0);
    }
}
