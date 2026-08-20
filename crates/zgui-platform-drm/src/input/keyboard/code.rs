//! Which position on the keyboard was pressed, independent of what is printed on it.
//!
//! A kernel key code *is* a position. `KEY_A` is the key a standard layout marks `A`, on a Dvorak
//! keyboard as much as on a British one, so the correspondence with
//! [`KeyCode`](zgui_vocab::KeyCode) is mechanical and exact: one row per position, no layout
//! anywhere in it.
//!
//! # The fallback names
//!
//! [`name`] is the other half. A console keymap says what a key *types* and has no way to say that
//! a key is Escape — the console driver treats that as an action it takes on itself — so a layout
//! with nothing to say falls back to the name a position carries on a standard keyboard. That
//! fallback belongs here, because it is a statement about positions and about nothing else.

use zgui_evdev::Key;
use zgui_vocab::{KeyCode, NamedKey, PhysicalKey};

/// Where each key the kernel numbers sits, in the standard vocabulary.
///
/// Every [`KeyCode`] the vocabulary holds is in here exactly once, and so is every kernel code
/// that reaches one.
// The tests below assert both properties. A table of this shape goes wrong in one row at a time and
// the row is silent: a key that answers for its neighbour still answers.
const POSITIONS: &[(Key, KeyCode)] = &[
    // The writing-system block: the keys whose *meaning* a layout changes and whose position it
    // never does.
    (Key::KEY_GRAVE, KeyCode::Backquote),
    (Key::KEY_BACKSLASH, KeyCode::Backslash),
    (Key::KEY_LEFTBRACE, KeyCode::BracketLeft),
    (Key::KEY_RIGHTBRACE, KeyCode::BracketRight),
    (Key::KEY_COMMA, KeyCode::Comma),
    (Key::KEY_0, KeyCode::Digit0),
    (Key::KEY_1, KeyCode::Digit1),
    (Key::KEY_2, KeyCode::Digit2),
    (Key::KEY_3, KeyCode::Digit3),
    (Key::KEY_4, KeyCode::Digit4),
    (Key::KEY_5, KeyCode::Digit5),
    (Key::KEY_6, KeyCode::Digit6),
    (Key::KEY_7, KeyCode::Digit7),
    (Key::KEY_8, KeyCode::Digit8),
    (Key::KEY_9, KeyCode::Digit9),
    (Key::KEY_EQUAL, KeyCode::Equal),
    // The extra key of a hundred-and-two-key layout, which the kernel numbers after the rest of
    // the block rather than inside it.
    (Key::KEY_102ND, KeyCode::IntlBackslash),
    (Key::KEY_RO, KeyCode::IntlRo),
    (Key::KEY_YEN, KeyCode::IntlYen),
    (Key::KEY_A, KeyCode::KeyA),
    (Key::KEY_B, KeyCode::KeyB),
    (Key::KEY_C, KeyCode::KeyC),
    (Key::KEY_D, KeyCode::KeyD),
    (Key::KEY_E, KeyCode::KeyE),
    (Key::KEY_F, KeyCode::KeyF),
    (Key::KEY_G, KeyCode::KeyG),
    (Key::KEY_H, KeyCode::KeyH),
    (Key::KEY_I, KeyCode::KeyI),
    (Key::KEY_J, KeyCode::KeyJ),
    (Key::KEY_K, KeyCode::KeyK),
    (Key::KEY_L, KeyCode::KeyL),
    (Key::KEY_M, KeyCode::KeyM),
    (Key::KEY_N, KeyCode::KeyN),
    (Key::KEY_O, KeyCode::KeyO),
    (Key::KEY_P, KeyCode::KeyP),
    (Key::KEY_Q, KeyCode::KeyQ),
    (Key::KEY_R, KeyCode::KeyR),
    (Key::KEY_S, KeyCode::KeyS),
    (Key::KEY_T, KeyCode::KeyT),
    (Key::KEY_U, KeyCode::KeyU),
    (Key::KEY_V, KeyCode::KeyV),
    (Key::KEY_W, KeyCode::KeyW),
    (Key::KEY_X, KeyCode::KeyX),
    (Key::KEY_Y, KeyCode::KeyY),
    (Key::KEY_Z, KeyCode::KeyZ),
    (Key::KEY_MINUS, KeyCode::Minus),
    (Key::KEY_DOT, KeyCode::Period),
    (Key::KEY_APOSTROPHE, KeyCode::Quote),
    (Key::KEY_SEMICOLON, KeyCode::Semicolon),
    (Key::KEY_SLASH, KeyCode::Slash),
    // The functional block.
    (Key::KEY_LEFTALT, KeyCode::AltLeft),
    (Key::KEY_RIGHTALT, KeyCode::AltRight),
    (Key::KEY_BACKSPACE, KeyCode::Backspace),
    (Key::KEY_CAPSLOCK, KeyCode::CapsLock),
    // The kernel's name for the menu key is `KEY_COMPOSE`, from a time when that is what the key
    // did. `KEY_MENU` is a different key and no keyboard in use has it.
    (Key::KEY_COMPOSE, KeyCode::ContextMenu),
    (Key::KEY_LEFTCTRL, KeyCode::ControlLeft),
    (Key::KEY_RIGHTCTRL, KeyCode::ControlRight),
    (Key::KEY_ENTER, KeyCode::Enter),
    // The kernel calls the key beside the space bar meta, and so does the standard.
    (Key::KEY_LEFTMETA, KeyCode::MetaLeft),
    (Key::KEY_RIGHTMETA, KeyCode::MetaRight),
    (Key::KEY_LEFTSHIFT, KeyCode::ShiftLeft),
    (Key::KEY_RIGHTSHIFT, KeyCode::ShiftRight),
    (Key::KEY_SPACE, KeyCode::Space),
    (Key::KEY_TAB, KeyCode::Tab),
    // The three keys a Japanese keyboard has beside its space bar.
    (Key::KEY_HENKAN, KeyCode::Convert),
    (Key::KEY_KATAKANAHIRAGANA, KeyCode::KanaMode),
    (Key::KEY_MUHENKAN, KeyCode::NonConvert),
    // The control pad and the arrows.
    (Key::KEY_DELETE, KeyCode::Delete),
    (Key::KEY_END, KeyCode::End),
    (Key::KEY_HELP, KeyCode::Help),
    (Key::KEY_HOME, KeyCode::Home),
    (Key::KEY_INSERT, KeyCode::Insert),
    (Key::KEY_PAGEDOWN, KeyCode::PageDown),
    (Key::KEY_PAGEUP, KeyCode::PageUp),
    (Key::KEY_DOWN, KeyCode::ArrowDown),
    (Key::KEY_LEFT, KeyCode::ArrowLeft),
    (Key::KEY_RIGHT, KeyCode::ArrowRight),
    (Key::KEY_UP, KeyCode::ArrowUp),
    // The numeric keypad.
    (Key::KEY_NUMLOCK, KeyCode::NumLock),
    (Key::KEY_KP0, KeyCode::Numpad0),
    (Key::KEY_KP1, KeyCode::Numpad1),
    (Key::KEY_KP2, KeyCode::Numpad2),
    (Key::KEY_KP3, KeyCode::Numpad3),
    (Key::KEY_KP4, KeyCode::Numpad4),
    (Key::KEY_KP5, KeyCode::Numpad5),
    (Key::KEY_KP6, KeyCode::Numpad6),
    (Key::KEY_KP7, KeyCode::Numpad7),
    (Key::KEY_KP8, KeyCode::Numpad8),
    (Key::KEY_KP9, KeyCode::Numpad9),
    (Key::KEY_KPPLUS, KeyCode::NumpadAdd),
    (Key::KEY_KPCOMMA, KeyCode::NumpadComma),
    (Key::KEY_KPDOT, KeyCode::NumpadDecimal),
    (Key::KEY_KPSLASH, KeyCode::NumpadDivide),
    (Key::KEY_KPENTER, KeyCode::NumpadEnter),
    (Key::KEY_KPEQUAL, KeyCode::NumpadEqual),
    (Key::KEY_KPASTERISK, KeyCode::NumpadMultiply),
    (Key::KEY_KPMINUS, KeyCode::NumpadSubtract),
    // The function section.
    (Key::KEY_ESC, KeyCode::Escape),
    (Key::KEY_SYSRQ, KeyCode::PrintScreen),
    (Key::KEY_SCROLLLOCK, KeyCode::ScrollLock),
    (Key::KEY_PAUSE, KeyCode::Pause),
    (Key::KEY_F1, KeyCode::F1),
    (Key::KEY_F2, KeyCode::F2),
    (Key::KEY_F3, KeyCode::F3),
    (Key::KEY_F4, KeyCode::F4),
    (Key::KEY_F5, KeyCode::F5),
    (Key::KEY_F6, KeyCode::F6),
    (Key::KEY_F7, KeyCode::F7),
    (Key::KEY_F8, KeyCode::F8),
    (Key::KEY_F9, KeyCode::F9),
    (Key::KEY_F10, KeyCode::F10),
    (Key::KEY_F11, KeyCode::F11),
    (Key::KEY_F12, KeyCode::F12),
    (Key::KEY_F13, KeyCode::F13),
    (Key::KEY_F14, KeyCode::F14),
    (Key::KEY_F15, KeyCode::F15),
    (Key::KEY_F16, KeyCode::F16),
    (Key::KEY_F17, KeyCode::F17),
    (Key::KEY_F18, KeyCode::F18),
    (Key::KEY_F19, KeyCode::F19),
    (Key::KEY_F20, KeyCode::F20),
    (Key::KEY_F21, KeyCode::F21),
    (Key::KEY_F22, KeyCode::F22),
    (Key::KEY_F23, KeyCode::F23),
    (Key::KEY_F24, KeyCode::F24),
    // The media keys a keyboard puts wherever it likes.
    (Key::KEY_VOLUMEDOWN, KeyCode::AudioVolumeDown),
    (Key::KEY_MUTE, KeyCode::AudioVolumeMute),
    (Key::KEY_VOLUMEUP, KeyCode::AudioVolumeUp),
    (Key::KEY_PLAYPAUSE, KeyCode::MediaPlayPause),
    (Key::KEY_STOPCD, KeyCode::MediaStop),
    (Key::KEY_NEXTSONG, KeyCode::MediaTrackNext),
    (Key::KEY_PREVIOUSSONG, KeyCode::MediaTrackPrevious),
];

/// The name a position carries on a standard keyboard.
///
/// This is what a layout falls back to when it cannot name a key itself. Only the positions whose
/// meaning is a name are here: a letter, a digit and a keypad digit all answer nothing, because
/// what they mean is a character and a layout is what says which one.
const NAMES: &[(KeyCode, NamedKey)] = &[
    (KeyCode::Escape, NamedKey::Escape),
    (KeyCode::Enter, NamedKey::Enter),
    (KeyCode::NumpadEnter, NamedKey::Enter),
    (KeyCode::Tab, NamedKey::Tab),
    (KeyCode::Space, NamedKey::Space),
    (KeyCode::Backspace, NamedKey::Backspace),
    (KeyCode::Delete, NamedKey::Delete),
    (KeyCode::Insert, NamedKey::Insert),
    (KeyCode::Home, NamedKey::Home),
    (KeyCode::End, NamedKey::End),
    (KeyCode::PageUp, NamedKey::PageUp),
    (KeyCode::PageDown, NamedKey::PageDown),
    (KeyCode::ArrowUp, NamedKey::ArrowUp),
    (KeyCode::ArrowDown, NamedKey::ArrowDown),
    (KeyCode::ArrowLeft, NamedKey::ArrowLeft),
    (KeyCode::ArrowRight, NamedKey::ArrowRight),
    (KeyCode::ShiftLeft, NamedKey::Shift),
    (KeyCode::ShiftRight, NamedKey::Shift),
    (KeyCode::ControlLeft, NamedKey::Control),
    (KeyCode::ControlRight, NamedKey::Control),
    // Both alt keys are alt here. Whether the right one reaches a third level is the layout's
    // decision, and the layout reports it as a held modifier rather than as this key's name.
    (KeyCode::AltLeft, NamedKey::Alt),
    (KeyCode::AltRight, NamedKey::Alt),
    // Super rather than meta, because that is what the key beside the space bar is called on both
    // backends. The vocabulary names each of the two, and a shortcut written against one of them
    // has to find the same name whichever backend ran.
    (KeyCode::MetaLeft, NamedKey::Super),
    (KeyCode::MetaRight, NamedKey::Super),
    (KeyCode::ContextMenu, NamedKey::ContextMenu),
    (KeyCode::CapsLock, NamedKey::CapsLock),
    (KeyCode::NumLock, NamedKey::NumLock),
    (KeyCode::ScrollLock, NamedKey::ScrollLock),
    (KeyCode::PrintScreen, NamedKey::PrintScreen),
    (KeyCode::Pause, NamedKey::Pause),
    (KeyCode::Help, NamedKey::Help),
    (KeyCode::F1, NamedKey::F1),
    (KeyCode::F2, NamedKey::F2),
    (KeyCode::F3, NamedKey::F3),
    (KeyCode::F4, NamedKey::F4),
    (KeyCode::F5, NamedKey::F5),
    (KeyCode::F6, NamedKey::F6),
    (KeyCode::F7, NamedKey::F7),
    (KeyCode::F8, NamedKey::F8),
    (KeyCode::F9, NamedKey::F9),
    (KeyCode::F10, NamedKey::F10),
    (KeyCode::F11, NamedKey::F11),
    (KeyCode::F12, NamedKey::F12),
    (KeyCode::F13, NamedKey::F13),
    (KeyCode::F14, NamedKey::F14),
    (KeyCode::F15, NamedKey::F15),
    (KeyCode::F16, NamedKey::F16),
    (KeyCode::F17, NamedKey::F17),
    (KeyCode::F18, NamedKey::F18),
    (KeyCode::F19, NamedKey::F19),
    (KeyCode::F20, NamedKey::F20),
    (KeyCode::F21, NamedKey::F21),
    (KeyCode::F22, NamedKey::F22),
    (KeyCode::F23, NamedKey::F23),
    (KeyCode::F24, NamedKey::F24),
    (KeyCode::AudioVolumeDown, NamedKey::AudioVolumeDown),
    (KeyCode::AudioVolumeMute, NamedKey::AudioVolumeMute),
    (KeyCode::AudioVolumeUp, NamedKey::AudioVolumeUp),
    (KeyCode::MediaPlayPause, NamedKey::MediaPlayPause),
    (KeyCode::MediaStop, NamedKey::MediaStop),
    (KeyCode::MediaTrackNext, NamedKey::MediaTrackNext),
    (KeyCode::MediaTrackPrevious, NamedKey::MediaTrackPrevious),
];

/// Returns where `key` sits, in the standard vocabulary.
///
/// A code the vocabulary has no position for keeps the kernel's own number, so a keyboard with an
/// extra key still has something to bind to it. The number is the kernel's rather than a hash of
/// anything: every code a device can report is under seven hundred and sixty-eight, and this
/// backend reads no other numbering, so there is nothing for it to collide with.
pub(crate) fn physical(key: Key) -> PhysicalKey {
    POSITIONS.iter().find(|(code, _)| *code == key).map_or(
        PhysicalKey::Unidentified(u32::from(key.raw())),
        |(_, at)| PhysicalKey::Code(*at),
    )
}

/// Returns the name this position carries on a standard keyboard, when it carries one.
pub(crate) fn name(at: PhysicalKey) -> Option<NamedKey> {
    let at = at.code()?;
    NAMES
        .iter()
        .find(|(code, _)| *code == at)
        .map(|(_, named)| *named)
}

#[cfg(test)]
mod tests {
    //! The two tables, as tables.
    //!
    //! Spot values catch a row that was transcribed wrong. They do not catch a row that is missing,
    //! a row that is there twice, or a position two codes both claim — and each of those is a key
    //! that quietly answers for another one. So the properties are what is asserted, over the whole
    //! table, and the spot values are the few rows worth naming on top of that.

    use super::{NAMES, POSITIONS, name, physical};
    use zgui_evdev::Key;
    use zgui_vocab::{KeyCode, NamedKey, PhysicalKey};

    #[test]
    fn every_position_the_vocabulary_names_has_a_kernel_code() {
        // The vocabulary is the smaller set and every one of its positions is on a keyboard the
        // kernel drives, so a position with no row here is one a program on this backend can never
        // be told about.
        for code in KeyCode::ALL {
            assert!(
                POSITIONS.iter().any(|(_, at)| at == code),
                "{code:?} is in the vocabulary and no kernel code reaches it"
            );
        }
    }

    #[test]
    fn no_two_kernel_codes_claim_one_position() {
        for (index, (code, at)) in POSITIONS.iter().enumerate() {
            for (other, other_at) in &POSITIONS[index + 1..] {
                assert_ne!(at, other_at, "{code:?} and {other:?} claim one position");
                assert_ne!(
                    code, other,
                    "{code:?} is in the table twice, and the second row is unreachable"
                );
            }
        }
    }

    #[test]
    fn every_code_the_table_claims_round_trips_to_the_position_it_claims() {
        for (code, at) in POSITIONS {
            assert_eq!(
                physical(*code),
                PhysicalKey::Code(*at),
                "{code:?} did not cross to {at:?}"
            );
        }
    }

    #[test]
    fn the_rows_a_binding_is_written_against_are_the_rows_they_say_they_are() {
        // A keyboard's own numbering, read out of `input-event-codes.h`. A table shifted by one is
        // a keyboard that types its neighbour, and these are the numbers that would say so.
        let pairs = [
            (Key::KEY_A, 30, KeyCode::KeyA),
            (Key::KEY_Z, 44, KeyCode::KeyZ),
            (Key::KEY_ESC, 1, KeyCode::Escape),
            (Key::KEY_ENTER, 28, KeyCode::Enter),
            (Key::KEY_LEFTSHIFT, 42, KeyCode::ShiftLeft),
            (Key::KEY_RIGHTSHIFT, 54, KeyCode::ShiftRight),
            (Key::KEY_SPACE, 57, KeyCode::Space),
            (Key::KEY_LEFTMETA, 125, KeyCode::MetaLeft),
            (Key::KEY_102ND, 86, KeyCode::IntlBackslash),
            (Key::KEY_KPENTER, 96, KeyCode::NumpadEnter),
            (Key::KEY_F12, 88, KeyCode::F12),
        ];
        for (code, number, at) in pairs {
            assert_eq!(code.raw(), number, "{code:?} is not code {number}");
            assert_eq!(physical(code), PhysicalKey::Code(at));
        }
    }

    #[test]
    fn a_position_is_where_a_key_is_rather_than_what_it_types() {
        // The one assertion that says this table is layout-free. `KEY_Y` is where `z` sits on a
        // German keyboard, and it is `KeyY` on every keyboard there is.
        assert_eq!(physical(Key::KEY_Y), PhysicalKey::Code(KeyCode::KeyY));
        assert_eq!(physical(Key::KEY_Q), PhysicalKey::Code(KeyCode::KeyQ));
    }

    #[test]
    fn a_position_the_vocabulary_has_no_name_for_keeps_the_kernel_number() {
        // A programmable key on a gaming keyboard, a laptop's own function key, and a code from a
        // kernel newer than this table. Each stays bindable, and each stays itself.
        for code in [Key::KEY_MACRO27, Key::KEY_FN_F1, Key::new(0x2ff)] {
            assert_eq!(
                physical(code),
                PhysicalKey::Unidentified(u32::from(code.raw())),
                "{code:?} lost its number"
            );
        }
    }

    #[test]
    fn a_position_that_carries_a_name_answers_with_it() {
        // What a console keymap cannot say. Escape, enter and the arrows are actions the console
        // driver takes on itself, so a layout reading that keymap has nothing to call them.
        let pairs = [
            (KeyCode::Escape, NamedKey::Escape),
            (KeyCode::Enter, NamedKey::Enter),
            (KeyCode::NumpadEnter, NamedKey::Enter),
            (KeyCode::ArrowUp, NamedKey::ArrowUp),
            (KeyCode::ShiftRight, NamedKey::Shift),
            (KeyCode::F7, NamedKey::F7),
        ];
        for (at, named) in pairs {
            assert_eq!(name(PhysicalKey::Code(at)), Some(named));
        }
    }

    #[test]
    fn a_position_whose_meaning_is_a_character_carries_no_name() {
        // A letter, a digit and a keypad digit all mean whatever the layout puts on them, so
        // naming one here would type it in place of what the person pressed.
        for at in [KeyCode::KeyA, KeyCode::Digit1, KeyCode::Numpad5] {
            assert_eq!(name(PhysicalKey::Code(at)), None, "{at:?} was named");
        }
        assert_eq!(
            name(PhysicalKey::Unidentified(0x2ff)),
            None,
            "a position the vocabulary does not name has no name to carry"
        );
    }

    #[test]
    fn no_position_is_given_two_names() {
        for (index, (at, named)) in NAMES.iter().enumerate() {
            for (other, other_named) in &NAMES[index + 1..] {
                assert_ne!(
                    at, other,
                    "{at:?} is named twice, as {named:?} and as {other_named:?}"
                );
            }
        }
    }
}
