//! The content one window draws from: its shaped lines, its decoded images and its vector content.
//!
//! Everything above this module describes content by name, such as *line four of paragraph seven*
//! or *the picture attached to this node*. This is where a name becomes what is drawn: a line
//! becomes the shaped runs the compositor rasterises, and a picture becomes the texels of a
//! compositor image.
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`cache`] | [`ContentCache`], the images attached to replaced nodes, the vector caches and the shaped lines |
//! | [`images`] | replaced content as decoded texels and external textures |
//! | [`vectors`] | path notation into placed shapes, route recognitions and CPU layers |

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
