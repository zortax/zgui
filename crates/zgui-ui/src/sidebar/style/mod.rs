//! What a sidebar looks like, in tokens.
//!
//! Three sheets rather than one, because a sidebar dresses three unrelated things: the frame the
//! panel and the page share, the bands stacked inside the panel, and the list of places it holds.
//! Only the frame is scoped — it is the one element every other rule hangs off, and the parts read
//! its state through the stable `zui-sidebar-provider` class rather than through a scope of their
//! own.

mod bands;
mod frame;
mod menu;

use zgui::prelude::install_stylesheet;

pub use crate::sidebar::style::bands::SidebarBandStyle;
pub use crate::sidebar::style::frame::SidebarStyle;
pub use crate::sidebar::style::menu::SidebarMenuStyle;

/// What the frame's rules are installed under.
const FRAME: &str = "zui-sidebar";
/// What the bands' rules are installed under.
const BANDS: &str = "zui-sidebar-bands";
/// What the menu's rules are installed under.
const MENU: &str = "zui-sidebar-menu";

/// Puts every sheet a sidebar needs in place.
///
/// Any part will do it: a panel built out of three of the dozen parts still wants the frame's
/// state rules, and installing a sheet twice is free.
pub(crate) fn install() {
    install_stylesheet(FRAME, SidebarStyle::CSS);
    install_stylesheet(BANDS, SidebarBandStyle::CSS);
    install_stylesheet(MENU, SidebarMenuStyle::CSS);
}
