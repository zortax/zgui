//! The themes this library ships, as CSS.
//!
//! A theme is a set of CSS values, so a theme this crate ships is written the way an application
//! would write one: a block of custom-property declarations laid over the base token set. There is
//! no second mechanism here and no privileged one — [`Preset::light`] and [`Preset::dark`] are
//! `Theme::light().with_css(…)` and `Theme::dark().with_css(…)`, and an application's own theme is
//! the same call with its own text.
//!
//! # What a preset is allowed to change
//!
//! Anything, but each of these deliberately reaches more than one kind of decision: the accent it
//! is built around, how round a corner is, and how long a change takes. A theme that only re-tinted
//! would be a colour picker, and the point of these is to show that the token schema carries the
//! *feel* of an interface and not only its palette.
//!
//! Each one sets `--zui-radius-base` and the motion durations rather than the ladders above them,
//! because those two are the units their whole ladders are multiples of — one declaration squares
//! off or slows down every component at once.

use crate::theme::Theme;

/// A theme this library ships, for either slot of a [`ThemeProvider`](crate::ThemeProvider).
///
/// ```
/// use zgui_ui_tokens::{Preset, Theme};
///
/// let theme: Theme = Preset::Ember.light();
/// assert_eq!(theme.radius.base, "4px");
/// ```
///
/// The two slots are independent: an interface can be [`Preset::Base`] on a light desktop and
/// [`Preset::Midnight`] on a dark one, which is the arrangement most applications actually want.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
#[non_exhaustive]
pub enum Preset {
    /// The tokens the library ships with: a neutral, near-monochrome interface on soft corners.
    #[default]
    Base,
    /// Cool and blue, on wider corners and a slower, softer motion. The calmest of them.
    Ocean,
    /// Warm orange on tight corners, with the shortest motion in the set. Reads as brisk.
    Ember,
    /// Green, squared right off, with no motion at all — every transition is instant.
    ///
    /// Also what an interface looks like under a reduced-motion preference, which is why the
    /// durations are zeroed rather than merely shortened.
    Terminal,
    /// Violet on very round corners with a long, easy motion.
    Midnight,
}

impl Preset {
    /// Every preset, in the order they are offered.
    pub const ALL: &'static [Self] = &[
        Self::Base,
        Self::Ocean,
        Self::Ember,
        Self::Terminal,
        Self::Midnight,
    ];

    /// How this is written, for a chooser or a transcript.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Ocean => "ocean",
            Self::Ember => "ember",
            Self::Terminal => "terminal",
            Self::Midnight => "midnight",
        }
    }

    /// What it is called in an interface.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Base => "Base",
            Self::Ocean => "Ocean",
            Self::Ember => "Ember",
            Self::Terminal => "Terminal",
            Self::Midnight => "Midnight",
        }
    }

    /// The preset written as `name`, if there is one.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|preset| preset.name() == name)
    }

    /// The declarations this preset lays over the light token set.
    #[must_use]
    pub const fn light_css(self) -> &'static str {
        match self {
            Self::Base => "",
            Self::Ocean => OCEAN_LIGHT,
            Self::Ember => EMBER_LIGHT,
            Self::Terminal => TERMINAL_LIGHT,
            Self::Midnight => MIDNIGHT_LIGHT,
        }
    }

    /// The declarations it lays over the dark token set.
    #[must_use]
    pub const fn dark_css(self) -> &'static str {
        match self {
            Self::Base => "",
            Self::Ocean => OCEAN_DARK,
            Self::Ember => EMBER_DARK,
            Self::Terminal => TERMINAL_DARK,
            Self::Midnight => MIDNIGHT_DARK,
        }
    }

    /// This preset as the theme for a light surface.
    #[must_use]
    pub fn light(self) -> Theme {
        Theme::light().with_css(self.light_css())
    }

    /// This preset as the theme for a dark surface.
    #[must_use]
    pub fn dark(self) -> Theme {
        Theme::dark().with_css(self.dark_css())
    }
}

/// The shape every preset below shares, so that what differs between them is only the values.
///
/// Six colours, one radius and three durations. The colours are the ones an interface's character
/// actually lives in: what a primary control is filled with, what a hover or a highlight washes
/// with, what the focus ring is, and what the page and its cards are. Everything else in the schema
/// either resolves through one of these or is a decision no theme should be re-taking.
macro_rules! preset {
    (
        $name:ident,
        primary = $primary:literal, on_primary = $on_primary:literal,
        accent = $accent:literal, on_accent = $on_accent:literal,
        ring = $ring:literal, background = $background:literal, surface = $surface:literal,
        border = $border:literal, muted = $muted:literal, on_muted = $on_muted:literal,
        radius = $radius:literal,
        fast = $fast:literal, normal = $normal:literal, slow = $slow:literal, ease = $ease:literal,
    ) => {
        const $name: &str = concat!(
            "--zui-color-primary: ",
            $primary,
            ";",
            "--zui-color-primary-foreground: ",
            $on_primary,
            ";",
            "--zui-color-accent: ",
            $accent,
            ";",
            "--zui-color-accent-foreground: ",
            $on_accent,
            ";",
            "--zui-color-secondary: ",
            $accent,
            ";",
            "--zui-color-secondary-foreground: ",
            $on_accent,
            ";",
            "--zui-color-ring: ",
            $ring,
            ";",
            "--zui-color-background: ",
            $background,
            ";",
            "--zui-color-card: ",
            $surface,
            ";",
            "--zui-color-popover: ",
            $surface,
            ";",
            "--zui-color-sidebar: ",
            $surface,
            ";",
            "--zui-color-sidebar-primary: ",
            $primary,
            ";",
            "--zui-color-sidebar-primary-foreground: ",
            $on_primary,
            ";",
            "--zui-color-sidebar-accent: ",
            $accent,
            ";",
            "--zui-color-sidebar-accent-foreground: ",
            $on_accent,
            ";",
            "--zui-color-sidebar-border: ",
            $border,
            ";",
            "--zui-color-sidebar-ring: ",
            $ring,
            ";",
            "--zui-color-border: ",
            $border,
            ";",
            "--zui-color-input: ",
            $border,
            ";",
            "--zui-color-muted: ",
            $muted,
            ";",
            "--zui-color-muted-foreground: ",
            $on_muted,
            ";",
            "--zui-radius-base: ",
            $radius,
            ";",
            "--zui-motion-duration-fast: ",
            $fast,
            ";",
            "--zui-motion-duration-normal: ",
            $normal,
            ";",
            "--zui-motion-duration-slow: ",
            $slow,
            ";",
            "--zui-motion-ease-standard: ",
            $ease,
            ";",
        );
    };
}

preset!(
    OCEAN_LIGHT,
    primary = "oklch(0.55 0.16 245)",
    on_primary = "oklch(0.99 0.01 245)",
    accent = "oklch(0.95 0.03 245)",
    on_accent = "oklch(0.38 0.12 245)",
    ring = "oklch(0.62 0.14 245)",
    background = "oklch(0.99 0.005 245)",
    surface = "oklch(1 0 0)",
    border = "oklch(0.90 0.02 245)",
    muted = "oklch(0.96 0.015 245)",
    on_muted = "oklch(0.52 0.04 245)",
    radius = "14px",
    fast = "120ms",
    normal = "200ms",
    slow = "280ms",
    ease = "cubic-bezier(0.33, 1, 0.68, 1)",
);

preset!(
    OCEAN_DARK,
    primary = "oklch(0.68 0.15 245)",
    on_primary = "oklch(0.16 0.03 245)",
    accent = "oklch(0.30 0.05 245)",
    on_accent = "oklch(0.93 0.03 245)",
    ring = "oklch(0.58 0.12 245)",
    background = "oklch(0.17 0.02 245)",
    surface = "oklch(0.22 0.025 245)",
    border = "oklch(1 0 0 / 12%)",
    muted = "oklch(0.28 0.03 245)",
    on_muted = "oklch(0.75 0.03 245)",
    radius = "14px",
    fast = "120ms",
    normal = "200ms",
    slow = "280ms",
    ease = "cubic-bezier(0.33, 1, 0.68, 1)",
);

preset!(
    EMBER_LIGHT,
    primary = "oklch(0.62 0.19 42)",
    on_primary = "oklch(0.99 0.01 42)",
    accent = "oklch(0.95 0.04 60)",
    on_accent = "oklch(0.42 0.14 42)",
    ring = "oklch(0.68 0.17 42)",
    background = "oklch(0.99 0.008 70)",
    surface = "oklch(1 0 0)",
    border = "oklch(0.90 0.025 60)",
    muted = "oklch(0.96 0.02 60)",
    on_muted = "oklch(0.52 0.05 50)",
    radius = "4px",
    fast = "60ms",
    normal = "90ms",
    slow = "130ms",
    ease = "cubic-bezier(0.2, 0, 0, 1)",
);

preset!(
    EMBER_DARK,
    primary = "oklch(0.70 0.18 45)",
    on_primary = "oklch(0.18 0.04 45)",
    accent = "oklch(0.32 0.06 45)",
    on_accent = "oklch(0.93 0.04 60)",
    ring = "oklch(0.62 0.16 45)",
    background = "oklch(0.16 0.015 50)",
    surface = "oklch(0.21 0.02 50)",
    border = "oklch(1 0 0 / 12%)",
    muted = "oklch(0.27 0.03 50)",
    on_muted = "oklch(0.76 0.04 60)",
    radius = "4px",
    fast = "60ms",
    normal = "90ms",
    slow = "130ms",
    ease = "cubic-bezier(0.2, 0, 0, 1)",
);

preset!(
    TERMINAL_LIGHT,
    primary = "oklch(0.48 0.13 150)",
    on_primary = "oklch(0.99 0.01 150)",
    accent = "oklch(0.93 0.05 150)",
    on_accent = "oklch(0.34 0.10 150)",
    ring = "oklch(0.55 0.12 150)",
    background = "oklch(0.98 0.008 150)",
    surface = "oklch(1 0 0)",
    border = "oklch(0.86 0.02 150)",
    muted = "oklch(0.95 0.015 150)",
    on_muted = "oklch(0.48 0.04 150)",
    radius = "0px",
    fast = "0ms",
    normal = "0ms",
    slow = "0ms",
    ease = "linear",
);

preset!(
    TERMINAL_DARK,
    primary = "oklch(0.72 0.17 150)",
    on_primary = "oklch(0.16 0.03 150)",
    accent = "oklch(0.28 0.05 150)",
    on_accent = "oklch(0.90 0.10 150)",
    ring = "oklch(0.60 0.14 150)",
    background = "oklch(0.15 0.015 150)",
    surface = "oklch(0.19 0.02 150)",
    border = "oklch(0.72 0.17 150 / 25%)",
    muted = "oklch(0.24 0.03 150)",
    on_muted = "oklch(0.74 0.06 150)",
    radius = "0px",
    fast = "0ms",
    normal = "0ms",
    slow = "0ms",
    ease = "linear",
);

preset!(
    MIDNIGHT_LIGHT,
    primary = "oklch(0.52 0.20 295)",
    on_primary = "oklch(0.99 0.01 295)",
    accent = "oklch(0.95 0.035 295)",
    on_accent = "oklch(0.40 0.15 295)",
    ring = "oklch(0.60 0.17 295)",
    background = "oklch(0.99 0.006 295)",
    surface = "oklch(1 0 0)",
    border = "oklch(0.91 0.02 295)",
    muted = "oklch(0.96 0.018 295)",
    on_muted = "oklch(0.52 0.05 295)",
    radius = "20px",
    fast = "140ms",
    normal = "240ms",
    slow = "340ms",
    ease = "cubic-bezier(0.16, 1, 0.3, 1)",
);

preset!(
    MIDNIGHT_DARK,
    primary = "oklch(0.70 0.17 295)",
    on_primary = "oklch(0.17 0.04 295)",
    accent = "oklch(0.31 0.06 295)",
    on_accent = "oklch(0.93 0.04 295)",
    ring = "oklch(0.60 0.15 295)",
    background = "oklch(0.15 0.025 295)",
    surface = "oklch(0.20 0.03 295)",
    border = "oklch(1 0 0 / 12%)",
    muted = "oklch(0.26 0.04 295)",
    on_muted = "oklch(0.77 0.04 295)",
    radius = "20px",
    fast = "140ms",
    normal = "240ms",
    slow = "340ms",
    ease = "cubic-bezier(0.16, 1, 0.3, 1)",
);

#[cfg(test)]
mod tests {
    use super::Preset;
    use crate::Theme;

    #[test]
    fn the_base_preset_is_the_token_set_itself() {
        assert_eq!(Preset::Base.light(), Theme::light());
        assert_eq!(Preset::Base.dark(), Theme::dark());
    }

    #[test]
    fn every_other_preset_changes_colour_corners_and_motion() {
        for preset in Preset::ALL.iter().copied().filter(|p| *p != Preset::Base) {
            for (theme, base) in [
                (preset.light(), Theme::light()),
                (preset.dark(), Theme::dark()),
            ] {
                assert_ne!(
                    theme.color.primary,
                    base.color.primary,
                    "{} left the primary colour alone",
                    preset.name()
                );
                assert_ne!(
                    theme.radius.base,
                    base.radius.base,
                    "{} left the corners alone",
                    preset.name()
                );
                assert_ne!(
                    theme.motion.duration_normal,
                    base.motion.duration_normal,
                    "{} left the motion alone",
                    preset.name()
                );
            }
        }
    }

    #[test]
    fn every_declaration_a_preset_writes_names_a_token_that_exists() {
        // A preset with a typo in a property name is a preset that silently does less than it
        // says, and nothing else in the crate would notice: an unknown declaration is skipped.
        for preset in Preset::ALL.iter().copied() {
            for (scheme, css) in [("light", preset.light_css()), ("dark", preset.dark_css())] {
                let written = css.matches("--zui-").count();
                let mut theme = Theme::light();
                assert_eq!(
                    theme.apply_css(css),
                    written,
                    "{}'s {scheme} block writes a property this schema has no token for",
                    preset.name()
                );
            }
        }
    }

    #[test]
    fn a_preset_is_named_by_the_name_it_answers_to() {
        for preset in Preset::ALL.iter().copied() {
            assert_eq!(Preset::from_name(preset.name()), Some(preset));
        }
        assert_eq!(Preset::from_name("nothing"), None);
    }

    #[test]
    fn no_two_presets_share_a_name() {
        let mut names: Vec<&str> = Preset::ALL.iter().map(|preset| preset.name()).collect();
        let offered = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), offered);
    }
}
