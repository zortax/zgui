//! A key press, under the three readings a press has to be given.
//!
//! One press answers three unrelated questions and gives three different answers, and all three
//! have to survive the crossing together or something above has to guess:
//!
//! * **what to insert** — the layout applied, the modifiers applied;
//! * **which shortcut this is** — the layout applied, the modifiers *not* applied, so that a
//!   shortcut written for one key stays on that key when a modifier would have remapped it;
//! * **which position was pressed** — no layout at all, so that a game's movement keys sit where
//!   the fingers are rather than where the letters are.

pub mod code;
pub mod key;
pub mod modifiers;

pub use crate::input::keyboard::modifiers::modifiers;

use smithay_client_toolkit::seat::keyboard::KeyEvent as WaylandKey;
use zgui_vocab::{KeyEvent, KeyLocation};

/// A press, with all three readings of it.
///
/// The compositor reports one symbol per press, already resolved through the layout and the held
/// modifiers, plus the text it produces. What it does not report is the same key with the
/// modifiers taken off, so the modified reading stands in for the unmodified one — which makes a
/// shortcut here follow the modified key, and is the honest degradation rather than a guess.
///
/// `repeat` is the compositor's own: a repeat arrives as an ordinary press, and the loop marks the
/// ones it generated from the repeat timer.
pub fn event(pressed: &WaylandKey, repeat: bool) -> KeyEvent {
    let named = key::key(pressed.keysym, pressed.utf8.as_deref());
    KeyEvent {
        key: named.clone(),
        key_without_modifiers: named,
        physical: code::physical(pressed.raw_code),
        location: location(pressed.raw_code),
        repeat,
    }
}

/// Which of several same-named keys this is.
///
/// Derived from the position rather than reported, because the protocol does not report it and the
/// position is what the answer is about: the right shift key is the one on the right of the
/// keyboard whatever it types.
pub const fn location(scancode: u32) -> KeyLocation {
    match scancode {
        // Right control, right shift, right alt, right meta.
        97 | 54 | 100 | 126 => KeyLocation::Right,
        // Left control, left shift, left alt, left meta.
        29 | 42 | 56 | 125 => KeyLocation::Left,
        // The numeric pad, the enter and the separators on it included.
        55 | 69 | 71..=83 | 96 | 98 | 117 | 121 => KeyLocation::Numpad,
        _ => KeyLocation::Standard,
    }
}

#[cfg(test)]
mod tests {
    use super::location;
    use zgui_vocab::KeyLocation;

    #[test]
    fn the_two_of_each_paired_key_are_placed_on_their_own_sides() {
        assert_eq!(location(29), KeyLocation::Left);
        assert_eq!(location(97), KeyLocation::Right);
        assert_eq!(location(42), KeyLocation::Left);
        assert_eq!(location(54), KeyLocation::Right);
    }

    #[test]
    fn the_numeric_pad_is_placed_on_the_numeric_pad() {
        assert_eq!(location(79), KeyLocation::Numpad);
        assert_eq!(location(96), KeyLocation::Numpad);
        assert_eq!(location(98), KeyLocation::Numpad);
    }

    #[test]
    fn an_ordinary_key_is_in_the_ordinary_place() {
        assert_eq!(location(30), KeyLocation::Standard);
        assert_eq!(location(28), KeyLocation::Standard);
        assert_eq!(location(2), KeyLocation::Standard);
    }
}
