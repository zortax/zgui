//! The transfer functions that relate an RGB space's encoded values to light.
//!
//! Every function here is odd — `f(-x) == -f(x)` — because a colour outside a gamut has negative
//! channels and an interpolation that runs through one has to come back out the far side. Folding
//! the sign out and back in is what CSS Color 4 specifies and what keeps
//! [`Color::to_space`](crate::Color::to_space) reversible.

/// Applies `transfer` to the magnitude of `value` and puts the sign back.
fn signed(value: f32, transfer: impl Fn(f32) -> f32) -> f32 {
    let magnitude = transfer(value.abs());
    if value < 0.0 { -magnitude } else { magnitude }
}

/// Decodes one gamma-encoded sRGB channel to linear light.
///
/// The same curve serves [`ColorSpace::DisplayP3`](crate::ColorSpace::DisplayP3), which shares
/// sRGB's transfer function and differs only in its primaries.
pub(crate) fn srgb_to_linear(value: f32) -> f32 {
    signed(value, |magnitude| {
        if magnitude <= 0.040_45 {
            magnitude / 12.92
        } else {
            ((magnitude + 0.055) / 1.055).powf(2.4)
        }
    })
}

/// Encodes one linear-light channel as gamma-encoded sRGB.
pub(crate) fn linear_to_srgb(value: f32) -> f32 {
    signed(value, |magnitude| {
        if magnitude <= 0.003_130_8 {
            magnitude * 12.92
        } else {
            1.055 * magnitude.powf(1.0 / 2.4) - 0.055
        }
    })
}

/// Adobe RGB (1998)'s encoding exponent, 563/256.
const A98_EXPONENT: f32 = 563.0 / 256.0;

/// Decodes one Adobe RGB (1998) channel to linear light.
pub(crate) fn a98_to_linear(value: f32) -> f32 {
    signed(value, |magnitude| magnitude.powf(A98_EXPONENT))
}

/// Encodes one linear-light channel as Adobe RGB (1998).
pub(crate) fn linear_to_a98(value: f32) -> f32 {
    signed(value, |magnitude| magnitude.powf(1.0 / A98_EXPONENT))
}

/// Decodes one ProPhoto RGB channel to linear light.
pub(crate) fn prophoto_to_linear(value: f32) -> f32 {
    signed(value, |magnitude| {
        if magnitude <= 16.0 / 512.0 {
            magnitude / 16.0
        } else {
            magnitude.powf(1.8)
        }
    })
}

/// Encodes one linear-light channel as ProPhoto RGB.
pub(crate) fn linear_to_prophoto(value: f32) -> f32 {
    signed(value, |magnitude| {
        if magnitude >= 1.0 / 512.0 {
            magnitude.powf(1.0 / 1.8)
        } else {
            magnitude * 16.0
        }
    })
}

/// The scale factor of the BT.2020 transfer function's power segment.
const REC2020_ALPHA: f32 = 1.099_296_8;

/// Where the BT.2020 transfer function's linear segment ends, in linear light.
const REC2020_BETA: f32 = 0.018_053_97;

/// Decodes one BT.2020 channel to linear light.
pub(crate) fn rec2020_to_linear(value: f32) -> f32 {
    signed(value, |magnitude| {
        if magnitude < REC2020_BETA * 4.5 {
            magnitude / 4.5
        } else {
            ((magnitude + REC2020_ALPHA - 1.0) / REC2020_ALPHA).powf(1.0 / 0.45)
        }
    })
}

/// Encodes one linear-light channel as BT.2020.
pub(crate) fn linear_to_rec2020(value: f32) -> f32 {
    signed(value, |magnitude| {
        if magnitude > REC2020_BETA {
            REC2020_ALPHA * magnitude.powf(0.45) - (REC2020_ALPHA - 1.0)
        } else {
            magnitude * 4.5
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        a98_to_linear, linear_to_a98, linear_to_prophoto, linear_to_rec2020, linear_to_srgb,
        prophoto_to_linear, rec2020_to_linear, srgb_to_linear,
    };

    /// An encoding function and the decoding function that undoes it.
    type Pair = (fn(f32) -> f32, fn(f32) -> f32);

    /// Every pair, as `(encode, decode)`.
    const PAIRS: [Pair; 4] = [
        (linear_to_srgb, srgb_to_linear),
        (linear_to_a98, a98_to_linear),
        (linear_to_prophoto, prophoto_to_linear),
        (linear_to_rec2020, rec2020_to_linear),
    ];

    #[test]
    fn zero_and_one_are_fixed_points() {
        for (encode, decode) in PAIRS {
            assert!(encode(0.0).abs() < 1e-6);
            assert!((encode(1.0) - 1.0).abs() < 1e-5);
            assert!(decode(0.0).abs() < 1e-6);
            assert!((decode(1.0) - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn encoding_and_decoding_are_inverses() {
        for (encode, decode) in PAIRS {
            for step in -32i16..=100 {
                let value = f32::from(step) / 100.0;
                let round_tripped = decode(encode(value));
                assert!(
                    (round_tripped - value).abs() < 1e-5,
                    "{value} became {round_tripped}",
                );
            }
        }
    }

    #[test]
    fn every_curve_is_odd() {
        for (encode, decode) in PAIRS {
            for step in 1i16..=20 {
                let value = f32::from(step) / 20.0;
                assert!((encode(-value) + encode(value)).abs() < 1e-6);
                assert!((decode(-value) + decode(value)).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn srgb_matches_known_values() {
        // The midpoint of the 8-bit range, and the sample CSS Color 4 quotes for it.
        assert!((srgb_to_linear(0.5) - 0.214_041_14).abs() < 1e-5);
        assert!((linear_to_srgb(0.214_041_14) - 0.5).abs() < 1e-5);
        // Inside the linear segment the curve is exactly a division by 12.92.
        assert!((srgb_to_linear(0.02) - 0.02 / 12.92).abs() < 1e-7);
    }
}
