//! How much room there is between things.
//!
//! One base and seven named steps, used for padding, for gaps and for the space around a surface.
//!
//! # The base, and the sizes that are not on the ladder
//!
//! Every named step is a whole multiple of `--zui-space-base`, and the base is four pixels. That
//! is what makes the in-between sizes expressible without adding tokens for them: a control that
//! wants two and a half steps of horizontal padding writes
//!
//! ```
//! use zgui::css;
//!
//! const DENSE: &str = css!(
//!     ".dense {
//!         padding-inline: calc(var(--zui-space-base) * 2.5);
//!         gap: calc(var(--zui-space-base) * 1.5);
//!     }"
//! );
//! assert!(DENSE.contains("--zui-space-base"));
//! ```
//!
//! rather than reaching for a pixel length. An application that sets `--zui-space-base` to
//! something else moves the whole interface's density at once, including those.
//!
//! Sizes are absolute rather than a fraction of the text, because an interface whose gaps grow
//! with its type ends up as a menu that is mostly gap the moment anybody enlarges it. The type
//! scale and the spacing scale are moved by two separate properties for that reason.

use crate::token::group::group;

group! {
    /// The spacing steps an interface is laid out on.
    SpacingTokens, prefix = "space", {
        /// The unit every named step is a multiple of, and what an in-between size is built from.
        base => "base", light = "4px", dark = "4px";
        /// A hairline: the gap between an icon and the label beside it.
        xs => "xs", light = "calc(var(--zui-space-base) * 1)",
            dark = "calc(var(--zui-space-base) * 1)";
        /// Inside a dense control, and between a label and its field.
        sm => "sm", light = "calc(var(--zui-space-base) * 2)",
            dark = "calc(var(--zui-space-base) * 2)";
        /// Inside an ordinary control, and between related controls.
        md => "md", light = "calc(var(--zui-space-base) * 3)",
            dark = "calc(var(--zui-space-base) * 3)";
        /// Inside a surface, and between groups of controls.
        lg => "lg", light = "calc(var(--zui-space-base) * 4)",
            dark = "calc(var(--zui-space-base) * 4)";
        /// Inside a card or a dialog, and between sections of a page.
        xl => "xl", light = "calc(var(--zui-space-base) * 6)",
            dark = "calc(var(--zui-space-base) * 6)";
        /// Around a page's content.
        x2l => "2xl", light = "calc(var(--zui-space-base) * 8)",
            dark = "calc(var(--zui-space-base) * 8)";
        /// Between the major regions of a window.
        x3l => "3xl", light = "calc(var(--zui-space-base) * 12)",
            dark = "calc(var(--zui-space-base) * 12)";
    }
}

#[cfg(test)]
mod tests {
    use super::SpacingTokens;

    /// How many base units a step is, for a value this module wrote.
    fn steps(text: &str) -> f32 {
        text.trim_start_matches("calc(var(--zui-space-base) * ")
            .trim_end_matches(')')
            .parse()
            .expect("every named step is a plain multiple of the base")
    }

    #[test]
    fn the_scale_only_ever_grows() {
        let spacing = SpacingTokens::light();
        let ladder: Vec<f32> = [
            &spacing.xs,
            &spacing.sm,
            &spacing.md,
            &spacing.lg,
            &spacing.xl,
            &spacing.x2l,
            &spacing.x3l,
        ]
        .iter()
        .map(|step| steps(step))
        .collect();
        assert!(
            ladder.windows(2).all(|pair| pair[0] < pair[1]),
            "{ladder:?} is not increasing"
        );
    }

    #[test]
    fn every_named_step_follows_the_base() {
        // The property this protects: one override of one property changes the density of the
        // whole interface. A step written as an absolute length would silently stay put.
        let spacing = SpacingTokens::light();
        for step in [
            &spacing.xs,
            &spacing.sm,
            &spacing.md,
            &spacing.lg,
            &spacing.xl,
            &spacing.x2l,
            &spacing.x3l,
        ] {
            assert!(
                step.contains("var(--zui-space-base)"),
                "{step} does not follow the base"
            );
        }
    }
}
