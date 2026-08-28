//! Shaped runs into rasterised, positioned glyphs.
//!
//! # What is cached, and by what
//!
//! One tile per distinct rasterisation, where *distinct* means everything that changes a pixel:
//! the face, the glyph index, the size, the subpixel phase, the synthesis, and which of the three
//! coverage forms was asked for. All of it is [`GlyphKey`](zgui_text::GlyphKey), so the same letter
//! at the same phase anywhere on the page is one tile however many times it appears — and a
//! paragraph that scrolled by a whole pixel keeps every one of them, because a whole pixel does not
//! change the phase.
//!
//! Beside the tile, the glyph cache holds the two facts the tile does not carry — where the pixels sit
//! relative to the glyph's origin, and how many of them there are — and the fact that a key
//! rasterises to *no* pixels at all. Without the first, a frame that already had the tile still had
//! to rasterise the glyph to learn where to put it; without the second, every space on the page was
//! rasterised again every frame for ever. Both are the difference between a repaint costing what
//! its glyphs cost and costing what the whole document's glyphs cost.
//!
//! # The runs this module does not tile
//!
//! A run the atlas cannot serve — see [`RasterPath`](zgui_text::RasterPath) — is placed as curves
//! instead, by [`curve`], and nothing about it reaches the atlas or this cache. That is not a
//! fallback for a failure: the two paths are chosen between before either is asked for anything.
//!
//! # What a missing glyph does
//!
//! Nothing, silently, and that is deliberate at two of the three points it can happen. A face that
//! has no outline for a glyph and a glyph that rasterises to no pixels at all (a space does) each
//! produce no primitive rather than a placeholder: a frame that drew a box where a space belongs
//! would be worse than one that drew nothing.
//!
//! An atlas with no room left is the third, and it is not the same kind of answer. Room is made
//! rather than waited for — one eviction step, then the placement is asked again — because the
//! frame is the last moment at which anything knows the glyph was wanted. What the encoding
//! produces is recorded by the paint cache and replayed until the fragment itself changes, so a
//! letter dropped here is a letter dropped for the rest of the session. When even the retry cannot
//! place it, nothing is drawn and the record is refused instead; see
//! [`Encoding::complete`](crate::walk::replay::Encoding::complete).

pub(crate) mod cache;
pub mod curve;
pub(crate) mod place;

pub(crate) use crate::content::glyphs::cache::{GlyphCache, Rasterising};
pub use crate::content::glyphs::curve::OutlineGlyph;
pub(crate) use crate::content::glyphs::place::place;

use zgui_text::GlyphFormat;

/// Which of the display list's sprite kinds a rasterisation style becomes.
pub(crate) fn format_of(style: zgui_text::RasterStyle) -> GlyphFormat {
    match style {
        zgui_text::RasterStyle::Grayscale => GlyphFormat::Mono,
        zgui_text::RasterStyle::Subpixel => GlyphFormat::Subpixel,
        zgui_text::RasterStyle::Color => GlyphFormat::Color,
    }
}
