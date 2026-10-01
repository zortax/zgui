//! Showing decoded video: Y′CbCr planes on zgui's device, converted to colour by zgui.
//!
//! A producer that decodes video holds planes of luma and chroma samples. It wraps them in a
//! [`VideoFrame`] with the [`ColorSpace`] the bitstream states and presents that through
//! [`SurfaceHandle::present_video`](crate::SurfaceHandle::present_video). The frame that shows
//! the surface converts the planes in one pass on the shared device. The planes may come from a
//! software decoder through [`Planes::upload_i420`], or from a hardware decoder's own memory
//! imported into a texture, with the decoder's buffer held by [`VideoFrame::with_guard`].

mod color;
mod convert;
mod frame;
#[cfg(test)]
pub(crate) mod testing;

pub use color::{ColorMatrix, ColorRange, ColorSpace};
pub(crate) use convert::Converter;
pub use frame::{Planes, VideoFrame};
