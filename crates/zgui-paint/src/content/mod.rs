//! The rasterised content one window draws from: its glyph tiles and its decoded images.
//!
//! Everything above this module describes content by name — *line four of paragraph seven*, *the
//! picture attached to this node*. Everything below it draws a quad reading a rectangle of a
//! texture. This is where a name becomes a rectangle, and it is the consumer the glyph rasteriser
//! and the atlas were built for.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`cache`] | [`ContentCache`], the atlas both halves share and the images registered against it |
//! | [`glyphs`] | shaped runs into rasterised, positioned glyphs |
//! | [`images`] | replaced content into atlas tiles and external textures |
//! | [`vectors`] | path notation into placed curves a rasteriser encodes once |
//!
//! # The one rule about ordering
//!
//! Tiles are allocated while the emit walk runs and uploaded in one batch afterwards, because the
//! atlas defers its writes. A frame that drew without flushing would sample texels that were never
//! written — which on most devices is not a blank glyph but whatever the texture held before, so it
//! is the kind of defect that shows up as another glyph's pixels rather than as nothing.
//! [`ContentCache::flush`] is what closes it, and the frame loop calls it between emitting and
//! drawing.

pub mod cache;
pub mod custom;
pub mod glyphs;
pub mod images;
pub mod probe;
pub mod shader;
pub mod vectors;

pub use crate::content::cache::{ContentCache, FrameContent, TileOwner};
pub use crate::content::images::{ImageError, MipLevel};
pub use crate::content::vectors::{
    Drawing, NoVectorMasks, NoVectors, Placement as VectorPlacement, VectorCache, VectorMask,
    VectorMaskRequest, VectorMaskSource, VectorMaskStyle, VectorSource, Vectors,
};
