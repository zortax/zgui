//! Menu entries whose behaviour belongs to the framework or the desktop.

use zgui_vocab::{Key, Modifiers};

use crate::menu::shortcut::Shortcut;

/// A standard entry the framework or the desktop carries out.
///
/// The desktop roles act on the application or a window directly. The editing roles act on
/// whatever has the keyboard: the framework replays their shortcut into the focused window, so a
/// text field, a terminal and a custom editor each handle them the way they handle the keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MenuRole {
    /// Shows the application's name, version and icon.
    About,
    /// The desktop's services submenu.
    Services,
    /// Hides the application.
    Hide,
    /// Hides every other application.
    HideOthers,
    /// Shows every application again.
    ShowAll,
    /// Minimizes the key window.
    Minimize,
    /// Zooms the key window.
    Zoom,
    /// Puts the key window full screen, or takes it out.
    Fullscreen,
    /// Brings every window of the application to the front.
    BringAllToFront,
    /// Undoes the last edit in the focused element.
    Undo,
    /// Redoes the last undone edit in the focused element.
    Redo,
    /// Cuts the selection in the focused element.
    Cut,
    /// Copies the selection in the focused element.
    Copy,
    /// Pastes into the focused element.
    Paste,
    /// Selects everything in the focused element.
    SelectAll,
}

impl MenuRole {
    /// Whether the framework carries this role out by replaying its shortcut.
    pub const fn is_editing(self) -> bool {
        matches!(
            self,
            Self::Undo | Self::Redo | Self::Cut | Self::Copy | Self::Paste | Self::SelectAll
        )
    }

    /// The label the menu shows.
    pub const fn label(self) -> &'static str {
        match self {
            Self::About => "About",
            Self::Services => "Services",
            Self::Hide => "Hide",
            Self::HideOthers => "Hide Others",
            Self::ShowAll => "Show All",
            Self::Minimize => "Minimize",
            Self::Zoom => "Zoom",
            Self::Fullscreen => "Enter Full Screen",
            Self::BringAllToFront => "Bring All to Front",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select All",
        }
    }

    /// The standard shortcut of this role on this platform.
    pub fn shortcut(self) -> Option<Shortcut> {
        let primary = Modifiers::PRIMARY;
        let letter = |text: &str, modifiers| Some(Shortcut::new(Key::character(text), modifiers));
        match self {
            Self::Hide => letter("h", primary),
            Self::HideOthers => letter("h", primary | Modifiers::ALT),
            Self::Minimize => letter("m", primary),
            Self::Fullscreen => letter("f", primary | Modifiers::CONTROL),
            Self::Undo => letter("z", primary),
            Self::Redo => letter("z", primary | Modifiers::SHIFT),
            Self::Cut => letter("x", primary),
            Self::Copy => letter("c", primary),
            Self::Paste => letter("v", primary),
            Self::SelectAll => letter("a", primary),
            Self::About | Self::Services | Self::ShowAll | Self::Zoom | Self::BringAllToFront => {
                None
            }
        }
    }

    /// Every role, in declaration order.
    pub const ALL: [Self; 15] = [
        Self::About,
        Self::Services,
        Self::Hide,
        Self::HideOthers,
        Self::ShowAll,
        Self::Minimize,
        Self::Zoom,
        Self::Fullscreen,
        Self::BringAllToFront,
        Self::Undo,
        Self::Redo,
        Self::Cut,
        Self::Copy,
        Self::Paste,
        Self::SelectAll,
    ];
}
