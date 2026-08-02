//! The colours a control takes that are not the same idea in both schemes.
//!
//! Most of an interface's colour is one semantic token whose value happens to differ between the
//! two schemes — [`ColorTokens::border`](crate::ColorTokens) is a pale grey in one and a
//! translucent white in the other, but it is *the border* in both, and a sheet says
//! `var(--zui-color-border)` once. This group holds the handful of places where that is not true:
//! where a control is filled in one scheme and unfilled in the other, or takes a different token
//! entirely rather than a different value of the same one.
//!
//! # Why these are tokens rather than a media query
//!
//! Because a media query asks the *desktop*, and which scheme an interface is in is the
//! application's answer, not the desktop's. An interface pinned to
//! [`ColorScheme::Dark`](crate::ColorScheme) on a light desktop resolves every token to its dark
//! value — the theme sheet writes them out unconditionally — while a
//! `@media (prefers-color-scheme: dark)` block in a component's own sheet would not match at all.
//! A control styled that way comes out half dark: dark surface, light frame.
//!
//! So the difference is put where the scheme is already known. Each of these is one name a sheet
//! reads unconditionally, and the theme decides what it holds.

use crate::token::group::group;

group! {
    /// The colours that differ between the schemes by more than a value.
    ControlTokens, prefix = "color-control", {
        /// What a text field, a chooser or a checkbox is filled with at rest.
        ///
        /// Nothing in the light scheme — the field is the page, held in by its border. A dark
        /// interface has too little contrast between a border and the surface behind it for that
        /// to read, so the field is washed slightly lighter than what it sits on.
        field => "field", light = "transparent",
            dark = "color-mix(in oklab, var(--zui-color-input) 30%, transparent)";
        /// The same, under the pointer.
        field_hover => "field-hover", light = "transparent",
            dark = "color-mix(in oklab, var(--zui-color-input) 50%, transparent)";
        /// The halo around a control the keyboard is on that has something wrong with it.
        ///
        /// Weaker than the ordinary focus ring in either scheme, and stronger in the dark one,
        /// where a wash of red over a dark surface all but disappears.
        ring_invalid => "ring-invalid",
            light = "color-mix(in oklab, var(--zui-color-destructive) 20%, transparent)",
            dark = "color-mix(in oklab, var(--zui-color-destructive) 40%, transparent)";
        /// What a control that destroys something is filled with.
        ///
        /// The full red on a light page; softened on a dark one, where the same red against a
        /// near-black surface reads as a warning light rather than as a button.
        destructive_fill => "destructive-fill", light = "var(--zui-color-destructive)",
            dark = "color-mix(in oklab, var(--zui-color-destructive) 60%, transparent)";
        /// What a control with no frame of its own is filled with under the pointer.
        ghost_hover => "ghost-hover", light = "var(--zui-color-accent)",
            dark = "color-mix(in oklab, var(--zui-color-accent) 50%, transparent)";
        /// The border of a control that carries one but no fill.
        outline_border => "outline-border", light = "var(--zui-color-border)",
            dark = "var(--zui-color-input)";
        /// What that control is filled with.
        ///
        /// The page's own colour rather than nothing, so an outlined control keeps its own surface
        /// when it is standing on a card or in a filled strip.
        outline_fill => "outline-fill", light = "var(--zui-color-background)",
            dark = "color-mix(in oklab, var(--zui-color-input) 30%, transparent)";
        /// The same, under the pointer.
        outline_hover => "outline-hover", light = "var(--zui-color-accent)",
            dark = "color-mix(in oklab, var(--zui-color-input) 50%, transparent)";

        /// The track of a switch that is off.
        switch_off => "switch-off", light = "var(--zui-color-input)",
            dark = "color-mix(in oklab, var(--zui-color-input) 80%, transparent)";
        /// The thumb of a switch that is off.
        switch_thumb => "switch-thumb", light = "var(--zui-color-background)",
            dark = "var(--zui-color-foreground)";
        /// The thumb of a switch that is on.
        switch_thumb_on => "switch-thumb-on", light = "var(--zui-color-background)",
            dark = "var(--zui-color-primary-foreground)";

        /// The writing on a tab that is not the live one.
        ///
        /// Dimmed by transparency on a light strip and by a token on a dark one, because a
        /// translucent dark grey over a dark strip is not dimmer, only muddier.
        tab_ink => "tab-ink",
            light = "color-mix(in oklab, var(--zui-color-foreground) 60%, transparent)",
            dark = "var(--zui-color-muted-foreground)";
        /// What the live tab's own pill is filled with.
        tab_fill => "tab-fill", light = "var(--zui-color-background)",
            dark = "color-mix(in oklab, var(--zui-color-input) 30%, transparent)";
        /// The edge of that pill, which only a dark strip needs.
        tab_border => "tab-border", light = "transparent", dark = "var(--zui-color-input)";
    }
}

#[cfg(test)]
mod tests {
    use super::ControlTokens;

    #[test]
    fn every_one_of_them_earns_its_place_by_differing_between_the_schemes() {
        // The whole reason this group exists is that these cannot be one value. A token whose two
        // schemes agree belongs in `ColorTokens` beside the rest of the palette, where a sheet
        // will find it — so one that crept in here is a name in the wrong place.
        let light = ControlTokens::light();
        let dark = ControlTokens::dark();
        assert_ne!(light, dark);
        for ((name, lit), (_, dim)) in light.pairs().iter().zip(dark.pairs()) {
            assert_ne!(*lit, dim, "{name} is the same in both schemes");
        }
    }

    #[test]
    fn nothing_here_is_a_colour_of_its_own() {
        // Every one of these is the palette seen differently, not a thirteenth grey. A literal
        // colour here would be one the palette cannot be re-themed through.
        for tokens in [ControlTokens::light(), ControlTokens::dark()] {
            for (name, value) in tokens.pairs() {
                assert!(
                    value == "transparent"
                        || value.starts_with("var(--zui-color-")
                        || value.contains("var(--zui-color-"),
                    "{name} is {value}, which does not resolve through the palette"
                );
            }
        }
    }
}
