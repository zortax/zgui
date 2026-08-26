//! A key press, under the three readings a press has to be given.
//!
//! One press answers three unrelated questions and gives three different answers, and all three
//! have to survive the crossing together or something above has to guess:
//!
//! * **what to insert** — the layout applied, with the modifiers applied;
//! * **which shortcut this is** — the layout applied, the modifiers *not* applied, so that a
//!   shortcut written for one key stays on that key when a modifier would have remapped it;
//! * **which position was pressed** — no layout at all, so that a game's movement keys sit where
//!   the fingers are rather than where the letters are.
//!
//! A press also says whether it is a repeat, and that too is carried rather than resolved: holding
//! a letter down should insert another letter and must not run a command a second time, and only
//! the thing being told about the press knows which of those it is doing.

pub(crate) mod code;
pub mod layout;
pub(crate) mod modifiers;
pub(crate) mod names;
pub(crate) mod terminal;

use zgui_vocab::{KeyCode, KeyEvent, KeyLocation, PhysicalKey};

/// Returns where a key sits when a keyboard has more than one of it.
///
/// The kernel numbers the two shift keys apart and gives the keypad its own codes, so this is read
/// off the position rather than guessed at.
pub(crate) fn location(at: PhysicalKey) -> KeyLocation {
    let Some(at) = at.code() else {
        return KeyLocation::Standard;
    };
    match at {
        KeyCode::ShiftLeft | KeyCode::ControlLeft | KeyCode::AltLeft | KeyCode::MetaLeft => {
            KeyLocation::Left
        }
        KeyCode::ShiftRight | KeyCode::ControlRight | KeyCode::AltRight | KeyCode::MetaRight => {
            KeyLocation::Right
        }
        KeyCode::Numpad0
        | KeyCode::Numpad1
        | KeyCode::Numpad2
        | KeyCode::Numpad3
        | KeyCode::Numpad4
        | KeyCode::Numpad5
        | KeyCode::Numpad6
        | KeyCode::Numpad7
        | KeyCode::Numpad8
        | KeyCode::Numpad9
        | KeyCode::NumpadAdd
        | KeyCode::NumpadComma
        | KeyCode::NumpadDecimal
        | KeyCode::NumpadDivide
        | KeyCode::NumpadEnter
        | KeyCode::NumpadEqual
        | KeyCode::NumpadMultiply
        | KeyCode::NumpadSubtract => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    }
}

/// Returns a press with all three readings of it, at a position, held or not.
///
/// The two layout readings are the caller's, because reading them changes a layout's state and
/// only the layout may decide the order that happens in.
pub(crate) fn event(
    at: PhysicalKey,
    key: zgui_vocab::Key,
    without_modifiers: zgui_vocab::Key,
    repeat: bool,
) -> KeyEvent {
    KeyEvent {
        key,
        key_without_modifiers: without_modifiers,
        physical: at,
        location: location(at),
        repeat,
    }
}

#[cfg(test)]
mod tests {
    use super::{event, location};
    use zgui_vocab::{Key, KeyCode, KeyLocation, NamedKey, PhysicalKey};

    #[test]
    fn a_key_a_keyboard_has_two_of_says_which_one_it_is() {
        let pairs = [
            (KeyCode::ShiftLeft, KeyLocation::Left),
            (KeyCode::ShiftRight, KeyLocation::Right),
            (KeyCode::ControlLeft, KeyLocation::Left),
            (KeyCode::ControlRight, KeyLocation::Right),
            (KeyCode::AltLeft, KeyLocation::Left),
            (KeyCode::AltRight, KeyLocation::Right),
            (KeyCode::MetaLeft, KeyLocation::Left),
            (KeyCode::MetaRight, KeyLocation::Right),
        ];
        for (at, side) in pairs {
            assert_eq!(location(PhysicalKey::Code(at)), side, "{at:?}");
        }
    }

    #[test]
    fn a_key_on_the_keypad_says_that_it_is_on_the_keypad() {
        // The keypad's enter and the main block's enter mean the same thing and are different
        // keys, and this is the only field that tells them apart.
        assert_eq!(
            location(PhysicalKey::Code(KeyCode::NumpadEnter)),
            KeyLocation::Numpad
        );
        assert_eq!(
            location(PhysicalKey::Code(KeyCode::Numpad5)),
            KeyLocation::Numpad
        );
        assert_eq!(
            location(PhysicalKey::Code(KeyCode::Enter)),
            KeyLocation::Standard
        );
    }

    #[test]
    fn a_key_a_keyboard_has_one_of_is_the_standard_one() {
        assert_eq!(
            location(PhysicalKey::Code(KeyCode::KeyA)),
            KeyLocation::Standard
        );
        assert_eq!(
            location(PhysicalKey::Unidentified(0x2ff)),
            KeyLocation::Standard,
            "a position the vocabulary does not name is somewhere, and nothing here knows where"
        );
    }

    #[test]
    fn the_three_readings_travel_together_and_stay_apart() {
        // A German layout with shift held: the key marked `Y` types `Z`, the shortcut it stands
        // for is still `z`, and the position is where `Y` is on every keyboard.
        let press = event(
            PhysicalKey::Code(KeyCode::KeyY),
            Key::character("Z"),
            Key::character("z"),
            false,
        );

        assert_eq!(press.key.inserted_text(), Some("Z"));
        assert_eq!(press.key_without_modifiers, Key::character("z"));
        assert_eq!(press.physical, PhysicalKey::Code(KeyCode::KeyY));
        assert_eq!(press.location, KeyLocation::Standard);
        assert!(!press.repeat);
    }

    #[test]
    fn a_held_key_says_that_it_is_held() {
        let held = event(
            PhysicalKey::Code(KeyCode::Enter),
            Key::Named(NamedKey::Enter),
            Key::Named(NamedKey::Enter),
            true,
        );

        assert!(
            held.repeat,
            "text insertion takes it and a command drops it"
        );
    }
}
