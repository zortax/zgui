//! Which physical key was pressed, independently of what it produces.

use core::fmt::{self, Display};
use core::str::FromStr;

/// Declares one variant per physical key position, with the standard code it is written with.
macro_rules! key_codes {
    ($( $name:ident => $text:literal, $doc:literal; )+) => {
        /// A key's position on the keyboard, independent of the layout in force.
        ///
        /// The names describe where the key *is* on a keyboard laid out in the standard way, not
        /// what it types. On a Dvorak layout the key labelled `.` is still `KeyE`, because that is
        /// where the key sits. This is what a shortcut bound to a position — the movement cluster
        /// of a game, a chord chosen for where the fingers fall — has to be written against.
        ///
        /// A shortcut the user reads off the keycap is the opposite case and belongs against
        /// [`Key`](crate::Key) instead.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum KeyCode {
            $(
                #[doc = $doc]
                $name,
            )+
        }

        impl KeyCode {
            /// The standard code of this key position.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$name => $text, )+
                }
            }

            /// Every key position, in declaration order.
            pub const ALL: &'static [Self] = &[ $( Self::$name, )+ ];
        }

        impl FromStr for KeyCode {
            type Err = UnknownKeyCode;

            fn from_str(text: &str) -> Result<Self, UnknownKeyCode> {
                match text {
                    $( $text => Ok(Self::$name), )+
                    _ => Err(UnknownKeyCode),
                }
            }
        }
    };
}

/// The error from parsing a code this set does not hold.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnknownKeyCode;

impl Display for UnknownKeyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not a key code")
    }
}

impl core::error::Error for UnknownKeyCode {}

key_codes! {
    Backquote => "Backquote", "The key left of the digit row's first key.";
    Backslash => "Backslash", "The key above the enter key on a standard layout.";
    BracketLeft => "BracketLeft", "The first bracket key, right of the top letter row.";
    BracketRight => "BracketRight", "The second bracket key.";
    Comma => "Comma", "The comma key.";
    Digit0 => "Digit0", "The zero of the digit row.";
    Digit1 => "Digit1", "The one of the digit row.";
    Digit2 => "Digit2", "The two of the digit row.";
    Digit3 => "Digit3", "The three of the digit row.";
    Digit4 => "Digit4", "The four of the digit row.";
    Digit5 => "Digit5", "The five of the digit row.";
    Digit6 => "Digit6", "The six of the digit row.";
    Digit7 => "Digit7", "The seven of the digit row.";
    Digit8 => "Digit8", "The eight of the digit row.";
    Digit9 => "Digit9", "The nine of the digit row.";
    Equal => "Equal", "The key right of the digit row.";
    IntlBackslash => "IntlBackslash", "The extra key of a hundred-and-two-key layout.";
    IntlRo => "IntlRo", "The extra key of a Japanese layout, right of the bottom letter row.";
    IntlYen => "IntlYen", "The extra key of a Japanese layout, right of the digit row.";
    KeyA => "KeyA", "The key labelled A on a standard layout.";
    KeyB => "KeyB", "The key labelled B on a standard layout.";
    KeyC => "KeyC", "The key labelled C on a standard layout.";
    KeyD => "KeyD", "The key labelled D on a standard layout.";
    KeyE => "KeyE", "The key labelled E on a standard layout.";
    KeyF => "KeyF", "The key labelled F on a standard layout.";
    KeyG => "KeyG", "The key labelled G on a standard layout.";
    KeyH => "KeyH", "The key labelled H on a standard layout.";
    KeyI => "KeyI", "The key labelled I on a standard layout.";
    KeyJ => "KeyJ", "The key labelled J on a standard layout.";
    KeyK => "KeyK", "The key labelled K on a standard layout.";
    KeyL => "KeyL", "The key labelled L on a standard layout.";
    KeyM => "KeyM", "The key labelled M on a standard layout.";
    KeyN => "KeyN", "The key labelled N on a standard layout.";
    KeyO => "KeyO", "The key labelled O on a standard layout.";
    KeyP => "KeyP", "The key labelled P on a standard layout.";
    KeyQ => "KeyQ", "The key labelled Q on a standard layout.";
    KeyR => "KeyR", "The key labelled R on a standard layout.";
    KeyS => "KeyS", "The key labelled S on a standard layout.";
    KeyT => "KeyT", "The key labelled T on a standard layout.";
    KeyU => "KeyU", "The key labelled U on a standard layout.";
    KeyV => "KeyV", "The key labelled V on a standard layout.";
    KeyW => "KeyW", "The key labelled W on a standard layout.";
    KeyX => "KeyX", "The key labelled X on a standard layout.";
    KeyY => "KeyY", "The key labelled Y on a standard layout.";
    KeyZ => "KeyZ", "The key labelled Z on a standard layout.";
    Minus => "Minus", "The key left of the equals key.";
    Period => "Period", "The full stop key.";
    Quote => "Quote", "The apostrophe key.";
    Semicolon => "Semicolon", "The semicolon key.";
    Slash => "Slash", "The key left of the right shift on a standard layout.";

    AltLeft => "AltLeft", "The left alt key.";
    AltRight => "AltRight", "The right alt key.";
    Backspace => "Backspace", "The backspace key.";
    CapsLock => "CapsLock", "The capitals lock key.";
    ContextMenu => "ContextMenu", "The context menu key.";
    ControlLeft => "ControlLeft", "The left control key.";
    ControlRight => "ControlRight", "The right control key.";
    Enter => "Enter", "The enter key of the main block.";
    MetaLeft => "MetaLeft", "The left command key.";
    MetaRight => "MetaRight", "The right command key.";
    ShiftLeft => "ShiftLeft", "The left shift key.";
    ShiftRight => "ShiftRight", "The right shift key.";
    Space => "Space", "The space bar.";
    Tab => "Tab", "The tab key.";

    Convert => "Convert", "The input-method conversion key.";
    KanaMode => "KanaMode", "The kana mode key.";
    NonConvert => "NonConvert", "The input-method non-conversion key.";

    Delete => "Delete", "The forward delete key of the navigation block.";
    End => "End", "The end key.";
    Help => "Help", "The help key.";
    Home => "Home", "The home key.";
    Insert => "Insert", "The insert key.";
    PageDown => "PageDown", "The page down key.";
    PageUp => "PageUp", "The page up key.";
    ArrowDown => "ArrowDown", "The down arrow.";
    ArrowLeft => "ArrowLeft", "The left arrow.";
    ArrowRight => "ArrowRight", "The right arrow.";
    ArrowUp => "ArrowUp", "The up arrow.";

    NumLock => "NumLock", "The numeric keypad lock.";
    Numpad0 => "Numpad0", "The keypad zero.";
    Numpad1 => "Numpad1", "The keypad one.";
    Numpad2 => "Numpad2", "The keypad two.";
    Numpad3 => "Numpad3", "The keypad three.";
    Numpad4 => "Numpad4", "The keypad four.";
    Numpad5 => "Numpad5", "The keypad five.";
    Numpad6 => "Numpad6", "The keypad six.";
    Numpad7 => "Numpad7", "The keypad seven.";
    Numpad8 => "Numpad8", "The keypad eight.";
    Numpad9 => "Numpad9", "The keypad nine.";
    NumpadAdd => "NumpadAdd", "The keypad plus.";
    NumpadComma => "NumpadComma", "The keypad comma.";
    NumpadDecimal => "NumpadDecimal", "The keypad decimal separator.";
    NumpadDivide => "NumpadDivide", "The keypad divide.";
    NumpadEnter => "NumpadEnter", "The keypad enter.";
    NumpadEqual => "NumpadEqual", "The keypad equals.";
    NumpadMultiply => "NumpadMultiply", "The keypad multiply.";
    NumpadSubtract => "NumpadSubtract", "The keypad minus.";

    Escape => "Escape", "The escape key.";
    PrintScreen => "PrintScreen", "The print screen key.";
    ScrollLock => "ScrollLock", "The scroll lock key.";
    Pause => "Pause", "The pause key.";

    F1 => "F1", "The first function key.";
    F2 => "F2", "The second function key.";
    F3 => "F3", "The third function key.";
    F4 => "F4", "The fourth function key.";
    F5 => "F5", "The fifth function key.";
    F6 => "F6", "The sixth function key.";
    F7 => "F7", "The seventh function key.";
    F8 => "F8", "The eighth function key.";
    F9 => "F9", "The ninth function key.";
    F10 => "F10", "The tenth function key.";
    F11 => "F11", "The eleventh function key.";
    F12 => "F12", "The twelfth function key.";
    F13 => "F13", "The thirteenth function key.";
    F14 => "F14", "The fourteenth function key.";
    F15 => "F15", "The fifteenth function key.";
    F16 => "F16", "The sixteenth function key.";
    F17 => "F17", "The seventeenth function key.";
    F18 => "F18", "The eighteenth function key.";
    F19 => "F19", "The nineteenth function key.";
    F20 => "F20", "The twentieth function key.";
    F21 => "F21", "The twenty-first function key.";
    F22 => "F22", "The twenty-second function key.";
    F23 => "F23", "The twenty-third function key.";
    F24 => "F24", "The twenty-fourth function key.";

    AudioVolumeDown => "AudioVolumeDown", "The volume down key.";
    AudioVolumeMute => "AudioVolumeMute", "The mute key.";
    AudioVolumeUp => "AudioVolumeUp", "The volume up key.";
    MediaPlayPause => "MediaPlayPause", "The play and pause key.";
    MediaStop => "MediaStop", "The media stop key.";
    MediaTrackNext => "MediaTrackNext", "The next track key.";
    MediaTrackPrevious => "MediaTrackPrevious", "The previous track key.";
}

impl Display for KeyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Where a key sits, including keys this vocabulary has no name for.
///
/// A keyboard can report a position that is not in the standard set — an extra key on a laptop, a
/// programmable key on a gaming keyboard. Those arrive as [`PhysicalKey::Unidentified`] carrying
/// whatever the platform calls them, so they can still be bound to something even though nothing
/// can say what they are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum PhysicalKey {
    /// A key at a position this vocabulary names.
    Code(KeyCode),
    /// A key at a position it does not, identified by the platform's own number for it.
    Unidentified(u32),
}

impl PhysicalKey {
    /// The named position, when there is one.
    pub const fn code(self) -> Option<KeyCode> {
        match self {
            Self::Code(code) => Some(code),
            Self::Unidentified(_) => None,
        }
    }
}

impl From<KeyCode> for PhysicalKey {
    fn from(code: KeyCode) -> Self {
        Self::Code(code)
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyCode, PhysicalKey, UnknownKeyCode};
    use core::str::FromStr;

    #[test]
    fn every_code_round_trips_through_its_text() {
        for code in KeyCode::ALL {
            assert_eq!(KeyCode::from_str(code.as_str()), Ok(*code));
        }
    }

    #[test]
    fn no_two_positions_share_a_code() {
        for (index, code) in KeyCode::ALL.iter().enumerate() {
            for other in &KeyCode::ALL[index + 1..] {
                assert_ne!(
                    code.as_str(),
                    other.as_str(),
                    "{code:?} and {other:?} collide"
                );
            }
        }
    }

    #[test]
    fn an_unnamed_position_is_carried_rather_than_dropped() {
        assert_eq!(KeyCode::from_str("Lang5"), Err(UnknownKeyCode));
        assert_eq!(PhysicalKey::Unidentified(191).code(), None);
        assert_eq!(PhysicalKey::from(KeyCode::KeyW).code(), Some(KeyCode::KeyW));
    }
}
