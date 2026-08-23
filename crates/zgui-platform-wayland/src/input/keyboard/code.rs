//! Where a key sits, from the number the kernel gives it.

use zgui_vocab::{KeyCode, PhysicalKey};

/// The position `scancode` names, in the standard vocabulary.
///
/// The number is the kernel's own code for the key — the same one every input device on this
/// system reports — and the answer is where that key sits on a keyboard laid out in the standard
/// way, whatever the layout in force makes it type. A key this table has no name for keeps its
/// number, so it is still bindable even though nothing can say what it is.
///
/// The table is written out rather than derived because the two numberings agree about nothing:
/// the kernel's order is the order the codes were assigned in, and the standard's names describe a
/// keyboard. It covers exactly the positions the vocabulary names — a keyboard, its numeric pad,
/// its function row and the media keys — and the rest of the kernel's several hundred codes are
/// deliberately not invented names for.
pub fn physical(scancode: u32) -> PhysicalKey {
    match code(scancode) {
        Some(code) => PhysicalKey::Code(code),
        None => PhysicalKey::Unidentified(scancode),
    }
}

/// The named position, when the vocabulary has one.
#[expect(clippy::too_many_lines, reason = "one line per key on a keyboard")]
const fn code(scancode: u32) -> Option<KeyCode> {
    // Names and numbers from `linux/input-event-codes.h`, in its order.
    Some(match scancode {
        1 => KeyCode::Escape,
        2 => KeyCode::Digit1,
        3 => KeyCode::Digit2,
        4 => KeyCode::Digit3,
        5 => KeyCode::Digit4,
        6 => KeyCode::Digit5,
        7 => KeyCode::Digit6,
        8 => KeyCode::Digit7,
        9 => KeyCode::Digit8,
        10 => KeyCode::Digit9,
        11 => KeyCode::Digit0,
        12 => KeyCode::Minus,
        13 => KeyCode::Equal,
        14 => KeyCode::Backspace,
        15 => KeyCode::Tab,
        16 => KeyCode::KeyQ,
        17 => KeyCode::KeyW,
        18 => KeyCode::KeyE,
        19 => KeyCode::KeyR,
        20 => KeyCode::KeyT,
        21 => KeyCode::KeyY,
        22 => KeyCode::KeyU,
        23 => KeyCode::KeyI,
        24 => KeyCode::KeyO,
        25 => KeyCode::KeyP,
        26 => KeyCode::BracketLeft,
        27 => KeyCode::BracketRight,
        28 => KeyCode::Enter,
        29 => KeyCode::ControlLeft,
        30 => KeyCode::KeyA,
        31 => KeyCode::KeyS,
        32 => KeyCode::KeyD,
        33 => KeyCode::KeyF,
        34 => KeyCode::KeyG,
        35 => KeyCode::KeyH,
        36 => KeyCode::KeyJ,
        37 => KeyCode::KeyK,
        38 => KeyCode::KeyL,
        39 => KeyCode::Semicolon,
        40 => KeyCode::Quote,
        41 => KeyCode::Backquote,
        42 => KeyCode::ShiftLeft,
        43 => KeyCode::Backslash,
        44 => KeyCode::KeyZ,
        45 => KeyCode::KeyX,
        46 => KeyCode::KeyC,
        47 => KeyCode::KeyV,
        48 => KeyCode::KeyB,
        49 => KeyCode::KeyN,
        50 => KeyCode::KeyM,
        51 => KeyCode::Comma,
        52 => KeyCode::Period,
        53 => KeyCode::Slash,
        54 => KeyCode::ShiftRight,
        55 => KeyCode::NumpadMultiply,
        56 => KeyCode::AltLeft,
        57 => KeyCode::Space,
        58 => KeyCode::CapsLock,
        59 => KeyCode::F1,
        60 => KeyCode::F2,
        61 => KeyCode::F3,
        62 => KeyCode::F4,
        63 => KeyCode::F5,
        64 => KeyCode::F6,
        65 => KeyCode::F7,
        66 => KeyCode::F8,
        67 => KeyCode::F9,
        68 => KeyCode::F10,
        69 => KeyCode::NumLock,
        70 => KeyCode::ScrollLock,
        71 => KeyCode::Numpad7,
        72 => KeyCode::Numpad8,
        73 => KeyCode::Numpad9,
        74 => KeyCode::NumpadSubtract,
        75 => KeyCode::Numpad4,
        76 => KeyCode::Numpad5,
        77 => KeyCode::Numpad6,
        78 => KeyCode::NumpadAdd,
        79 => KeyCode::Numpad1,
        80 => KeyCode::Numpad2,
        81 => KeyCode::Numpad3,
        82 => KeyCode::Numpad0,
        83 => KeyCode::NumpadDecimal,
        86 => KeyCode::IntlBackslash,
        87 => KeyCode::F11,
        88 => KeyCode::F12,
        89 => KeyCode::IntlRo,
        92 => KeyCode::Convert,
        93 => KeyCode::KanaMode,
        94 => KeyCode::NonConvert,
        96 => KeyCode::NumpadEnter,
        97 => KeyCode::ControlRight,
        98 => KeyCode::NumpadDivide,
        99 => KeyCode::PrintScreen,
        100 => KeyCode::AltRight,
        102 => KeyCode::Home,
        103 => KeyCode::ArrowUp,
        104 => KeyCode::PageUp,
        105 => KeyCode::ArrowLeft,
        106 => KeyCode::ArrowRight,
        107 => KeyCode::End,
        108 => KeyCode::ArrowDown,
        109 => KeyCode::PageDown,
        110 => KeyCode::Insert,
        111 => KeyCode::Delete,
        113 => KeyCode::AudioVolumeMute,
        114 => KeyCode::AudioVolumeDown,
        115 => KeyCode::AudioVolumeUp,
        117 => KeyCode::NumpadEqual,
        119 => KeyCode::Pause,
        121 => KeyCode::NumpadComma,
        124 => KeyCode::IntlYen,
        125 => KeyCode::MetaLeft,
        126 => KeyCode::MetaRight,
        127 => KeyCode::ContextMenu,
        138 => KeyCode::Help,
        163 => KeyCode::MediaTrackNext,
        164 => KeyCode::MediaPlayPause,
        165 => KeyCode::MediaTrackPrevious,
        166 => KeyCode::MediaStop,
        183 => KeyCode::F13,
        184 => KeyCode::F14,
        185 => KeyCode::F15,
        186 => KeyCode::F16,
        187 => KeyCode::F17,
        188 => KeyCode::F18,
        189 => KeyCode::F19,
        190 => KeyCode::F20,
        191 => KeyCode::F21,
        192 => KeyCode::F22,
        193 => KeyCode::F23,
        194 => KeyCode::F24,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{code, physical};
    use zgui_vocab::{KeyCode, PhysicalKey};

    #[test]
    fn a_key_is_named_by_where_it_sits_and_not_by_what_it_types() {
        // On a Dvorak layout the key at this position types a full stop. It is still `KeyE`.
        assert_eq!(code(18), Some(KeyCode::KeyE));
        assert_eq!(physical(18), PhysicalKey::Code(KeyCode::KeyE));
    }

    #[test]
    fn a_key_this_table_has_no_name_for_keeps_its_number() {
        // Still bindable, which is the point: a programmable key on a gaming keyboard is a key.
        assert_eq!(code(700), None);
        assert_eq!(physical(700), PhysicalKey::Unidentified(700));
    }

    #[test]
    fn the_two_of_each_paired_key_are_told_apart() {
        assert_eq!(code(29), Some(KeyCode::ControlLeft));
        assert_eq!(code(97), Some(KeyCode::ControlRight));
        assert_eq!(code(42), Some(KeyCode::ShiftLeft));
        assert_eq!(code(54), Some(KeyCode::ShiftRight));
        assert_eq!(code(125), Some(KeyCode::MetaLeft));
        assert_eq!(code(126), Some(KeyCode::MetaRight));
    }

    #[test]
    fn the_numeric_pad_is_not_the_number_row() {
        // A shortcut bound to a position must not fire from the other side of the keyboard.
        assert_eq!(code(2), Some(KeyCode::Digit1));
        assert_eq!(code(79), Some(KeyCode::Numpad1));
        assert_ne!(code(2), code(79));
        assert_eq!(code(28), Some(KeyCode::Enter));
        assert_eq!(code(96), Some(KeyCode::NumpadEnter));
    }

    #[test]
    fn no_two_positions_share_a_name() {
        // A duplicate would silently make one of the two keys unbindable.
        let mut seen: Vec<KeyCode> = (0..=250).filter_map(code).collect();
        let total = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), total, "two scancodes crossed to one position");
    }
}
