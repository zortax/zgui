//! What each part of an interface is coloured with.
//!
//! These are the tokens a component names. A component asks for *the colour of the control that
//! carries the main action* and gets it; it never learns which colour that is, nor whether the
//! interface is light or dark.
//!
//! # Why these are colours and not ramp steps
//!
//! The [ramps](crate::ScaleTokens) still exist and still mean what they meant, but the semantic
//! tokens no longer resolve through them. They hold measured values instead, because the palette
//! this library is cut to does not form an even twelve-step ramp — its solid primary fill is
//! near-black in light and near-white in dark, its muted surface and its accent surface are the
//! same colour, and its two borders match no ramp step. Derived values would land *near* those
//! colours, and a colour that is nearly right is the one thing a design system cannot ship.
//!
//! # What an application overrides to re-tint an interface
//!
//! **The semantic tokens themselves.** Before, one declaration of `--zui-scale-accent-9` re-tinted
//! everything; now the same job is a handful of declarations naming what each one is *for*:
//!
//! ```
//! use zgui::css;
//!
//! const BRAND: &str = css!(
//!     ":root {
//!         --zui-color-primary: oklch(0.55 0.22 264);
//!         --zui-color-primary-foreground: oklch(0.985 0 0);
//!         --zui-color-accent: oklch(0.95 0.03 264);
//!         --zui-color-accent-foreground: oklch(0.4 0.16 264);
//!         --zui-color-ring: oklch(0.55 0.22 264);
//!     }"
//! );
//! assert!(BRAND.contains("--zui-color-primary"));
//! ```
//!
//! Those five are the whole tint: `primary` and its foreground are every solid call to action,
//! `accent` and its foreground are every hovered row and highlighted menu item, and `ring` is the
//! focus ring around all of it. A dark scheme overrides the same five inside
//! `@media (prefers-color-scheme: dark)`, or under whatever selector
//! [`ColorScheme`](crate::ColorScheme) put the dark set behind.
//!
//! Colours are written in `oklch()`, which is perceptually uniform: holding the first number and
//! moving the last two re-tints without changing how light anything looks.

use crate::token::group::group;

group! {
    /// The colour each part of an interface takes.
    ColorTokens, prefix = "color", {
        /// Behind everything.
        background => "background", light = "oklch(1 0 0)", dark = "oklch(0.145 0 0)";
        /// Ordinary text.
        foreground => "foreground", light = "oklch(0.145 0 0)", dark = "oklch(0.985 0 0)";
        /// A card's surface.
        card => "card", light = "oklch(1 0 0)", dark = "oklch(0.205 0 0)";
        /// Text on a card.
        card_foreground => "card-foreground", light = "oklch(0.145 0 0)",
            dark = "oklch(0.985 0 0)";
        /// A popover, menu or tooltip surface.
        popover => "popover", light = "oklch(1 0 0)", dark = "oklch(0.205 0 0)";
        /// Text on a popover.
        popover_foreground => "popover-foreground", light = "oklch(0.145 0 0)",
            dark = "oklch(0.985 0 0)";
        /// The solid fill of the control that carries the main action.
        primary => "primary", light = "oklch(0.205 0 0)", dark = "oklch(0.922 0 0)";
        /// Text on that fill.
        primary_foreground => "primary-foreground", light = "oklch(0.985 0 0)",
            dark = "oklch(0.205 0 0)";
        /// The fill of a control beside the main one.
        secondary => "secondary", light = "oklch(0.97 0 0)", dark = "oklch(0.269 0 0)";
        /// Text on a secondary fill.
        secondary_foreground => "secondary-foreground", light = "oklch(0.205 0 0)",
            dark = "oklch(0.985 0 0)";
        /// A surface that recedes.
        muted => "muted", light = "oklch(0.97 0 0)", dark = "oklch(0.269 0 0)";
        /// Text that recedes: a hint, a timestamp, a placeholder.
        muted_foreground => "muted-foreground", light = "oklch(0.556 0 0)",
            dark = "oklch(0.708 0 0)";
        /// What a highlighted row or a hovered menu item is filled with.
        accent => "accent", light = "oklch(0.97 0 0)", dark = "oklch(0.269 0 0)";
        /// Text on that highlight.
        accent_foreground => "accent-foreground", light = "oklch(0.205 0 0)",
            dark = "oklch(0.985 0 0)";
        /// The fill, text and border of anything that destroys something.
        destructive => "destructive", light = "oklch(0.577 0.245 27.325)",
            dark = "oklch(0.704 0.191 22.216)";
        /// Text on a destructive fill.
        destructive_foreground => "destructive-foreground", light = "oklch(0.97 0.01 17)",
            dark = "oklch(0.58 0.22 27)";
        /// Text and icons of something that has gone wrong, on an ordinary background.
        ///
        /// The same colour as the destructive fill, named apart because the two are used in
        /// different places and an application may want the message redder than the button.
        danger => "danger", light = "var(--zui-color-destructive)",
            dark = "var(--zui-color-destructive)";
        /// An ordinary border.
        border => "border", light = "oklch(0.922 0 0)", dark = "oklch(1 0 0 / 10%)";
        /// The border of something the user types into.
        input => "input", light = "oklch(0.922 0 0)", dark = "oklch(1 0 0 / 15%)";
        /// The ring drawn around whatever the keyboard is on.
        ring => "ring", light = "oklch(0.708 0 0)", dark = "oklch(0.556 0 0)";
        /// What dims the interface behind a modal surface.
        scrim => "scrim", light = "oklch(0 0 0 / 50%)", dark = "oklch(0 0 0 / 50%)";

        /// The first series of a chart.
        chart_1 => "chart-1", light = "oklch(0.87 0 0)", dark = "oklch(0.87 0 0)";
        /// The second series.
        chart_2 => "chart-2", light = "oklch(0.556 0 0)", dark = "oklch(0.556 0 0)";
        /// The third series.
        chart_3 => "chart-3", light = "oklch(0.439 0 0)", dark = "oklch(0.439 0 0)";
        /// The fourth series.
        chart_4 => "chart-4", light = "oklch(0.371 0 0)", dark = "oklch(0.371 0 0)";
        /// The fifth series.
        chart_5 => "chart-5", light = "oklch(0.269 0 0)", dark = "oklch(0.269 0 0)";

        /// The sidebar's own surface, which sits beside the page rather than on it.
        sidebar => "sidebar", light = "oklch(0.985 0 0)", dark = "oklch(0.205 0 0)";
        /// Text in the sidebar.
        sidebar_foreground => "sidebar-foreground", light = "oklch(0.145 0 0)",
            dark = "oklch(0.985 0 0)";
        /// The fill of the sidebar's current item.
        sidebar_primary => "sidebar-primary", light = "oklch(0.205 0 0)",
            dark = "oklch(0.488 0.243 264.376)";
        /// Text on that fill.
        sidebar_primary_foreground => "sidebar-primary-foreground", light = "oklch(0.985 0 0)",
            dark = "oklch(0.985 0 0)";
        /// What a hovered sidebar item is filled with.
        sidebar_accent => "sidebar-accent", light = "oklch(0.97 0 0)", dark = "oklch(0.269 0 0)";
        /// Text on that highlight.
        sidebar_accent_foreground => "sidebar-accent-foreground", light = "oklch(0.205 0 0)",
            dark = "oklch(0.985 0 0)";
        /// A border inside the sidebar.
        sidebar_border => "sidebar-border", light = "oklch(0.922 0 0)",
            dark = "oklch(1 0 0 / 10%)";
        /// The focus ring inside the sidebar.
        sidebar_ring => "sidebar-ring", light = "oklch(0.708 0 0)", dark = "oklch(0.556 0 0)";
    }
}

#[cfg(test)]
mod tests {
    use super::ColorTokens;

    #[test]
    fn the_two_schemes_are_two_sets_of_colours_rather_than_one_set_over_two_ramps() {
        // The property this test protects, stated as the thing that changed: a semantic token now
        // holds a colour, so the dark scheme is a second colour and not the same `var()` over a
        // flipped ramp. Anything still written as a `var()` is deliberate indirection, and there
        // is exactly one of those.
        let light = ColorTokens::light();
        let indirect: Vec<&'static str> = light
            .pairs()
            .iter()
            .filter(|(_, value)| value.starts_with("var("))
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(indirect, ["--zui-color-danger"]);
    }

    #[test]
    fn every_colour_is_written_in_the_perceptual_space_the_palette_was_measured_in() {
        // A converted colour is a colour that is nearly right, and nearly right is what this
        // library is trying not to be. Anything not in `oklch()` is either the one alias above or
        // a conversion that crept in.
        for (name, value) in ColorTokens::light().pairs() {
            assert!(
                value.starts_with("oklch(") || value.starts_with("var("),
                "{name} is {value}, which is not the space the palette was measured in"
            );
        }
    }

    #[test]
    fn the_sidebar_carries_a_whole_palette_of_its_own() {
        // It sits beside the page rather than on it, so it needs its own surface, text, highlight,
        // border and ring — not a tint of the page's.
        let sidebar = ColorTokens::PROPERTIES
            .iter()
            .filter(|name| name.contains("sidebar"))
            .count();
        assert_eq!(sidebar, 8);
    }

    #[test]
    fn a_chart_series_keeps_its_colour_when_the_scheme_flips() {
        // A series is identified by its colour across a legend, a tooltip and a printout. One that
        // changed with the desktop's setting would not identify anything.
        let light = ColorTokens::light();
        let dark = ColorTokens::dark();
        assert_eq!(light.chart_1, dark.chart_1);
        assert_eq!(light.chart_5, dark.chart_5);
    }
}
