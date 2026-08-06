//! Everything an interface uses tokens through, in one import.
//!
//! A component written with `view!` needs both the component and the props type its `#[component]`
//! attribute generated, because the macro names the second one to build the first. Importing them
//! in pairs by hand is a paper cut per component, so they are exported together here:
//!
//! ```
//! use zgui_ui_tokens::prelude::*;
//! ```
//!
//! Nothing here is exclusive to it — every name is reachable at its own path too.

pub use crate::provider::{ThemeContext, ThemeProvider, ThemeProviderProps, Themes, use_theme};
pub use crate::scheme::ColorScheme;
pub use crate::theme::{Preset, THEME_SHEET, Theme, theme_sheet};
pub use crate::token::{
    ColorTokens, Declarations, MotionTokens, RadiusTokens, ScaleTokens, ShadowTokens,
    SpacingTokens, TypeTokens,
};

/// The accessors the token stores derive, so that reading one group subscribes to that group
/// alone.
pub use crate::token::{
    ColorTokensStoreFields, MotionTokensStoreFields, RadiusTokensStoreFields,
    ScaleTokensStoreFields, ShadowTokensStoreFields, SpacingTokensStoreFields,
    TypeTokensStoreFields,
};

/// The same, for a whole theme and for the pair of sets a provider carries.
pub use crate::{ThemeStoreFields, ThemesStoreFields};
