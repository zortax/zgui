//! The design tokens a component library is built out of: colour, spacing, radii, type, elevation
//! and motion, as CSS custom properties an application can override without rebuilding anything.
//!
//! A token is a name for a decision. `--zui-color-primary` is *the colour of the control that
//! carries the main action*, and a component that names it needs to know nothing else — not which
//! colour it is, not whether the interface is light or dark, and not what the application did to
//! it. That indirection is the whole point: it is what lets a library ship one set of rules and an
//! application re-theme every component in it with a style sheet.
//!
//! ```no_run
//! use zgui::prelude::*;
//! use zgui::{component, css, view};
//! use zgui_ui_tokens::prelude::*;
//!
//! /// A card, written entirely in tokens.
//! #[component]
//! fn Card(children: Children) -> impl IntoView {
//!     view! { box(class = "card") {{children.into_view_once()}} }
//! }
//!
//! const SHEET: &str = css!(
//!     ".card {
//!         background-color: var(--zui-color-card);
//!         color: var(--zui-color-card-foreground);
//!         border: 1px solid var(--zui-color-border);
//!         border-radius: var(--zui-radius-lg);
//!         padding: var(--zui-space-lg);
//!         box-shadow: var(--zui-shadow-sm);
//!     }"
//! );
//!
//! fn main() -> Result<(), zgui::Error> {
//!     app().with_stylesheet(SHEET).run(|| {
//!         view! { ThemeProvider(scheme = ColorScheme::System) {Card {"hello"}} }
//!     })
//! }
//! ```
//!
//! # How a theme reaches a document
//!
//! [`ThemeProvider`] writes the tokens out as a **style sheet at the author origin**, not as
//! inline properties on an element. That decision carries the crate:
//!
//! * an application overrides any token by writing an ordinary rule — inline properties would beat
//!   every such rule and the library would be un-themeable except by rebuilding it;
//! * the outermost provider declares on `:root`, so a menu portalled onto an overlay band is
//!   themed even though it is nowhere near the provider in the tree;
//! * [`ColorScheme::System`] costs nothing, because the dark tokens go inside
//!   `@media (prefers-color-scheme: dark)` and the desktop's own setting is what decides.
//!
//! # Overriding a token
//!
//! Every token is a custom property, and an application overrides one by writing an ordinary rule.
//! Three of them carry most of the interface's character:
//!
//! ```
//! use zgui::css;
//!
//! const BRAND: &str = css!(
//!     ":root {
//!         --zui-color-primary: oklch(0.55 0.22 264);
//!         --zui-radius-base: 4px;
//!         --zui-space-base: 3px;
//!     }"
//! );
//! assert!(BRAND.contains("--zui-color-primary"));
//! ```
//!
//! `--zui-radius-base` and `--zui-space-base` are each the unit their whole ladder is a multiple
//! of, so one declaration squares off or tightens up every component at once. Colour has no such
//! single knob — the semantic colours hold measured values rather than steps of a ramp, for the
//! reasons in [`token::color`], and re-tinting an interface is the handful of declarations listed
//! there.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`token`] | the seven token groups, and [`Declarations`] |
//! | [`theme`] | [`Theme`], and [`theme_sheet`] — a theme as the text of a style sheet |
//! | [`scheme`] | [`ColorScheme`] |
//! | [`provider`] | [`ThemeProvider`], [`ThemeContext`] and [`use_theme`] |
//! | [`prelude`] | all of the above, in one import |

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod prelude;
pub mod provider;
pub mod scheme;
pub mod theme;
pub mod token;

pub use crate::provider::{
    ThemeContext, ThemeProvider, ThemeProviderProps, Themes, ThemesStoreFields, use_theme,
};
pub use crate::scheme::ColorScheme;
pub use crate::theme::{THEME_SHEET, Theme, ThemeStoreFields, theme_sheet};
pub use crate::token::{
    ColorTokens, ControlTokens, Declarations, MotionTokens, RadiusTokens, ScaleTokens,
    ShadowTokens, SpacingTokens, TypeTokens,
};

/// The accessors a store's derive generates, one trait per token group.
///
/// They are traits, so `theme.color()` does not resolve without the trait in scope — and a
/// consumer should not have to know that the accessor came from a derive in order to import it.
pub use crate::token::{
    ColorTokensStoreFields, ControlTokensStoreFields, MotionTokensStoreFields,
    RadiusTokensStoreFields, ScaleTokensStoreFields, ShadowTokensStoreFields,
    SpacingTokensStoreFields, TypeTokensStoreFields,
};
