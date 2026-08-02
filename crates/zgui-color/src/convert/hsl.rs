//! Hue, saturation and lightness, over gamma-encoded sRGB.
//!
//! HSL is a re-parameterisation of sRGB rather than a space of its own: nothing is gained or lost
//! going either way, and the conversion never leaves the sRGB gamut. Saturation and lightness are
//! fractions here, not percentages, so `hsl(120deg 50% 50%)` is `[120.0, 0.5, 0.5]`.

use crate::convert::polar::normalize_hue;

/// Converts `[hue, saturation, lightness]` to gamma-encoded sRGB.
pub(crate) fn to_srgb(hsl: [f32; 3]) -> [f32; 3] {
    let [hue, saturation, lightness] = hsl;
    let hue = normalize_hue(hue);
    let amplitude = saturation * lightness.min(1.0 - lightness);
    let channel = |offset: f32| {
        let position = (offset + hue / 30.0) % 12.0;
        lightness - amplitude * (position - 3.0).min(9.0 - position).clamp(-1.0, 1.0)
    };
    [channel(0.0), channel(8.0), channel(4.0)]
}

/// Converts gamma-encoded sRGB to `[hue, saturation, lightness]`.
pub(crate) fn from_srgb(srgb: [f32; 3]) -> [f32; 3] {
    let [red, green, blue] = srgb;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let lightness = (min + max) / 2.0;
    let range = max - min;

    let mut hue = 0.0;
    let mut saturation = 0.0;
    if range != 0.0 {
        saturation = if lightness == 0.0 || lightness == 1.0 {
            0.0
        } else {
            (max - lightness) / lightness.min(1.0 - lightness)
        };
        hue = 60.0
            * if max == red {
                (green - blue) / range + if green < blue { 6.0 } else { 0.0 }
            } else if max == green {
                (blue - red) / range + 2.0
            } else {
                (red - green) / range + 4.0
            };
    }

    // A colour far outside the sRGB gamut produces a negative saturation, which names the same
    // colour as the opposite hue at the positive saturation.
    if saturation < 0.0 {
        hue += 180.0;
        saturation = saturation.abs();
    }
    [normalize_hue(hue), saturation, lightness]
}

#[cfg(test)]
mod tests {
    use super::{from_srgb, to_srgb};

    #[test]
    fn the_primaries_are_where_they_should_be() {
        assert_eq!(from_srgb([1.0, 0.0, 0.0]), [0.0, 1.0, 0.5]);
        assert_eq!(from_srgb([0.0, 1.0, 0.0]), [120.0, 1.0, 0.5]);
        assert_eq!(from_srgb([0.0, 0.0, 1.0]), [240.0, 1.0, 0.5]);
    }

    #[test]
    fn greys_have_no_hue_and_no_saturation() {
        assert_eq!(from_srgb([0.25, 0.25, 0.25]), [0.0, 0.0, 0.25]);
        assert_eq!(to_srgb([210.0, 0.0, 0.25]), [0.25, 0.25, 0.25]);
    }

    #[test]
    fn black_and_white_survive_the_division_by_zero() {
        assert_eq!(from_srgb([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
        assert_eq!(from_srgb([1.0, 1.0, 1.0]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn sixty_degree_steps_land_on_the_secondaries() {
        assert_eq!(to_srgb([60.0, 1.0, 0.5]), [1.0, 1.0, 0.0]);
        assert_eq!(to_srgb([180.0, 1.0, 0.5]), [0.0, 1.0, 1.0]);
        assert_eq!(to_srgb([300.0, 1.0, 0.5]), [1.0, 0.0, 1.0]);
    }

    #[test]
    fn the_two_directions_are_inverses() {
        for red in 0u8..=10 {
            for green in 0u8..=10 {
                for blue in 0u8..=10 {
                    let srgb = [
                        f32::from(red) / 10.0,
                        f32::from(green) / 10.0,
                        f32::from(blue) / 10.0,
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
