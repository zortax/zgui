//! Showing decoded video: Y′CbCr planes on zgui's device, converted to colour by zgui.
//!
//! A producer that decodes video holds planes of luma and chroma samples. It wraps them in a
//! [`VideoFrame`] with the [`ColorSpace`] the bitstream states and presents that through
//! [`SurfaceHandle::present_video`](crate::SurfaceHandle::present_video). The frame that shows
//! the surface converts the planes in one pass on the shared device, into a texture zgui owns,
//! and the decoder's buffer is free again once that pass completed.
//!
//! The planes may come from a software decoder through [`Planes::upload_triplanar`], or from a
//! hardware decoder's own memory imported into textures, with the decoder's buffer held by
//! [`VideoFrame::with_guard`].
//!
//! Standard-dynamic-range BT.709 frames convert with one matrix. Every other colour space passes
//! through linear light: HDR highlights are tone mapped with the ITU-R BT.2390 EETF, wide gamuts
//! are mapped onto BT.709, and the result is encoded the way the compositor encodes everything.

mod color;
mod convert;
mod frame;
mod params;
mod sample;
mod schedule;
#[cfg(test)]
pub(crate) mod testing;

pub use color::{ColorMatrix, ColorPrimaries, ColorRange, ColorSpace, TransferFunction};
pub(crate) use convert::Converter;
pub use frame::{PlaneData, Planes, SampleSize, UnsupportedFormat, VideoFrame};
pub use sample::{ChromaSiting, Packing, SampleDepth};
pub(crate) use schedule::Queue;
