//! The feature flags a style sheet has to be parsed with.
//!
//! Every one of these is read at *parse* time, so a sheet parsed while a flag is off silently
//! loses those declarations rather than reporting them. They are flipped once per process, before
//! anything is parsed.
//!
//! The badly named one is the "unimplemented layout" flag: the properties behind it parse, cascade
//! and inherit exactly like any other, and are unimplemented only in the sense that the engine's
//! own layout does not consume them. Layout here is this framework's, so they must be on.

use std::sync::Once;

use stylo_static_prefs::{pref, set_pref};

/// Expands one list of flag names into the code that sets them and the code that reads them back.
///
/// The engine matches a preference on the literal token of its name, so neither call can be handed
/// a name through a value or through a captured literal. A single token tree *can* be compared to
/// other tokens, which is what lets both halves come from one list — and one list cannot disagree
/// with itself, which is the failure this arrangement exists to make impossible: a flag set and
/// never read back is a flag nothing checks, and a flag read back and never set reports as off
/// forever.
macro_rules! feature_flags {
    ($($name:tt),+ $(,)?) => {
        /// How many flags [`enable_css_features`] turns on.
        pub const FEATURE_FLAG_COUNT: usize = [$($name),+].len();

        /// Turns on every CSS feature this framework targets, once per process.
        ///
        /// Idempotent and safe to call from anywhere, including from several threads at once.
        /// Anything that parses a style sheet calls it first; anything that only *reads* computed
        /// styles does not need to.
        pub fn enable_css_features() {
            static ONCE: Once = Once::new();
            ONCE.call_once(|| {
                $( set_pref!($name, true); )+
            });
        }

        /// Every flag [`enable_css_features`] sets, with the value it currently holds.
        ///
        /// Reading them back is what turns the bootstrap into something observable: a caller can
        /// assert that the flags were off beforehand and on afterwards, which is the difference
        /// between a start-up that works and one whose flags happened to default the right way.
        ///
        /// ```
        /// use zgui_css::prefs::{enable_css_features, feature_flags};
        ///
        /// enable_css_features();
        /// assert!(feature_flags().into_iter().all(|(_, on)| on));
        /// ```
        pub fn feature_flags() -> [(&'static str, bool); FEATURE_FLAG_COUNT] {
            [$( ($name, pref!($name)) ),+]
        }
    };
}

feature_flags! {
    "layout.grid.enabled",
    "layout.columns.enabled",
    "layout.container-queries.enabled",
    "layout.unimplemented",
    "layout.writing-mode.enabled",
    "layout.variable_fonts.enabled",
    "layout.css.at-scope.enabled",
    "layout.css.starting-style-at-rules.enabled",
    "layout.css.attr.enabled",
    "layout.css.custom-media.enabled",
    "layout.css.style-queries.enabled",
    "layout.css.font-palette.enabled",
    "layout.css.font-tech.enabled",
    "layout.css.basic-shape-shape.enabled",
    "layout.css.light-dark.images.enabled",
    "layout.css.content.alt-text.enabled",
    "layout.css.motion-path-url.enabled",
    "layout.css.anchor-positioning.enabled",
    "layout.css.scroll-driven-animations.enabled",
    "layout.css.scroll-state.enabled",
    "layout.css.appearance-base.enabled",
    "layout.css.margin-rules.enabled",
    "dom.select.customizable_select.enabled",
    "dom.viewTransitions.cross-document.enabled",
}
