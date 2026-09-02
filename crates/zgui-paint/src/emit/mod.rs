//! Pushing primitives, one kind of thing at a time.
//!
//! Each module here knows how to draw one kind of content and nothing about when it is drawn:
//! sequencing is [`walk`](crate::walk)'s, and keeping the two apart is what lets the painting order
//! be stated once, in one place, rather than implied by the order of a dozen calls.

pub mod box_;
pub mod group;
pub mod highlight;
pub mod paint;
pub mod replaced;
pub mod scrollbar;
pub mod shader;
pub mod text;
pub mod vector;

pub use crate::emit::box_::BoxPlacement;
pub use crate::emit::group::Isolation;
pub use crate::emit::highlight::{
    Highlight, HighlightLayer, HighlightRequest, HighlightSource, NoHighlights,
};
pub use crate::emit::replaced::{ReplacedPlacement, Source};
pub use crate::emit::scrollbar::ScrollbarPaint;
pub use crate::emit::text::{
    DecorationStyle, GlyphRun, GlyphSource, NoGlyphs, PlacedGlyph, RunContent, TextPlacement,
};
pub use crate::emit::vector::ShapePaint;
