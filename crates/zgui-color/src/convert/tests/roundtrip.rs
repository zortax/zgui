//! Reversibility: a colour that goes out to another space and comes back is the colour it was.
//!
//! Nothing here clamps, so this is a real property rather than a tautology — a conversion that
//! silently dropped an out-of-range channel, adapted the wrong way round, or lost the sign of a
//! negative channel would show up as a colour that does not come home.

use crate::color::Color;
use crate::space::ColorSpace;

/// How far a channel may drift over a round trip through another space, in sRGB units.
const TOLERANCE: f32 = 1e-4;

/// How many colours the sweep visits.
const SAMPLES: usize = 10_000;

/// A small deterministic generator, so a failure is reproducible without a seed to record.
struct Sequence {
    /// The generator's state.
    state: u64,
}

impl Sequence {
    /// A generator with a fixed starting state.
    const fn new() -> Self {
        Self {
            state: 0x2545_f491_4f6c_dd1d,
        }
    }

    /// The next value in `0..1`.
    fn next(&mut self) -> f32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let bits = u32::try_from(self.state >> 40).expect("24 bits fit in a u32");
        bits as f32 / 16_777_216.0
    }

    /// The next sRGB colour.
    fn next_color(&mut self) -> Color {
        Color::srgb(self.next(), self.next(), self.next(), self.next())
    }
}

#[test]
fn ten_thousand_colours_survive_every_space() {
    let mut sequence = Sequence::new();
    for _ in 0..SAMPLES {
        let color = sequence.next_color();
        for space in ColorSpace::ALL {
            let back = color.to_space(space).to_space(ColorSpace::Srgb);
            for channel in 0..3 {
                let drift = (back.components()[channel] - color.components()[channel]).abs();
                assert!(
                    drift <= TOLERANCE,
                    "{space:?}: {:?} came back as {:?}, channel {channel} drifted {drift}",
                    color.components(),
                    back.components(),
                );
            }
            assert_eq!(back.alpha(), color.alpha(), "{space:?} changed the alpha");
        }
    }
}

#[test]
fn every_pair_of_spaces_round_trips() {
    // Not just via sRGB: the shortcuts between related spaces are separate code paths, and a
    // mistake in one of them would hide behind a correct hub conversion.
    let mut sequence = Sequence::new();
    for _ in 0..200 {
        let color = sequence.next_color();
        for outward in ColorSpace::ALL {
            let start = color.to_space(outward);
            for inward in ColorSpace::ALL {
                let back = start.to_space(inward).to_space(outward);
                let reference = start.to_space(ColorSpace::Srgb);
                let actual = back.to_space(ColorSpace::Srgb);
                for channel in 0..3 {
                    let drift =
                        (actual.components()[channel] - reference.components()[channel]).abs();
                    assert!(
                        drift <= TOLERANCE,
                        "{outward:?} → {inward:?} → {outward:?} drifted {drift} on channel \
                         {channel} of {:?}",
                        color.components(),
                    );
                }
            }
        }
    }
}

#[test]
fn out_of_gamut_colours_round_trip_too() {
    // Wide-gamut colours have negative sRGB channels, and the transfer functions have to carry the
    // sign through rather than folding it away.
    let mut sequence = Sequence::new();
    for _ in 0..500 {
        let wide = Color::new(
            ColorSpace::Rec2020,
            [sequence.next(), sequence.next(), sequence.next()],
            1.0,
        );
        let back = wide
            .to_space(ColorSpace::Srgb)
            .to_space(ColorSpace::Rec2020);
        for channel in 0..3 {
            let drift = (back.components()[channel] - wide.components()[channel]).abs();
            assert!(
                drift <= TOLERANCE,
                "{:?} came back as {:?}",
                wide.components(),
                back.components(),
            );
        }
    }
}
