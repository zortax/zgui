//! Hue, whiteness and blackness, over gamma-encoded sRGB.
//!
//! HWB names a colour the way a painter would: take the pure hue, add this much white, add this
//! much black. Whiteness and blackness are fractions here, not percentages, so
//! `hwb(90deg 20% 30%)` is `[90.0, 0.2, 0.3]`. When they sum to one or more the hue is gone and
//! the result is the grey their ratio implies.

use crate::convert::hsl;
use crate::convert::polar::normalize_hue;

/// Converts `[hue, whiteness, blackness]` to gamma-encoded sRGB.
pub(crate) fn to_srgb(hwb: [f32; 3]) -> [f32; 3] {
    let [hue, whiteness, blackness] = hwb;
    if whiteness + blackness >= 1.0 {
        let grey = whiteness / (whiteness + blackness);
        return [grey, grey, grey];
    }
    let span = 1.0 - whiteness - blackness;
    hsl::to_srgb([hue, 1.0, 0.5]).map(|channel| channel * span + whiteness)
}

/// Converts gamma-encoded sRGB to `[hue, whiteness, blackness]`.
pub(crate) fn from_srgb(srgb: [f32; 3]) -> [f32; 3] {
    let [red, green, blue] = srgb;
    let hue = hsl::from_srgb(srgb)[0];
    let whiteness = red.min(green).min(blue);
    let blackness = 1.0 - red.max(green).max(blue);
    [normalize_hue(hue), whiteness, blackness]
}

#[cfg(test)]
mod tests {
    use super::{from_srgb, to_srgb};

    #[test]
    fn a_pure_hue_has_no_white_and_no_black() {
        assert_eq!(from_srgb([1.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
        assert_eq!(to_srgb([0.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
    }

    #[test]
    fn white_and_black_are_the_extremes() {
        assert_eq!(from_srgb([1.0, 1.0, 1.0]), [0.0, 1.0, 0.0]);
        assert_eq!(from_srgb([0.0, 0.0, 0.0]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn oversubscribed_white_and_black_give_their_ratio_as_a_grey() {
        assert_eq!(to_srgb([120.0, 0.6, 0.6]), [0.5, 0.5, 0.5]);
        assert_eq!(to_srgb([0.0, 1.0, 3.0]), [0.25, 0.25, 0.25]);
    }

    #[test]
    fn the_two_directions_are_inverses() {
        for red in 0u8..=8 {
            for green in 0u8..=8 {
                for blue in 0u8..=8 {
                    let srgb = [
                        f32::from(red) / 8.0,
                        f32::from(green) / 8.0,
                        f32::from(blue) / 8.0,
                    ];
                    let back = to_srgb(from_srgb(srgb));
                    for channel in 0..3 {
                        assert!(
                            (back[channel] - srgb[channel]).abs() < 1e-6,
                            "{srgb:?} came back as {back:?}",
                        );
                    }
                }
            }
        }
    }
}
