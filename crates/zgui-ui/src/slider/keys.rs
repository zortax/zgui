//! What each key does to a slider's value.

use zgui::vocab::{Key, NamedKey};

use crate::support::Bound;

/// How far a key moves the value, in steps, or where it puts it outright.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Move {
    /// Move this many steps, which may be negative.
    By(f64),
    /// Go to one end of the range.
    To(f64),
}

/// What `key` asks for, or nothing when the slider does not claim it.
///
/// The set is the authoring practices' own: the arrow keys move one step, page up and down move
/// ten, and home and end go to the ends. Anything else belongs to whatever is around the slider,
/// which is why a slider that swallowed every key would stop tab from leaving it.
///
/// ```
/// use zgui::vocab::{Key, NamedKey};
/// use zgui_ui::support::Bound;
/// use zgui_ui::slider::{Move, key_move};
///
/// let bound = Bound::new(0.0, 100.0, 5.0);
/// assert_eq!(key_move(&Key::Named(NamedKey::ArrowRight), bound), Some(Move::By(5.0)));
/// assert_eq!(key_move(&Key::Named(NamedKey::PageUp), bound), Some(Move::By(50.0)));
/// assert_eq!(key_move(&Key::Named(NamedKey::Home), bound), Some(Move::To(0.0)));
/// assert_eq!(key_move(&Key::Named(NamedKey::Tab), bound), None);
/// ```
#[must_use]
pub fn key_move(key: &Key, bound: Bound) -> Option<Move> {
    let step = if bound.step > 0.0 { bound.step } else { 1.0 };
    match key {
        Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => Some(Move::By(step)),
        Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => Some(Move::By(-step)),
        Key::Named(NamedKey::PageUp) => Some(Move::By(step * 10.0)),
        Key::Named(NamedKey::PageDown) => Some(Move::By(step * -10.0)),
        Key::Named(NamedKey::Home) => Some(Move::To(bound.min)),
        Key::Named(NamedKey::End) => Some(Move::To(bound.max)),
        _ => None,
    }
}
