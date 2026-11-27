//! Glyphs: what a shaped line hands to whoever draws it.

pub mod owned;
pub mod run;

pub use crate::glyph::owned::ShapedRunOwned;
pub use crate::glyph::run::{
    SUBPIXEL_MAX_SIZE, SYNTHETIC_BOLD_RATIO, ShapedGlyph, ShapedGlyphs, ShapedRun,
};
