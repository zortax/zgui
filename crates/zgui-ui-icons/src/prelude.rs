//! The icon component and its props, in one import.
//!
//! A component written with `view!` needs both the component and the props type its
//! `#[component]` attribute generated, because the macro names the second to build the first:
//!
//! ```
//! use zgui_ui_icons::prelude::*;
//! ```
//!
//! The icons themselves are deliberately absent. There are dozens of them, their names are short,
//! and importing all of them into an application's own namespace would shadow more than it helps —
//! so an application names the ones it draws, from [`set`](crate::set).

pub use crate::icon::{IconData, IconSize, IconVariants};
pub use crate::view::{Icon, IconProps, IconStyle};
