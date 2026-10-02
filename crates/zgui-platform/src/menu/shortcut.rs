//! The chord that picks a menu entry from the keyboard.

use zgui_vocab::{Key, Modifiers};

/// A key and the modifiers held with it.
///
/// The key is the one the user reads off the keycap, lowercase for a letter. Shift is a modifier
/// of its own.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Shortcut {
    /// The key.
    pub key: Key,
    /// The modifiers held with it.
    pub modifiers: Modifiers,
}

impl Shortcut {
    /// `key` with `modifiers` held.
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }

    /// Whether a press of `key`, read without modifiers, with `modifiers` held is this chord.
    pub fn matches(&self, key: &Key, modifiers: Modifiers) -> bool {
        if modifiers != self.modifiers {
            return false;
        }
        match (&self.key, key) {
            (Key::Character(own), Key::Character(other)) => own.eq_ignore_ascii_case(other),
            (own, other) => own == other,
        }
    }
}
