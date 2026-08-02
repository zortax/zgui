//! The token groups a theme is made of, and how a group reaches a style sheet.
//!
//! Each group is a struct of CSS values with a light and a dark default, and each field lowers to
//! one custom property under `--zui-`. Nothing here is a colour type, a length type or a duration
//! type: a token *is* a CSS value, so anything the style engine can parse is expressible and this
//! crate never becomes the thing standing between an author and a value.
//!
//! | Group | Prefix | What it decides |
//! |---|---|---|
//! | [`ScaleTokens`] | `--zui-scale-` | two twelve-step colour ramps, for a graded series |
//! | [`ColorTokens`] | `--zui-color-` | what each part of an interface is coloured with |
//! | [`ControlTokens`] | `--zui-color-control-` | the few colours that are a different idea in each scheme |
//! | [`SpacingTokens`] | `--zui-space-` | how much room there is between things |
//! | [`RadiusTokens`] | `--zui-radius-` | how round a corner is |
//! | [`TypeTokens`] | `--zui-type-` | what text is set in |
//! | [`ShadowTokens`] | `--zui-shadow-` | how far off the page something sits |
//! | [`MotionTokens`] | `--zui-motion-` | how long a change takes, and how it is paced |

pub mod color;
pub mod control;
mod group;
pub mod motion;
pub mod radius;
pub mod scale;
pub mod shadow;
pub mod space;
pub mod typography;
mod value;

pub use crate::token::color::{ColorTokens, ColorTokensStoreFields};
pub use crate::token::control::{ControlTokens, ControlTokensStoreFields};
pub use crate::token::motion::{MotionTokens, MotionTokensStoreFields};
pub use crate::token::radius::{RadiusTokens, RadiusTokensStoreFields};
pub use crate::token::scale::{ScaleTokens, ScaleTokensStoreFields};
pub use crate::token::shadow::{ShadowTokens, ShadowTokensStoreFields};
pub use crate::token::space::{SpacingTokens, SpacingTokensStoreFields};
pub use crate::token::typography::{TypeTokens, TypeTokensStoreFields};
pub use crate::token::value::Declarations;
