//! How round a corner is.
//!
//! A base and eight steps. Every step between the two ends is a *multiple* of
//! `--zui-radius-base`, so an application that wants a squarer or a rounder interface sets one
//! property and the whole thing follows in proportion — which is the single most common thing
//! anyone changes about a component library.
//!
//! Multiples rather than offsets, because an offset ladder collapses: subtract four pixels from a
//! base of three and the small step is negative, and every checkbox in the interface goes square
//! while the cards stay round.

use crate::token::group::group;

group! {
    /// The corner radii an interface is built from.
    RadiusTokens, prefix = "radius", {
        /// What every other step is derived from.
        base => "base", light = "10px", dark = "10px";
        /// A square corner.
        none => "none", light = "0px", dark = "0px";
        /// The smallest rounding: a badge on a dense row, a highlighted run of code.
        sm => "sm", light = "calc(var(--zui-radius-base) * 0.6)",
            dark = "calc(var(--zui-radius-base) * 0.6)";
        /// A control: a button, an input, a menu item, a popover.
        md => "md", light = "calc(var(--zui-radius-base) * 0.8)",
            dark = "calc(var(--zui-radius-base) * 0.8)";
        /// A surface that takes the window over: a dialog.
        lg => "lg", light = "var(--zui-radius-base)", dark = "var(--zui-radius-base)";
        /// A card, and a sidebar floating off the page.
        xl => "xl", light = "calc(var(--zui-radius-base) * 1.4)",
            dark = "calc(var(--zui-radius-base) * 1.4)";
        /// A large panel: a code block, a figure.
        x2l => "2xl", light = "calc(var(--zui-radius-base) * 1.8)",
            dark = "calc(var(--zui-radius-base) * 1.8)";
        /// A region of a page.
        x3l => "3xl", light = "calc(var(--zui-radius-base) * 2.2)",
            dark = "calc(var(--zui-radius-base) * 2.2)";
        /// The roundest rectangle in the interface.
        x4l => "4xl", light = "calc(var(--zui-radius-base) * 2.6)",
            dark = "calc(var(--zui-radius-base) * 2.6)";
        /// A pill or a circle.
        full => "full", light = "9999px", dark = "9999px";
    }
}

#[cfg(test)]
mod tests {
    use super::RadiusTokens;

    #[test]
    fn every_step_but_the_ends_is_derived_from_the_base() {
        let radii = RadiusTokens::light();
        for step in [
            &radii.sm, &radii.md, &radii.lg, &radii.xl, &radii.x2l, &radii.x3l, &radii.x4l,
        ] {
            assert!(
                step.contains("var(--zui-radius-base)"),
                "{step} does not follow the base"
            );
        }
        // The two ends are absolute on purpose: a square corner stays square and a pill stays a
        // pill however the base moves.
        assert_eq!(radii.none, "0px");
        assert_eq!(radii.full, "9999px");
    }

    #[test]
    fn the_ladder_scales_rather_than_shifts() {
        // Stated as a test because "base minus four pixels" is the wrong answer that looks right:
        // it goes negative for any small base, and a whole interface loses its rounding at once
        // while the base still reads as a sensible number.
        let radii = RadiusTokens::light();
        for step in [&radii.sm, &radii.md, &radii.xl, &radii.x2l] {
            assert!(
                step.contains('*'),
                "{step} shifts the base instead of scaling it"
            );
        }
    }

    #[test]
    fn rounding_does_not_change_with_the_scheme() {
        assert_eq!(RadiusTokens::light(), RadiusTokens::dark());
    }
}
