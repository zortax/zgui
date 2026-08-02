//! What the glyph path did, in numbers.
//!
//! Two questions, and the ratio between the answers is the whole point: how many glyphs a frame put
//! on the screen, and how many of those it had to turn into pixels. A repaint of text that has not
//! changed places every one of its glyphs and rasterises none of them, so the second number is what
//! a budget is written against.
//!
//! These are the workspace's own frame counters rather than statics of this crate's, so they are
//! compiled out of a build that did not ask for them and a shipped binary pays nothing for them.

use zgui_profile::{Counter, counter};

/// Records that one glyph was positioned on the surface.
pub(crate) fn placed() {
    counter::bump(Counter::GlyphsPlaced);
}

/// Records that one glyph was handed to the rasteriser.
pub(crate) fn rastered() {
    counter::bump(Counter::GlyphsRasterised);
}
