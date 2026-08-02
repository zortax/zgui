//! The properties a change to which costs this pipeline a layout and no more.
//!
//! Every one of them is a property the style engine's own relayout predicate names, so nothing here
//! is a guess about what the engine considers layout-affecting: the set is the engine's, and what
//! is done with it is ours. The properties the engine names that this pipeline does *not* lay
//! anything out for are the ones that are missing, and they are accounted for in
//! [`surface`](super::surface) rather than dropped.
//!
//! What separates these from [`structure`](super::structure) is that the boxes stay: a box holds a
//! clone of its element's style, that clone is refreshed on the frame the cascade moved it, and
//! every number here is read out of the clone each time the box is measured. So the answer is to
//! throw the cached measurement away, not the box.

mod boxes;
mod text;

use style::properties::ComputedValues;

/// Whether nothing that this pipeline measures a box with has changed.
pub(super) fn unchanged(old: &ComputedValues, new: &ComputedValues) -> bool {
    boxes::unchanged(old, new) && text::unchanged(old, new)
}
