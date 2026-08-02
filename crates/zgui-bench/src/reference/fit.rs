//! The straight line through a handful of (size, cost) points.

/// The least-squares slope of `points`, in cost units per unit of size.
///
/// The slope alone, without the intercept: the intercept is whatever a frame of an empty document
/// costs, and it is exactly the part that says nothing about how the cost grows.
///
/// Returns `None` when the points do not determine a line — fewer than two of them, or every one
/// at the same size.
///
/// ```
/// use zgui_bench::reference::fit;
///
/// let points = [(100.0, 12.0), (200.0, 15.0), (300.0, 18.0)];
/// let slope = fit::slope(&points).expect("three distinct sizes determine a line");
/// assert!((slope - 0.03).abs() < 1e-9);
/// ```
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "the count of points in a sweep is four, and the arithmetic below is a mean of them"
)]
pub fn slope(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / n;
    let mut covariance = 0.0;
    let mut variance = 0.0;
    for (x, y) in points {
        covariance += (x - mean_x) * (y - mean_y);
        variance += (x - mean_x) * (x - mean_x);
    }
    (variance > 0.0).then(|| covariance / variance)
}

#[cfg(test)]
mod tests {
    use super::slope;

    #[test]
    fn a_straight_line_gives_back_its_own_slope() {
        let points = [(1.0, 12.0), (2.0, 15.0), (3.0, 18.0), (4.0, 21.0)];
        let found = slope(&points).expect("four distinct sizes determine a line");
        assert!((found - 3.0).abs() < 1e-9, "{found}");
    }

    #[test]
    fn the_intercept_is_not_in_the_answer() {
        // Two cost curves with the same growth and very different constants have the same slope,
        // which is the whole reason the slope is the thing compared.
        let cheap = [(1.0, 0.0), (2.0, 3.0), (3.0, 6.0)];
        let dear = [(1.0, 900.0), (2.0, 903.0), (3.0, 906.0)];
        assert!((slope(&cheap).unwrap() - slope(&dear).unwrap()).abs() < 1e-9);
    }

    #[test]
    fn a_cost_that_does_not_grow_with_the_document_has_no_slope_at_all() {
        // The answer a virtualised list is supposed to give: four sizes, one cost.
        let flat = [(12_500.0, 410.0), (25_000.0, 410.0), (100_000.0, 410.0)];
        assert!(slope(&flat).unwrap().abs() < 1e-12);
    }

    #[test]
    fn points_that_do_not_determine_a_line_give_nothing() {
        assert_eq!(slope(&[]), None);
        assert_eq!(slope(&[(1.0, 1.0)]), None);
        assert_eq!(slope(&[(2.0, 1.0), (2.0, 9.0)]), None);
    }
}
