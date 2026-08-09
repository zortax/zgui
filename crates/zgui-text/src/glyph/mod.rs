//! Glyphs: what identifies a rasterised one, what it looks like, and who makes it.

pub mod atlas;
pub mod image;
pub mod key;
pub mod outline;
pub mod owned;
pub mod pen;
pub mod raster;
pub mod route;
pub mod run;

pub use crate::glyph::atlas::AtlasGlyph;
pub use crate::glyph::image::{GlyphFormat, GlyphImage};
pub use crate::glyph::key::{GlyphKey, RasterStyle, SubpixelOffset};
pub use crate::glyph::outline::{GlyphOutline, OutlineKey};
pub use crate::glyph::owned::ShapedRunOwned;
pub use crate::glyph::pen::PenPosition;
pub use crate::glyph::raster::{GlyphRaster, NoRaster};
pub use crate::glyph::route::{ATLAS_MAX_SIZE, RasterPath, RunProfile, RunSurface};
pub use crate::glyph::run::{SYNTHETIC_BOLD_RATIO, ShapedGlyph, ShapedGlyphs, ShapedRun};
