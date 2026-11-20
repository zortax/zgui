//! What the glyph path did, in numbers.
//!
//! These are the workspace's own frame counters rather than statics of this crate's, so they are
//! compiled out of a build that did not ask for them and a shipped binary pays nothing for them.

use zgui_profile::{Counter, counter};

/// Records that `glyphs` glyphs were recorded into glyph runs.
pub(crate) fn placed(glyphs: usize) {
    counter::add(Counter::GlyphsPlaced, glyphs as u64);
}
