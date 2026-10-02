//! The application menu a desktop shows outside the windows.
//!
//! macOS shows one menu bar for the whole application. Other desktops have no such place, and a
//! backend without one ignores the menu. [`PlatformCapabilities::app_menu`] says which applies.
//!
//! [`PlatformCapabilities::app_menu`]: crate::PlatformCapabilities::app_menu

mod model;
mod role;
mod shortcut;

pub use crate::menu::model::{AppMenu, MenuAction, MenuEntry, MenuId, Submenu};
pub use crate::menu::role::MenuRole;
pub use crate::menu::shortcut::Shortcut;

#[cfg(test)]
mod tests;
