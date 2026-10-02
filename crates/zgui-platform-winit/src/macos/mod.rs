//! The AppKit objects winit does not expose: the application menu, the reopen request, the
//! title-bar buttons and the title-bar double-click preference.
//!
//! Every function here runs on the main thread, inside a callback of the event loop.

mod buttons;
mod double_click;
mod menu;
mod reopen;
mod window;

pub(crate) use crate::macos::buttons::place as place_title_buttons;
pub(crate) use crate::macos::double_click::perform as title_bar_double_click;
pub(crate) use crate::macos::menu::{Menus, install as install_menu};
pub(crate) use crate::macos::reopen::install as install_reopen;
