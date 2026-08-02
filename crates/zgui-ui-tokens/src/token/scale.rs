//! Two twelve-step colour ramps, for anything that needs a graded series rather than a name.
//!
//! These are not what the [semantic colours](crate::ColorTokens) resolve through — those hold
//! measured values, for the reasons given there. A ramp is what to reach for when a component
//! needs *more steps than the semantic tokens name*: a heat map, a set of nested surfaces, a
//! series of states with no name of its own.
//!
//! Two ramps of twelve steps: a neutral one and an accent one. The steps mean the same thing in
//! both schemes and in every ramp, which is what makes a component's rules readable — step 3 is a
//! subtle surface whether the interface is light or dark, and a component that wants one asks for
//! step 3 rather than for a colour.
//!
//! | Steps | What they are for |
//! |---|---|
//! | 1–2 | page and surface backgrounds |
//! | 3–5 | component backgrounds: at rest, hovered, pressed |
//! | 6–8 | borders: subtle, ordinary, and the one under a pointer |
//! | 9–10 | the solid fill, and the solid fill under a pointer |
//! | 11–12 | text: the readable weight, and the high-contrast one |
//!
//! The dark ramps are not the light ones reversed. Each is chosen so that the *contrast* a step
//! carries is the same in both, which is why a component styled in steps needs no dark-mode rules
//! of its own.

use crate::token::group::group;

group! {
    /// The neutral and accent colour ramps, twelve steps each.
    ScaleTokens, prefix = "scale", {
        /// Neutral step 1: the page behind everything.
        neutral_1 => "neutral-1", light = "#fcfcfd", dark = "#111113";
        /// Neutral step 2: a surface raised off the page.
        neutral_2 => "neutral-2", light = "#f9f9fb", dark = "#18191b";
        /// Neutral step 3: a component at rest.
        neutral_3 => "neutral-3", light = "#f0f0f3", dark = "#212225";
        /// Neutral step 4: a component under the pointer.
        neutral_4 => "neutral-4", light = "#e8e8ec", dark = "#272a2d";
        /// Neutral step 5: a component being pressed.
        neutral_5 => "neutral-5", light = "#e0e1e6", dark = "#2e3135";
        /// Neutral step 6: a subtle border.
        neutral_6 => "neutral-6", light = "#d9d9e0", dark = "#363a3f";
        /// Neutral step 7: an ordinary border.
        neutral_7 => "neutral-7", light = "#cdced6", dark = "#43484e";
        /// Neutral step 8: a border under the pointer.
        neutral_8 => "neutral-8", light = "#b9bbc6", dark = "#5a6169";
        /// Neutral step 9: the solid fill.
        neutral_9 => "neutral-9", light = "#8b8d98", dark = "#696e77";
        /// Neutral step 10: the solid fill under the pointer.
        neutral_10 => "neutral-10", light = "#80838d", dark = "#777b84";
        /// Neutral step 11: readable secondary text.
        neutral_11 => "neutral-11", light = "#60646c", dark = "#b0b4ba";
        /// Neutral step 12: the highest-contrast text.
        neutral_12 => "neutral-12", light = "#1c2024", dark = "#edeef0";

        /// Accent step 1: a page tinted towards the accent.
        accent_1 => "accent-1", light = "#fbfdff", dark = "#0d1520";
        /// Accent step 2: a tinted surface.
        accent_2 => "accent-2", light = "#f4faff", dark = "#111927";
        /// Accent step 3: a tinted component at rest.
        accent_3 => "accent-3", light = "#e6f4fe", dark = "#0d2847";
        /// Accent step 4: a tinted component under the pointer.
        accent_4 => "accent-4", light = "#d5efff", dark = "#003362";
        /// Accent step 5: a tinted component being pressed.
        accent_5 => "accent-5", light = "#c2e5ff", dark = "#004074";
        /// Accent step 6: a subtle tinted border.
        accent_6 => "accent-6", light = "#acd8fc", dark = "#104d87";
        /// Accent step 7: an ordinary tinted border.
        accent_7 => "accent-7", light = "#8ec8f6", dark = "#205d9e";
        /// Accent step 8: the focus ring.
        accent_8 => "accent-8", light = "#5eb1ef", dark = "#2870bd";
        /// Accent step 9: the solid accent fill.
        accent_9 => "accent-9", light = "#0090ff", dark = "#0090ff";
        /// Accent step 10: the solid accent fill under the pointer.
        accent_10 => "accent-10", light = "#0588f0", dark = "#3b9eff";
        /// Accent step 11: readable accent text.
        accent_11 => "accent-11", light = "#0d74ce", dark = "#70b8ff";
        /// Accent step 12: the highest-contrast accent text.
        accent_12 => "accent-12", light = "#113264", dark = "#c2e6ff";
    }
}

#[cfg(test)]
mod tests {
    use super::ScaleTokens;

    #[test]
    fn each_ramp_has_twelve_steps() {
        assert_eq!(ScaleTokens::PROPERTIES.len(), 24);
        let neutral = ScaleTokens::PROPERTIES
            .iter()
            .filter(|name| name.contains("neutral"))
            .count();
        assert_eq!(neutral, 12);
    }

    #[test]
    fn the_dark_ramp_is_not_the_light_one_reversed() {
        // Stated as a test because "just reverse it" is the wrong answer that looks right: a
        // reversed ramp puts the same *lightness* at each step rather than the same contrast, and
        // every border in the interface comes out too strong.
        let light = ScaleTokens::light();
        let dark = ScaleTokens::dark();
        let reversed: Vec<&str> = light
            .pairs()
            .iter()
            .rev()
            .take(12)
            .map(|pair| pair.1)
            .collect();
        let dark_neutral: Vec<&str> = dark.pairs().iter().take(12).map(|pair| pair.1).collect();
        assert_ne!(reversed, dark_neutral);
    }

    #[test]
    fn the_solid_accent_fill_is_the_one_step_that_holds_across_both_schemes() {
        // Step 9 is a brand colour, and a brand colour that changed with the scheme would not be
        // one. Every other step moves.
        assert_eq!(ScaleTokens::light().accent_9, ScaleTokens::dark().accent_9);
        assert_ne!(ScaleTokens::light().accent_3, ScaleTokens::dark().accent_3);
    }
}
