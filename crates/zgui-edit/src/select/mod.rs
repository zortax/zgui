//! Where the caret is, and what is selected.

pub mod affinity;
pub mod grapheme;
pub mod motion;
pub mod word;

use core::ops::Range;

pub use crate::select::affinity::Affinity;
pub use crate::select::motion::{Granularity, Motion};

/// A caret, or a range with a fixed end and a moving one.
///
/// Anchor and focus are kept apart rather than reduced to a range because extending a selection
/// backwards past its own start is ordinary — shift-clicking above where a drag began, or holding
/// shift and pressing left past the anchor — and a range cannot express which end the next
/// keystroke moves.
///
/// ```
/// use zgui_edit::select::Selection;
///
/// let backwards = Selection::new(7, 2);
/// assert_eq!(backwards.range(), 2..7);
/// assert!(!backwards.is_caret());
/// assert_eq!(backwards.collapsed_to_start().focus, 2);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// The end that stays put while the selection is extended.
    pub anchor: usize,
    /// The end that moves, and where the caret is drawn.
    pub focus: usize,
    /// Which side of the focus the caret belongs to.
    pub affinity: Affinity,
}

impl Selection {
    /// A selection from `anchor` to `focus`.
    pub const fn new(anchor: usize, focus: usize) -> Self {
        Self {
            anchor,
            focus,
            affinity: Affinity::Upstream,
        }
    }

    /// A caret at `offset`, selecting nothing.
    pub const fn caret(offset: usize) -> Self {
        Self::new(offset, offset)
    }

    /// The same selection with a different affinity.
    pub const fn with_affinity(mut self, affinity: Affinity) -> Self {
        self.affinity = affinity;
        self
    }

    /// The selected range, in ascending order.
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.focus)..self.anchor.max(self.focus)
    }

    /// The lower end.
    pub fn start(&self) -> usize {
        self.anchor.min(self.focus)
    }

    /// The upper end.
    pub fn end(&self) -> usize {
        self.anchor.max(self.focus)
    }

    /// Whether nothing is selected.
    pub fn is_caret(&self) -> bool {
        self.anchor == self.focus
    }

    /// The caret at the lower end.
    pub fn collapsed_to_start(&self) -> Self {
        Self {
            anchor: self.start(),
            focus: self.start(),
            affinity: self.affinity,
        }
    }

    /// The caret at the upper end.
    pub fn collapsed_to_end(&self) -> Self {
        Self {
            anchor: self.end(),
            focus: self.end(),
            affinity: self.affinity,
        }
    }

    /// The same selection with the focus moved, keeping the anchor when `extend` is set and
    /// collapsing onto the focus when it is not.
    pub fn moved_to(&self, focus: usize, extend: bool) -> Self {
        Self {
            anchor: if extend { self.anchor } else { focus },
            focus,
            affinity: self.affinity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Affinity, Selection};

    #[test]
    fn a_backwards_selection_reports_an_ascending_range() {
        let selection = Selection::new(9, 4);
        assert_eq!(selection.range(), 4..9);
        assert_eq!(selection.start(), 4);
        assert_eq!(selection.end(), 9);
    }

    #[test]
    fn moving_without_extending_drops_the_anchor_and_moving_with_it_keeps_it() {
        let selection = Selection::new(2, 5);
        assert_eq!(selection.moved_to(8, true), Selection::new(2, 8));
        assert_eq!(selection.moved_to(8, false), Selection::caret(8));
    }

    #[test]
    fn affinity_survives_a_collapse_because_the_caret_is_still_at_a_boundary() {
        let selection = Selection::new(2, 5).with_affinity(Affinity::Downstream);
        assert_eq!(
            selection.collapsed_to_end().affinity,
            Affinity::Downstream,
            "which line the caret is drawn on does not change by collapsing onto it"
        );
    }
}
