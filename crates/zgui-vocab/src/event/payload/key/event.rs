//! What a key press or release carries.

use crate::event::kind::EventKind;
use crate::event::payload::key::code::PhysicalKey;
use crate::event::payload::key::named::NamedKey;
use crate::text::SharedString;

/// What a key means under the layout in force.
///
/// This is the layout-dependent half of a key event, and it is the half a shortcut the user reads
/// off the keycap is written against. The layout-independent half is
/// [`PhysicalKey`](crate::PhysicalKey).
///
/// ```
/// use zgui_vocab::{Key, NamedKey};
///
/// assert_eq!(Key::character("a").as_str(), Some("a"));
/// assert_eq!(Key::Named(NamedKey::Enter).as_str(), Some("Enter"));
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Key {
    /// A key whose meaning is a name.
    Named(NamedKey),
    /// A key that produces text, carrying exactly the text it produces.
    ///
    /// This is a string rather than a character because one key can produce several — a ligature,
    /// or a character outside the basic plane composed from one press.
    Character(SharedString),
    /// A key that starts an accent to be combined with the next one.
    ///
    /// The character is the accent itself, when the platform reports which one.
    Dead(Option<char>),
    /// A key with a standard name this vocabulary does not enumerate, carrying that name.
    Other(SharedString),
    /// A key the platform could not identify at all.
    Unidentified,
}

impl Key {
    /// A key that produces `text`.
    pub fn character(text: impl Into<SharedString>) -> Self {
        Self::Character(text.into())
    }

    /// The standard value of this key, when it has one.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Named(named) => Some(named.as_str()),
            Self::Character(text) | Self::Other(text) => Some(text.as_str()),
            Self::Dead(_) | Self::Unidentified => None,
        }
    }

    /// The text this key inserts, which is nothing at all for a key that means a name.
    ///
    /// The space bar is the case worth knowing: its standard value *is* a space, so it inserts
    /// text even though it is a named key.
    ///
    /// ```
    /// use zgui_vocab::{Key, NamedKey};
    ///
    /// assert_eq!(Key::character("é").inserted_text(), Some("é"));
    /// assert_eq!(Key::Named(NamedKey::Space).inserted_text(), Some(" "));
    /// assert_eq!(Key::Named(NamedKey::Enter).inserted_text(), None);
    /// ```
    pub fn inserted_text(&self) -> Option<&str> {
        match self {
            Self::Character(text) => Some(text.as_str()),
            Self::Named(NamedKey::Space) => Some(NamedKey::Space.as_str()),
            _ => None,
        }
    }

    /// Whether this is a modifier held alongside another key.
    pub fn is_modifier(&self) -> bool {
        matches!(self, Self::Named(named) if named.is_modifier())
    }
}

impl From<NamedKey> for Key {
    fn from(named: NamedKey) -> Self {
        Self::Named(named)
    }
}

/// Whether a key went down or came up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyState {
    /// The key went down.
    Pressed,
    /// The key came up.
    Released,
}

impl KeyState {
    /// The kind of event this state is delivered as.
    pub const fn event_kind(self) -> EventKind {
        match self {
            Self::Pressed => EventKind::KeyDown,
            Self::Released => EventKind::KeyUp,
        }
    }
}

/// Which of several same-named keys was used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum KeyLocation {
    /// The only key with this meaning, or the one in the main block.
    #[default]
    Standard,
    /// The left one of a pair.
    Left,
    /// The right one of a pair.
    Right,
    /// The one on the numeric keypad.
    Numpad,
}

/// What a key event carries.
///
/// Three descriptions of the same press sit side by side, and a handler picks the one that matches
/// what it is doing. [`KeyEvent::key`] is what the key means now, and is what a shortcut printed
/// on a menu is matched against. [`KeyEvent::key_without_modifiers`] is what the same key would
/// mean unshifted, which is what keeps a shortcut working when the user holds shift.
/// [`KeyEvent::physical`] is where the key is, which is what a position-based binding uses.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyEvent {
    /// What the key means under the layout in force, with modifiers applied.
    pub key: Key,
    /// What the key would mean with no modifier but the layout applied.
    pub key_without_modifiers: Key,
    /// Where the key sits on the keyboard.
    pub physical: PhysicalKey,
    /// Which of several same-named keys this is.
    pub location: KeyLocation,
    /// Whether this press was produced by the key being held down.
    ///
    /// Text insertion accepts repeats; a command dispatch drops them, or holding a shortcut runs
    /// it dozens of times.
    pub repeat: bool,
}

impl KeyEvent {
    /// A press of a named key, with no layout subtleties.
    pub fn named(key: NamedKey, physical: PhysicalKey) -> Self {
        Self {
            key: Key::Named(key),
            key_without_modifiers: Key::Named(key),
            physical,
            location: KeyLocation::Standard,
            repeat: false,
        }
    }

    /// A press of a key that produces `text`, with no layout subtleties.
    ///
    /// The physical key is left unidentified, because what a key inserts and where it sits on the
    /// keyboard are independent: the same text comes from different places under different layouts,
    /// and claiming a position here would be inventing one. A binding that cares about position
    /// reads [`KeyEvent::physical`] and finds nothing rather than finding something wrong.
    ///
    /// ```
    /// use zgui_vocab::KeyEvent;
    ///
    /// let event = KeyEvent::character("q");
    /// assert_eq!(event.key.inserted_text(), Some("q"));
    /// ```
    pub fn character(text: impl Into<crate::text::SharedString>) -> Self {
        let key = Key::character(text);
        Self {
            key_without_modifiers: key.clone(),
            key,
            physical: PhysicalKey::Unidentified(0),
            location: KeyLocation::Standard,
            repeat: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, KeyEvent, KeyLocation, KeyState};
    use crate::event::kind::EventKind;
    use crate::event::payload::key::code::{KeyCode, PhysicalKey};
    use crate::event::payload::key::named::NamedKey;

    #[test]
    fn a_press_and_a_release_are_different_event_kinds() {
        assert_eq!(KeyState::Pressed.event_kind(), EventKind::KeyDown);
        assert_eq!(KeyState::Released.event_kind(), EventKind::KeyUp);
    }

    #[test]
    fn only_keys_that_produce_text_report_inserted_text() {
        assert_eq!(Key::character("q").inserted_text(), Some("q"));
        assert_eq!(Key::Named(NamedKey::Space).inserted_text(), Some(" "));
        assert_eq!(Key::Named(NamedKey::Tab).inserted_text(), None);
        assert_eq!(Key::Dead(Some('\u{301}')).inserted_text(), None);
        assert_eq!(Key::Unidentified.inserted_text(), None);
    }

    #[test]
    fn an_unenumerated_key_still_reports_its_standard_name() {
        let key = Key::Other("TVPower".into());
        assert_eq!(key.as_str(), Some("TVPower"));
        assert!(!key.is_modifier());
    }

    #[test]
    fn a_named_press_agrees_with_itself_under_every_layout_reading() {
        let event = KeyEvent::named(NamedKey::Escape, PhysicalKey::Code(KeyCode::Escape));
        assert_eq!(event.key, event.key_without_modifiers);
        assert_eq!(event.location, KeyLocation::Standard);
        assert!(!event.repeat);
    }
}
