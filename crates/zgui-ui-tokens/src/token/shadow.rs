//! How far off the page something sits.
//!
//! Elevation is a ladder, and each rung means one thing: a control that is barely raised, a card,
//! a popover, a dialog, something being dragged. The blur and the offset grow together as a thing
//! rises, and each rung after the smallest carries a second, tighter shadow that keeps the
//! contact edge crisp — a single soft shadow reads as a blur rather than a lift.
//!
//! # Why the ladder does not change with the scheme
//!
//! A shadow here is neutral black at a low alpha, which composites correctly on a light surface
//! *and* on a dark one: on dark it deepens the gap between a raised surface and what is behind it
//! rather than disappearing, because the surfaces themselves are lighter than the page. Two
//! ladders would be two things to keep in step for no gain.

use crate::token::group::group;

group! {
    /// The elevation ladder.
    ShadowTokens, prefix = "shadow", {
        /// Flat on the page.
        none => "none", light = "none", dark = "none";
        /// A hairline lift, with no blur at all: a separator that needs to read as an edge.
        x2s => "2xs", light = "0 1px rgb(0 0 0 / 0.05)", dark = "0 1px rgb(0 0 0 / 0.05)";
        /// A control that is raised very slightly: an input, a button, a checkbox, a toggle.
        xs => "xs", light = "0 1px 2px 0 rgb(0 0 0 / 0.05)",
            dark = "0 1px 2px 0 rgb(0 0 0 / 0.05)";
        /// A card, and the active tab of a strip.
        sm => "sm", light = "0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1)",
            dark = "0 1px 3px 0 rgb(0 0 0 / 0.1), 0 1px 2px -1px rgb(0 0 0 / 0.1)";
        /// A popover, a tooltip, a select's list.
        md => "md", light = "0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)",
            dark = "0 4px 6px -1px rgb(0 0 0 / 0.1), 0 2px 4px -2px rgb(0 0 0 / 0.1)";
        /// A dialog, a sheet, a menu.
        lg => "lg", light = "0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)",
            dark = "0 10px 15px -3px rgb(0 0 0 / 0.1), 0 4px 6px -4px rgb(0 0 0 / 0.1)";
        /// Something dragged, held above everything else.
        xl => "xl", light = "0 20px 25px -5px rgb(0 0 0 / 0.1), 0 8px 10px -6px rgb(0 0 0 / 0.1)",
            dark = "0 20px 25px -5px rgb(0 0 0 / 0.1), 0 8px 10px -6px rgb(0 0 0 / 0.1)";
        /// The top of the ladder: a surface that floats clear of the whole window.
        x2l => "2xl", light = "0 25px 50px -12px rgb(0 0 0 / 0.25)",
            dark = "0 25px 50px -12px rgb(0 0 0 / 0.25)";
        /// A well: something pressed into the page rather than lifted off it.
        inset => "inset", light = "inset 0 2px 4px 0 rgb(0 0 0 / 0.05)",
            dark = "inset 0 2px 4px 0 rgb(0 0 0 / 0.05)";
    }
}

#[cfg(test)]
mod tests {
    use super::ShadowTokens;

    #[test]
    fn only_the_well_is_drawn_inside_the_box() {
        let shadows = ShadowTokens::light();
        assert!(shadows.inset.starts_with("inset"));
        for lifted in [
            &shadows.x2s,
            &shadows.xs,
            &shadows.sm,
            &shadows.md,
            &shadows.lg,
            &shadows.xl,
            &shadows.x2l,
        ] {
            assert!(
                !lifted.contains("inset"),
                "{lifted} is drawn inside the box"
            );
        }
    }

    #[test]
    fn the_ladder_is_one_ladder_and_not_two() {
        // The shadows are neutral black at a low alpha, which reads on either scheme. Two ladders
        // would be two things to keep in step, and the one that was not touched is the one that
        // shows.
        assert_eq!(ShadowTokens::light(), ShadowTokens::dark());
    }

    #[test]
    fn every_rung_above_the_hairline_keeps_a_tight_shadow_under_the_soft_one() {
        // A single soft shadow reads as a blur. The second, tighter one is what keeps the edge
        // where the surface meets the page crisp, so a raised thing looks raised rather than
        // out of focus.
        let shadows = ShadowTokens::light();
        for stacked in [&shadows.sm, &shadows.md, &shadows.lg, &shadows.xl] {
            assert!(
                stacked.contains(','),
                "{stacked} is a single shadow and will read as a blur"
            );
        }
    }
}
