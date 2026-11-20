//! Paint: what fills a shape, and what strokes it.
//!
//! Paint lives in a side table rather than in the primitive, and that single decision is what lets
//! a `conic-gradient(in oklch, …)` with fifty stops cost a quad the same instance bytes as a flat
//! colour does.
//!
//! [`PaintTable`] interns by content, so two shapes that computed to the same paint share an
//! entry. An entry can also be *owned*: named after a box and one of its paints, and rewritten in
//! place while an animation moves its colour. See [`PaintEntry`].

pub mod reference;
pub mod source;
pub mod table;

#[cfg(test)]
mod tests;

pub use crate::paint::reference::{PaintKind, PaintRef};
pub use crate::paint::source::{GradientKind, Paint};
pub use crate::paint::table::{PaintEntry, PaintOwner, PaintRewrite, PaintRole, PaintTable};
