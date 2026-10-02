//! What the desktop says to the application as a whole.

use crate::menu::{MenuId, MenuRole};

/// Something the desktop asked of the application rather than of one window.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppEvent {
    /// The user asked for the application again: a click on its dock icon, or a second launch.
    ///
    /// An application that hides its last window shows it again here.
    Reopen,
    /// The user picked an action of the application menu.
    Menu(MenuId),
    /// The user picked an editing role of the application menu.
    ///
    /// The framework replays the role's shortcut into the focused window.
    Role(MenuRole),
}
