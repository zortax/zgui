//! The whole token schema, in one value.

mod css;
mod preset;
mod sheet;

use zgui::reactive::store::reactive_stores;
use zgui::reactive::{Patch, Store};

use crate::token::{
    ColorTokens, ControlTokens, MotionTokens, RadiusTokens, ScaleTokens, ShadowTokens,
    SpacingTokens, TypeTokens,
};

pub use crate::theme::preset::Preset;
pub use crate::theme::sheet::{THEME_SHEET, theme_sheet};

/// Everything an interface's appearance is decided by, in one value.
///
/// A theme is data. It has no behaviour, it reaches the document as custom properties, and every
/// one of those properties is overridable by an application's own style sheet — so an application
/// re-themes a component library by writing CSS, not by rebuilding it.
///
/// # Two schemes
///
/// [`Theme::light`] and [`Theme::dark`] are the two token sets. Which one an interface is in is
/// **not** part of the theme: a theme carries both, and
/// [`ThemeProvider`](crate::ThemeProvider) writes whichever the
/// [`ColorScheme`](crate::ColorScheme) asks for — including the one that asks the desktop.
///
/// # Changing it at run time
///
/// Held in a store, so that reading one group subscribes to that group alone and applying a whole
/// new theme wakes only what actually changed:
///
/// ```
/// use zgui::reactive::prelude::*;
/// use zgui::reactive::{Patch, Store, install};
/// use zgui_ui_tokens::{Theme, ThemeStoreFields};
///
/// install().ok();
/// let theme = Store::new(Theme::light());
///
/// let mut brighter = Theme::light();
/// brighter.scale.accent_9 = "rebeccapurple".to_owned();
/// theme.patch(brighter);
///
/// assert_eq!(theme.scale().get().accent_9, "rebeccapurple");
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Store, Patch)]
pub struct Theme {
    /// The two twelve-step ramps every semantic colour resolves through.
    pub scale: ScaleTokens,
    /// What each part of the interface is coloured with.
    pub color: ColorTokens,
    /// The few colours a control takes that are a different idea in each scheme.
    pub control: ControlTokens,
    /// How much room there is between things.
    pub space: SpacingTokens,
    /// How round a corner is.
    pub radius: RadiusTokens,
    /// What text is set in.
    pub typography: TypeTokens,
    /// How far off the page something sits.
    pub shadow: ShadowTokens,
    /// How long a change takes, and how it is paced.
    pub motion: MotionTokens,
}

impl Theme {
    /// The light token set.
    pub fn light() -> Self {
        Self {
            scale: ScaleTokens::light(),
            color: ColorTokens::light(),
            control: ControlTokens::light(),
            space: SpacingTokens::light(),
            radius: RadiusTokens::light(),
            typography: TypeTokens::light(),
            shadow: ShadowTokens::light(),
            motion: MotionTokens::light(),
        }
    }

    /// The dark token set.
    pub fn dark() -> Self {
        Self {
            scale: ScaleTokens::dark(),
            color: ColorTokens::dark(),
            control: ControlTokens::dark(),
            space: SpacingTokens::dark(),
            radius: RadiusTokens::dark(),
            typography: TypeTokens::dark(),
            shadow: ShadowTokens::dark(),
            motion: MotionTokens::dark(),
        }
    }

    /// Every custom property a theme lowers to, in the order it is written.
    ///
    /// The list an application checks its own overrides against, and the list a documentation
    /// page is generated from.
    pub fn properties() -> Vec<&'static str> {
        let mut names = Vec::new();
        names.extend_from_slice(ScaleTokens::PROPERTIES);
        names.extend_from_slice(ColorTokens::PROPERTIES);
        names.extend_from_slice(ControlTokens::PROPERTIES);
        names.extend_from_slice(SpacingTokens::PROPERTIES);
        names.extend_from_slice(RadiusTokens::PROPERTIES);
        names.extend_from_slice(TypeTokens::PROPERTIES);
        names.extend_from_slice(ShadowTokens::PROPERTIES);
        names.extend_from_slice(MotionTokens::PROPERTIES);
        names
    }

    /// Sets one token by the custom property it is written as, and answers whether it exists.
    ///
    /// The inverse of [`declare`](Self::declare), one token at a time.
    pub fn set(&mut self, property: &str, value: &str) -> bool {
        self.scale.set(property, value)
            || self.color.set(property, value)
            || self.control.set(property, value)
            || self.space.set(property, value)
            || self.radius.set(property, value)
            || self.typography.set(property, value)
            || self.shadow.set(property, value)
            || self.motion.set(property, value)
    }

    /// Lays a block of custom-property declarations over this theme, and hands it back.
    ///
    /// A theme is a set of CSS values, so the pleasant way to write one is as CSS — the same text
    /// that would go in a style sheet, without having to know which Rust field holds which
    /// property:
    ///
    /// ```
    /// use zgui_ui_tokens::Theme;
    ///
    /// let warm = Theme::light().with_css(
    ///     "--zui-color-primary: oklch(0.62 0.18 40);
    ///      --zui-radius-base: 4px;
    ///      --zui-motion-duration-normal: 90ms;",
    /// );
    /// assert_eq!(warm.radius.base, "4px");
    /// ```
    ///
    /// Writing the same declarations in an application's own style sheet does the same thing to a
    /// whole window, and needs none of this. Coming through the theme is what makes it a *value*:
    /// something a provider can hold in one of its two slots, that a second provider can put a
    /// different one of in its own subtree, and that a control can switch between at run time.
    ///
    /// # What it does with what it cannot use
    ///
    /// Ignores it, declaration by declaration, exactly as a style sheet does — a name this schema
    /// has never heard of leaves the rest of the block standing. `/* comments */` are skipped, a
    /// wrapping `:root { … }` or `.theme { … }` is tolerated, and a trailing semicolon is optional.
    #[must_use]
    pub fn with_css(mut self, declarations: &str) -> Self {
        self.apply_css(declarations);
        self
    }

    /// The same, in place, answering how many declarations named a token this schema has.
    pub fn apply_css(&mut self, declarations: &str) -> usize {
        let mut applied = 0;
        for declaration in crate::theme::css::declarations(declarations) {
            if self.set(declaration.property, declaration.value) {
                applied += 1;
            }
        }
        applied
    }

    /// Writes every token in the theme as a custom-property declaration.
    pub fn declare(&self, out: &mut crate::Declarations) {
        self.scale.declare(out);
        self.color.declare(out);
        self.control.declare(out);
        self.space.declare(out);
        self.radius.declare(out);
        self.typography.declare(out);
        self.shadow.declare(out);
        self.motion.declare(out);
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

#[cfg(test)]
mod tests {
    use super::Theme;
    use crate::Declarations;

    #[test]
    fn no_two_tokens_share_a_custom_property() {
        let mut names = Theme::properties();
        let declared = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), declared);
    }

    #[test]
    fn every_property_is_under_the_one_prefix() {
        for name in Theme::properties() {
            assert!(
                name.starts_with("--zui-"),
                "{name} is outside the namespace"
            );
        }
    }

    #[test]
    fn lowering_writes_every_token_and_only_the_tokens() {
        let mut declarations = Declarations::new();
        Theme::light().declare(&mut declarations);
        assert_eq!(declarations.len(), Theme::properties().len());
    }

    #[test]
    fn the_two_schemes_are_different_themes_over_the_same_schema() {
        assert_ne!(Theme::light(), Theme::dark());
        let light: Vec<&str> = Theme::properties();
        let mut dark_declarations = Declarations::new();
        Theme::dark().declare(&mut dark_declarations);
        for name in light {
            assert!(dark_declarations.as_str().contains(name));
        }
    }
}
