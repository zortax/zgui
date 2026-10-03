//! The AppKit objects winit does not expose: the application menu and its key equivalents, the
//! reopen request, the title-bar buttons, the title-bar double-click preference and the momentum
//! phase of a scroll.
//!
//! Every function here runs on the main thread, inside a callback of the event loop.

mod buttons;
mod double_click;
mod fullscreen;
mod keys;
mod menu;
mod reopen;
mod scroll;
mod window;

pub(crate) use crate::macos::buttons::place as place_title_buttons;
pub(crate) use crate::macos::double_click::perform as title_bar_double_click;
pub(crate) use crate::macos::fullscreen::{
    forget as forget_surface, observe as observe_fullscreen,
};
pub(crate) use crate::macos::keys::install as answer_key_equivalents;
pub(crate) use crate::macos::menu::{Menus, install as install_menu};
pub(crate) use crate::macos::reopen::install as install_reopen;
pub(crate) use crate::macos::scroll::{install as read_scroll_phases, phase as scroll_phase};
pub(crate) use crate::macos::window::ns_view;
