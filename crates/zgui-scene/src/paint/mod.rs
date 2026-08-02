//! Paint: what fills a shape, and what strokes it.
//!
//! Paint lives in side tables rather than in the primitive, and that single decision is what lets a
//! `conic-gradient(in oklch, …)` with fifty stops cost a quad the same instance bytes as a flat
//! colour does.
//!
//! There are two tables, not one, and the split is forced. [`PaintTable`] interns by content, so
//! two shapes that computed to the same paint share an entry. [`TextPaintTable`] does not: its
//! entries are **mutated in place** when a theme changes, so that a dark-mode toggle re-colours
//! every already-shaped paragraph without re-shaping a single string. Interning those would make a
//! paragraph whose colour came from a theme variable and one whose colour was written literally
//! share a slot, and the theme change would silently re-colour the literal one.

pub mod reference;
pub mod source;
pub mod table;
pub mod text;

#[cfg(test)]
mod tests;

pub use crate::paint::reference::{PaintKind, PaintRef};
pub use crate::paint::source::{GradientKind, Paint};
pub use crate::paint::table::PaintTable;
pub use crate::paint::text::{TextPaint, TextPaintTable};
