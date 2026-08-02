//! Which reading of a key press is the right one for what is being decided.
//!
//! One press answers three different questions and gives three different answers. What character
//! should be inserted is a question about the layout *with* modifiers applied. What shortcut this
//! is, is a question about the layout *without* them, because a shortcut written for `Ctrl+C`
//! stays `Ctrl+C` when the user holds a modifier that would otherwise remap the key. And whether
//! this press counts at all depends on the first two: a repeat inserts another character and must
//! not run a command a second time.

use zgui_vocab::{Key, KeyEvent};

/// What a key press is being read for.
///
/// The distinction is not cosmetic: holding a key down while a command is bound to it runs that
/// command dozens of times a second, and dropping repeats while text is being typed makes holding
/// a letter insert exactly one of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Reading {
    /// The press is being read as text to insert.
    Text,
    /// The press is being read as a command to run.
    Command,
}

/// Whether this press should be acted on, read this way.
///
/// ```
/// use zgui_input::normalize::keyboard::{Reading, accepts};
/// use zgui_vocab::{KeyCode, KeyEvent, NamedKey, PhysicalKey};
///
/// let mut held = KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter));
/// held.repeat = true;
///
/// assert!(accepts(&held, Reading::Text), "holding a key keeps producing text");
/// assert!(!accepts(&held, Reading::Command), "and must not keep running a command");
/// ```
pub fn accepts(event: &KeyEvent, reading: Reading) -> bool {
    match reading {
        Reading::Text => true,
        Reading::Command => !event.repeat,
    }
}

/// The key a shortcut should be matched against.
///
/// The layout is applied and the modifiers are not, which is what keeps a shortcut bound to the
/// key the user's keyboard is printed with rather than to whatever the modifier turned it into.
///
/// ```
/// use zgui_input::normalize::keyboard::shortcut_key;
/// use zgui_vocab::{Key, KeyCode, KeyEvent, PhysicalKey};
///
/// let mut event = KeyEvent::named(zgui_vocab::NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter));
/// event.key = Key::Character("\r".into());
/// event.key_without_modifiers = Key::Named(zgui_vocab::NamedKey::Enter);
///
/// // Not the carriage return the layout produced: the key itself.
/// assert_eq!(shortcut_key(&event), &Key::Named(zgui_vocab::NamedKey::Enter));
/// ```
pub fn shortcut_key(event: &KeyEvent) -> &Key {
    &event.key_without_modifiers
}

#[cfg(test)]
mod tests {
    use zgui_vocab::{KeyCode, KeyEvent, NamedKey, PhysicalKey};

    use super::{Reading, accepts, shortcut_key};

    fn enter() -> KeyEvent {
        KeyEvent::named(NamedKey::Enter, PhysicalKey::Code(KeyCode::Enter))
    }

    #[test]
    fn a_first_press_is_accepted_under_both_readings() {
        let event = enter();
        assert!(accepts(&event, Reading::Text));
        assert!(accepts(&event, Reading::Command));
    }

    #[test]
    fn a_repeat_is_text_and_never_a_command() {
        let mut event = enter();
        event.repeat = true;
        assert!(accepts(&event, Reading::Text));
        assert!(!accepts(&event, Reading::Command));
    }

    #[test]
    fn the_shortcut_reading_ignores_what_the_modifiers_did_to_the_key() {
        let mut event = enter();
        event.key = zgui_vocab::Key::Character("\r".into());
        assert_eq!(shortcut_key(&event), &event.key_without_modifiers);
        assert_ne!(shortcut_key(&event), &event.key);
    }
}
