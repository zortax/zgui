//! What a layout-affecting change costs this pipeline.
//!
//! # The shape of the answer
//!
//! Three questions, asked widest first, and the first one to answer "yes, that moved" is the whole
//! answer. Did anything that decides *which boxes exist* move? Did anything that decides where
//! those boxes are and how large they are move? If neither, what did move — the shape of the area
//! the element covers, or only the colours drawn into it?
//!
//! The order is what makes the answer narrow rather than merely correct. A width and a `display`
//! are both "layout" to the style engine, and they are not the same work here: one throws away a
//! cached size, the other throws away a box and every box below it, and at document scale that is
//! the difference between a frame and a fifteenth of a second.
//!
//! # Why an unexplained difference is the widest answer
//!
//! The engine reaches this only after its own predicate decided the change was layout-affecting.
//! Reaching the end without having found a difference therefore means the property responsible is
//! one this classification does not name — a newer engine's property, or one that was overlooked —
//! and the only safe reading of a property nobody here knows about is that it changes everything.
//! That is what makes an omission cost time rather than correctness.

mod geometry;
mod same;
mod structure;
mod surface;

use style::properties::ComputedValues;

/// What a change from one computed style to another costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cost {
    /// The element is drawn again where it is, out of the fragments it already has.
    Repaint,
    /// The area the element covers moved, so its fragment is measured again; nothing is laid out.
    Ink,
    /// Sizes and positions have to be computed again, out of the boxes that are already there.
    Geometry,
    /// The box, its formatting context, everything below it and its text.
    Layout,
}

/// What the difference between `old` and `new` costs.
pub(super) fn cost(old: &ComputedValues, new: &ComputedValues) -> Cost {
    if !structure::unchanged(old, new) {
        return Cost::Layout;
    }
    if !geometry::unchanged(old, new) {
        return Cost::Geometry;
    }
    surface::cost(old, new)
}
