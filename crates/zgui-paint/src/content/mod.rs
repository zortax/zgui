//! The content one window draws from: its shaped lines, its decoded images and its vector tiles.
//!
//! Everything above this module describes content by name — *line four of paragraph seven*, *the
//! picture attached to this node*. This is where a name becomes what is drawn: a line becomes the
//! shaped runs the compositor rasterises, and a picture becomes a rectangle of an atlas texture.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`cache`] | [`ContentCache`], the atlas, the images registered against it, and the shaped lines |
//! | [`images`] | replaced content into atlas tiles and external textures |
//! | [`vectors`] | path notation into placed curves a rasteriser encodes once |
//!
//! # The one rule about ordering
//!
//! Tiles are allocated while the emit walk runs and uploaded in one batch afterwards, because the
//! atlas defers its writes. A frame that drew without flushing would sample texels that were never
//! written — which on most devices is not a blank tile but whatever the texture held before, so it
//! is the kind of defect that shows up as another picture's pixels rather than as nothing.
//! [`ContentCache::flush`] is what closes it, and the frame loop calls it between emitting and
//! drawing.

pub mod cache;
pub mod custom;
pub mod images;
pub mod probe;
pub mod shader;
pub mod vectors;

pub use crate::content::cache::{ContentCache, FrameContent, TileOwner};
pub use crate::content::images::{ImageError, MipLevel};
pub use crate::content::vectors::{
    AnalyticOnly, CachedMarks, Drawing, MarksOnly, NoVectorMasks, NoVectors,
    Placement as VectorPlacement, VectorCache, VectorMaskSource, VectorSource, Vectors,
};
