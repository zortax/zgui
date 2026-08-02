//! How long a change takes, and how it is paced.
//!
//! Six durations and four curves. Durations are short on purpose: an interface's motion is there
//! to say *what moved where*, and anything a user waits for has stopped doing that.
//!
//! | Rung | What moves on it |
//! |---|---|
//! | `duration-none` | nothing: what a reduced-motion override sets everything to |
//! | `duration-fast` | a colour, a border or a shadow settling under the pointer |
//! | `duration-normal` | the default a bare `transition` takes |
//! | `duration-slow` | a popover, a menu, a tooltip or a dialog arriving and leaving |
//! | `duration-slower` | a sheet or a drawer leaving |
//! | `duration-slowest` | a sheet or a drawer arriving, which travels furthest |
//!
//! Arriving is slower than leaving for the large surfaces. That asymmetry is deliberate: something
//! entering has to be *read*, and something leaving has already been.
//!
//! The tokens are CSS values, so they are usable directly in `transition` and `animation`
//! shorthands — which is what lets an exit animation be a keyframe in a style sheet rather than a
//! duration guessed in Rust.

use crate::token::group::group;

group! {
    /// The durations and curves an interface moves on.
    MotionTokens, prefix = "motion", {
        /// No time at all: what a reduced-motion override sets everything to.
        duration_none => "duration-none", light = "0ms", dark = "0ms";
        /// A state change under the pointer: a hover, a press, a ring appearing.
        duration_fast => "duration-fast", light = "100ms", dark = "100ms";
        /// The default: what a transition takes when nothing says otherwise.
        duration_normal => "duration-normal", light = "150ms", dark = "150ms";
        /// A surface appearing or disappearing: a popover, a menu, a tooltip, a dialog.
        duration_slow => "duration-slow", light = "200ms", dark = "200ms";
        /// A large surface leaving: a sheet, a drawer.
        duration_slower => "duration-slower", light = "300ms", dark = "300ms";
        /// A large surface arriving, which travels furthest and has to be read on the way in.
        duration_slowest => "duration-slowest", light = "500ms", dark = "500ms";

        /// The default: eases in and out, for something that moves and stops.
        ease_standard => "ease-standard", light = "cubic-bezier(0.4, 0, 0.2, 1)",
            dark = "cubic-bezier(0.4, 0, 0.2, 1)";
        /// For something entering: fast at the start, settling at the end.
        ease_out => "ease-out", light = "cubic-bezier(0, 0, 0.2, 1)",
            dark = "cubic-bezier(0, 0, 0.2, 1)";
        /// For something leaving: gentle at the start, quick at the end.
        ease_in => "ease-in", light = "cubic-bezier(0.4, 0, 1, 1)",
            dark = "cubic-bezier(0.4, 0, 1, 1)";
        /// For something that repeats or tracks a value: a spinner, a progress bar, a skeleton.
        ///
        /// Anything that eases will visibly stutter where one repetition meets the next.
        ease_linear => "ease-linear", light = "linear", dark = "linear";
    }
}

#[cfg(test)]
mod tests {
    use super::MotionTokens;

    #[test]
    fn nothing_takes_longer_than_half_a_second() {
        let motion = MotionTokens::light();
        for duration in [
            &motion.duration_none,
            &motion.duration_fast,
            &motion.duration_normal,
            &motion.duration_slow,
            &motion.duration_slower,
            &motion.duration_slowest,
        ] {
            let milliseconds: f32 = duration
                .trim_end_matches("ms")
                .parse()
                .expect("every default duration is written in milliseconds");
            assert!(
                milliseconds <= 500.0,
                "{duration} is long enough to wait for"
            );
        }
    }

    #[test]
    fn the_ladder_only_ever_grows() {
        let motion = MotionTokens::light();
        let rungs: Vec<f32> = [
            &motion.duration_none,
            &motion.duration_fast,
            &motion.duration_normal,
            &motion.duration_slow,
            &motion.duration_slower,
            &motion.duration_slowest,
        ]
        .iter()
        .map(|rung| rung.trim_end_matches("ms").parse().expect("milliseconds"))
        .collect();
        assert!(rungs.windows(2).all(|pair| pair[0] < pair[1]), "{rungs:?}");
    }

    #[test]
    fn the_one_curve_that_is_not_a_curve_is_the_repeating_one() {
        // A spinner or a pulse that eased would visibly stutter where one repetition meets the
        // next, so the repeating curve is the straight line and every other one bends.
        let motion = MotionTokens::light();
        assert_eq!(motion.ease_linear, "linear");
        for curve in [&motion.ease_standard, &motion.ease_out, &motion.ease_in] {
            assert!(curve.starts_with("cubic-bezier("), "{curve} does not bend");
        }
    }
}
