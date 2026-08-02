//! What the pointer and the caret interact with.

/// The computed value of `pointer-events`.
///
/// Inherited, so a descendant of an element that takes no pointer events computes to the same
/// value rather than having to be found by a walk up the tree.
pub use style::values::computed::ui::PointerEvents as PointerEventsValue;
