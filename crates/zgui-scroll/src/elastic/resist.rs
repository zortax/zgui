//! How far the content moves for a delta that has nowhere to go.
//!
//! The curve is the rubber band of AppKit and UIKit: `d·(1 − 1/(c·x/d + 1))`, where `x` is the
//! distance pulled and `d` the extent of the scrollport on that axis. The first pixels move the
//! content at `c` of the finger's speed, and no pull, however long, moves it a full scrollport.

/// How stiff the band is: the share of the first pixels pulled that move the content.
const COEFFICIENT: f32 = 0.55;

/// The smallest extent the band is measured against, in device pixels.
///
/// A scrollport with no size would divide by zero.
const SMALLEST_EXTENT: f32 = 1.0;

/// The displacement reached by pulling `pulled` device pixels against a port `extent` long.
pub(crate) fn band(pulled: f32, extent: f32) -> f32 {
    let extent = extent.max(SMALLEST_EXTENT);
    let magnitude = pulled.abs();
    pulled.signum() * extent * (1.0 - 1.0 / (COEFFICIENT * magnitude / extent + 1.0))
}

/// The distance that was pulled to reach a displacement of `held`, the inverse of [`band`].
pub(crate) fn unband(held: f32, extent: f32) -> f32 {
    let extent = extent.max(SMALLEST_EXTENT);
    let magnitude = held.abs().min(extent * 0.999);
    held.signum() * extent / COEFFICIENT * (1.0 / (1.0 - magnitude / extent) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::{band, unband};

    #[test]
    fn no_pull_leaves_the_scrollport() {
        let held = band(1.0e7, 600.0);
        assert!(held < 600.0, "a long pull moved the content {held}");
    }

    #[test]
    fn the_first_pixels_move_at_about_half_speed_and_the_later_ones_slower() {
        let early = band(2.0, 600.0);
        let late = band(602.0, 600.0) - band(600.0, 600.0);
        assert!((early - 1.1).abs() < 0.01, "{early}");
        assert!(late < early / 2.0, "{late}");
    }

    #[test]
    fn a_larger_port_stretches_further_for_the_same_pull() {
        assert!(band(200.0, 1200.0) > band(200.0, 300.0));
    }

    #[test]
    fn the_inverse_finds_the_pull_again() {
        for pulled in [-300.0, -3.0, 0.0, 5.0, 250.0] {
            assert!((unband(band(pulled, 600.0), 600.0) - pulled).abs() < 1e-2);
        }
    }
}
