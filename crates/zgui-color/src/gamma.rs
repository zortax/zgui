// DERIVED-FROM: the Windows Terminal project, src/renderer/atlas/dwrite.cpp (MIT)
// Copyright (c) Microsoft Corporation.
// The gamma-correction ratio table and the index and normalisation arithmetic in this module are
// adapted from that work, which is distributed under the MIT License, and have been modified: the
// per-entry division by four is folded into the normalisation constants, and the result is
// expressed in this crate's own terms.

//! Gamma correction ratios for text rendering.
//!
//! Glyph coverage is not a colour, it is a fraction of a pixel that the glyph covers, and blending
//! it as though it were a colour makes light-on-dark text look thin and dark-on-light text look
//! heavy. The correction is a cubic in the coverage value whose four coefficients depend on the
//! display gamma, and [`gamma_correction_ratios`] is the table of those coefficients.
//!
//! The values are meant to be handed to a text shader as a `vec4`; nothing in this crate applies
//! them, because the coverage they correct exists only inside a rasteriser.

/// The published ratios, one row per tenth of a gamma value from 1.0 to 2.2.
const TARGET_RATIOS: [[f32; 4]; 13] = [
    [0.0000, 0.0000, 0.0000, 0.0000],   // gamma = 1.0
    [0.0166, -0.0807, 0.2227, -0.0751], // gamma = 1.1
    [0.0350, -0.1760, 0.4325, -0.1370], // gamma = 1.2
    [0.0543, -0.2821, 0.6302, -0.1876], // gamma = 1.3
    [0.0739, -0.3963, 0.8167, -0.2287], // gamma = 1.4
    [0.0933, -0.5161, 0.9926, -0.2616], // gamma = 1.5
    [0.1121, -0.6395, 1.1588, -0.2877], // gamma = 1.6
    [0.1300, -0.7649, 1.3159, -0.3080], // gamma = 1.7
    [0.1469, -0.8911, 1.4644, -0.3234], // gamma = 1.8
    [0.1627, -1.0170, 1.6051, -0.3347], // gamma = 1.9
    [0.1773, -1.1420, 1.7385, -0.3426], // gamma = 2.0
    [0.1908, -1.2652, 1.8650, -0.3476], // gamma = 2.1
    [0.2031, -1.3864, 1.9851, -0.3501], // gamma = 2.2
];

/// The normalisation applied to the first and third coefficients, which multiply squared and cubed
/// coverage values that were tabulated over the 16-bit range.
const NORMALISE_SQUARED: f32 = 65536.0 / (255.0 * 255.0);

/// The normalisation applied to the second and fourth coefficients, which multiply coverage values
/// tabulated over the 8-bit range.
const NORMALISE_LINEAR: f32 = 256.0 / 255.0;

/// The lowest gamma the table covers.
const LOWEST_GAMMA: f32 = 1.0;

/// The highest gamma the table covers.
const HIGHEST_GAMMA: f32 = 2.2;

/// The four gamma-correction coefficients for a display gamma.
///
/// The table is sampled at tenths, so a gamma is rounded to the nearest tenth and clamped to
/// `1.0..=2.2`; a gamma outside that range, or one that is not a number, gets the nearest row
/// rather than an error, because there is no sensible way for a text pass to fail here.
///
/// ```
/// use zgui_color::gamma_correction_ratios;
///
/// // A gamma of one is no correction at all.
/// assert_eq!(gamma_correction_ratios(1.0), [0.0; 4]);
/// // Rounding to the nearest tenth, and clamping at both ends.
/// assert_eq!(gamma_correction_ratios(2.24), gamma_correction_ratios(2.2));
/// assert_eq!(gamma_correction_ratios(0.1), gamma_correction_ratios(1.0));
/// ```
pub fn gamma_correction_ratios(gamma: f32) -> [f32; 4] {
    let tenths = (gamma * 10.0).round();
    let tenths = if tenths.is_nan() {
        LOWEST_GAMMA * 10.0
    } else {
        tenths.clamp(LOWEST_GAMMA * 10.0, HIGHEST_GAMMA * 10.0)
    };
    let row = TARGET_RATIOS[(tenths as usize) - 10];
    [
        row[0] * NORMALISE_SQUARED,
        row[1] * NORMALISE_LINEAR,
        row[2] * NORMALISE_SQUARED,
        row[3] * NORMALISE_LINEAR,
    ]
}

#[cfg(test)]
mod tests {
    use super::{NORMALISE_LINEAR, NORMALISE_SQUARED, TARGET_RATIOS, gamma_correction_ratios};

    #[test]
    fn every_row_is_reachable_at_the_gamma_it_belongs_to() {
        for (index, row) in TARGET_RATIOS.iter().enumerate() {
            let gamma = 1.0 + index as f32 / 10.0;
            let ratios = gamma_correction_ratios(gamma);
            for channel in 0..4 {
                let normalisation = if channel % 2 == 0 {
                    NORMALISE_SQUARED
                } else {
                    NORMALISE_LINEAR
                };
                assert!(
                    (ratios[channel] - row[channel] * normalisation).abs() < 1e-6,
                    "gamma {gamma} channel {channel} is {}",
                    ratios[channel],
                );
            }
        }
    }

    #[test]
    fn the_correction_grows_with_the_gamma() {
        let mut previous = gamma_correction_ratios(1.0)[0];
        for step in 11i16..=22 {
            let ratios = gamma_correction_ratios(f32::from(step) / 10.0);
            assert!(ratios[0] > previous, "gamma {step} did not grow");
            previous = ratios[0];
        }
    }

    #[test]
    fn out_of_range_and_nonsense_gammas_are_clamped() {
        assert_eq!(gamma_correction_ratios(-4.0), gamma_correction_ratios(1.0));
        assert_eq!(gamma_correction_ratios(1e9), gamma_correction_ratios(2.2));
        assert_eq!(
            gamma_correction_ratios(f32::NAN),
            gamma_correction_ratios(1.0)
        );
    }

    #[test]
    fn the_normalisation_is_close_to_unity() {
        // The published table is tabulated over 8- and 16-bit ranges; normalising it to unit
        // coverage is a nudge, not a rescale, and a mistake here would be a very visible one.
        let ratios = gamma_correction_ratios(2.2);
        assert!((ratios[0] / 0.2031 - 1.0).abs() < 0.02);
        assert!((ratios[3] / -0.3501 - 1.0).abs() < 0.02);
    }
}
