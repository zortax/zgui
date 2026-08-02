//! Rasterising the vector parts of a display list with a compute-shader path renderer.
//!
//! # What this is, and what it is not
//!
//! It rasterises a *plan*. Which items survive a damage set, where one pass ends and the next
//! begins, what each pass clips through and whether it may be composited one item at a time are all
//! decided in the display list before any of this runs. Nothing here derives any of that, and in
//! particular nothing here culls against damage a second time.
//!
//! # Why a scratch texture at all
//!
//! The path renderer has exactly one entry point, it creates and submits a command encoder of its
//! own, it clears everything it is pointed at, and it never reads what was already there. It
//! therefore cannot draw into the middle of a frame. So a batch of paths lands in a scratch texture
//! and one ordinary draw of our own puts it back into the frame, inserted at exactly the right index
//! in the batch stream — which is exactly right, because submission order *is* z-order here: there
//! is no depth buffer, no stencil, and no order-independent scheme anywhere.
//!
//! The scratch holds **straight** — un-premultiplied — colour, because that is what the path
//! renderer writes; the composite premultiplies as it reads.
//!
//! # Getting one
//!
//! ```no_run
//! use zgui_geom::{Scale, Size};
//! use zgui_render::RenderTarget;
//! use zgui_render_wgpu::Builder;
//!
//! let target = RenderTarget::new(Size::new(256, 256), Scale::new(1.0));
//! let mut renderer = Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)?;
//! // Which rasteriser this is depends on what the device turned out to be able to do; a device
//! // without compute shaders gets the simpler one rather than nothing at all.
//! zgui_render_vector_vello::attach(&mut renderer, target.size);
//! # Ok::<(), zgui_render::GpuUnavailable>(())
//! ```

#![deny(missing_docs)]
#![allow(unsafe_code)]

pub mod device;
pub mod raster;
pub mod select;

pub use crate::device::SharedRenderer;
pub use crate::raster::VelloRaster;
pub use crate::select::{Choice, attach, chosen, for_device};

/// The path renderer this is written against, re-exported so a caller naming one of its types uses
/// the same version of it that this does.
pub use vello;
