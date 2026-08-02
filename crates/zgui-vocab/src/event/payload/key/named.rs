//! The keys that produce a name rather than a character.

use core::fmt::{self, Display};
use core::str::FromStr;

/// Declares one variant per named key, with the standard name it is written with.
macro_rules! named_keys {
    ($( $name:ident => $text:literal, $doc:literal; )+) => {
        /// A key whose meaning is a name rather than a character it inserts.
        ///
        /// The names are the standard key values a browser reports, so a handler written against
        /// one behaves the same everywhere and a web backend needs no translation table.
        ///
        /// This covers the keys a user interface reacts to. Anything outside it — television
        /// controls, telephony keys, vendor buttons — arrives as
        /// [`Key::Other`](crate::Key::Other) carrying the same standard name as text, so nothing
        /// is lost by the set being finite.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum NamedKey {
            $(
                #[doc = $doc]
                $name,
            )+
        }

        impl NamedKey {
            /// The standard name of this key.
            ///
            /// ```
            /// use zgui_vocab::NamedKey;
            ///
            /// assert_eq!(NamedKey::ArrowUp.as_str(), "ArrowUp");
            /// ```
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( Self::$name => $text, )+
                }
            }

            /// Every named key, in declaration order.
            pub const ALL: &'static [Self] = &[ $( Self::$name, )+ ];
        }

        impl FromStr for NamedKey {
            type Err = UnknownNamedKey;

            fn from_str(text: &str) -> Result<Self, UnknownNamedKey> {
                match text {
                    $( $text => Ok(Self::$name), )+
                    _ => Err(UnknownNamedKey),
                }
            }
        }
    };
}

/// The error from parsing a name this set does not hold.
///
/// A key outside the set is not an error in itself; it is simply carried as text instead.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnknownNamedKey;

impl Display for UnknownNamedKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("not a named key")
    }
}

impl core::error::Error for UnknownNamedKey {}

named_keys! {
    Alt => "Alt", "The alt modifier, called option on some keyboards.";
    AltGraph => "AltGraph", "The modifier that selects a third character on a key.";
    CapsLock => "CapsLock", "The capitals lock.";
    Control => "Control", "The control modifier.";
    Fn => "Fn", "The function modifier found on compact keyboards.";
    FnLock => "FnLock", "The lock for the function modifier.";
    NumLock => "NumLock", "The numeric keypad lock.";
    ScrollLock => "ScrollLock", "The scroll lock.";
    Shift => "Shift", "The shift modifier.";
    Meta => "Meta", "The platform's command modifier: super, command or the windows key.";
    Symbol => "Symbol", "The modifier that selects a symbol on a compact keyboard.";
    SymbolLock => "SymbolLock", "The lock for the symbol modifier.";
    Hyper => "Hyper", "The hyper modifier.";
    Super => "Super", "The super modifier, where it is distinct from meta.";

    Enter => "Enter", "The enter or return key.";
    Tab => "Tab", "The tab key.";
    Space => " ", "The space bar, whose standard value is a single space character.";

    ArrowDown => "ArrowDown", "The down arrow.";
    ArrowLeft => "ArrowLeft", "The left arrow.";
    ArrowRight => "ArrowRight", "The right arrow.";
    ArrowUp => "ArrowUp", "The up arrow.";
    End => "End", "The end key.";
    Home => "Home", "The home key.";
    PageDown => "PageDown", "The page down key.";
    PageUp => "PageUp", "The page up key.";

    Backspace => "Backspace", "The backspace key, which deletes backwards.";
    Clear => "Clear", "The clear key.";
    Copy => "Copy", "The dedicated copy key.";
    Cut => "Cut", "The dedicated cut key.";
    Paste => "Paste", "The dedicated paste key.";
    Delete => "Delete", "The delete key, which deletes forwards.";
    Insert => "Insert", "The insert key.";
    Redo => "Redo", "The dedicated redo key.";
    Undo => "Undo", "The dedicated undo key.";

    Cancel => "Cancel", "The cancel key.";
    ContextMenu => "ContextMenu", "The key that opens a context menu.";
    Escape => "Escape", "The escape key.";
    Execute => "Execute", "The execute key.";
    Find => "Find", "The find key.";
    Help => "Help", "The help key.";
    Pause => "Pause", "The pause key.";
    Play => "Play", "The play key.";
    Select => "Select", "The select key.";
    PrintScreen => "PrintScreen", "The print screen key.";
    Again => "Again", "The repeat-last-action key.";
    Props => "Props", "The properties key.";

    Accept => "Accept", "Accepts the current input-method candidate.";
    Compose => "Compose", "Begins a compose sequence.";
    Convert => "Convert", "Converts the current input-method text.";
    NonConvert => "NonConvert", "Leaves the current input-method text unconverted.";
    ModeChange => "ModeChange", "Changes the input method's mode.";
    Process => "Process", "The key was consumed by the input method.";
    NextCandidate => "NextCandidate", "Selects the next input-method candidate.";
    PreviousCandidate => "PreviousCandidate", "Selects the previous input-method candidate.";
    AllCandidates => "AllCandidates", "Shows every input-method candidate.";
    HangulMode => "HangulMode", "Switches the input method to hangul.";
    HanjaMode => "HanjaMode", "Switches the input method to hanja.";
    KanaMode => "KanaMode", "Switches the input method to kana.";
    KanjiMode => "KanjiMode", "Switches the input method to kanji.";
    Hiragana => "Hiragana", "Switches the input method to hiragana.";
    Katakana => "Katakana", "Switches the input method to katakana.";
    ZenkakuHankaku => "ZenkakuHankaku", "Toggles between full-width and half-width input.";

    BrowserBack => "BrowserBack", "The browser back key.";
    BrowserForward => "BrowserForward", "The browser forward key.";
    BrowserRefresh => "BrowserRefresh", "The browser refresh key.";
    BrowserSearch => "BrowserSearch", "The browser search key.";
    BrowserHome => "BrowserHome", "The browser home key.";

    MediaPlayPause => "MediaPlayPause", "The play and pause key.";
    MediaStop => "MediaStop", "The media stop key.";
    MediaTrackNext => "MediaTrackNext", "The next track key.";
    MediaTrackPrevious => "MediaTrackPrevious", "The previous track key.";
    AudioVolumeDown => "AudioVolumeDown", "The volume down key.";
    AudioVolumeUp => "AudioVolumeUp", "The volume up key.";
    AudioVolumeMute => "AudioVolumeMute", "The mute key.";
    BrightnessDown => "BrightnessDown", "The screen brightness down key.";
    BrightnessUp => "BrightnessUp", "The screen brightness up key.";

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
}

impl NamedKey {
    /// Whether this key is a modifier held alongside another.
    ///
    /// A shortcut matcher uses this to ignore the press of the modifier itself, which otherwise
    /// fires the shortcut as soon as control goes down.
    ///
    /// ```
    /// use zgui_vocab::NamedKey;
    ///
    /// assert!(NamedKey::Control.is_modifier());
    /// assert!(!NamedKey::Enter.is_modifier());
    /// ```
    pub const fn is_modifier(self) -> bool {
        matches!(
            self,
            Self::Alt
                | Self::AltGraph
                | Self::CapsLock
                | Self::Control
                | Self::Fn
                | Self::FnLock
                | Self::NumLock
                | Self::ScrollLock
                | Self::Shift
                | Self::Meta
                | Self::Symbol
                | Self::SymbolLock
                | Self::Hyper
                | Self::Super
        )
    }

    /// Whether this key moves a caret or a selection rather than changing content.
    pub const fn is_navigation(self) -> bool {
        matches!(
            self,
            Self::ArrowDown
                | Self::ArrowLeft
                | Self::ArrowRight
                | Self::ArrowUp
                | Self::End
                | Self::Home
                | Self::PageDown
                | Self::PageUp
        )
    }
}

impl Display for NamedKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{NamedKey, UnknownNamedKey};
    use core::str::FromStr;

    #[test]
    fn every_name_round_trips_through_its_text() {
        for key in NamedKey::ALL {
            assert_eq!(NamedKey::from_str(key.as_str()), Ok(*key));
        }
    }

    #[test]
    fn no_two_keys_share_a_name() {
        for (index, key) in NamedKey::ALL.iter().enumerate() {
            for other in &NamedKey::ALL[index + 1..] {
                assert_ne!(
                    key.as_str(),
                    other.as_str(),
                    "{key:?} and {other:?} collide"
                );
            }
        }
    }

    #[test]
    fn a_key_outside_the_set_is_reported_rather_than_guessed() {
        assert_eq!(NamedKey::from_str("TVSatelliteBS"), Err(UnknownNamedKey));
    }

    #[test]
    fn the_space_bar_reports_the_character_it_inserts() {
        assert_eq!(NamedKey::Space.as_str(), " ");
    }

    #[test]
    fn modifiers_and_navigation_keys_are_disjoint_groups() {
        for key in NamedKey::ALL {
            assert!(!(key.is_modifier() && key.is_navigation()));
        }
        assert!(NamedKey::Shift.is_modifier());
        assert!(NamedKey::PageUp.is_navigation());
    }
}
