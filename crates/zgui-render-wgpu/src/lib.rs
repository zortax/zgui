//! Putting a display list on a screen: the device, the surface, the pipelines and the target a
//! frame is composed into.
//!
//! # What a frame passes through
//!
//! A frame is composed into a **persistent target**, never straight into the surface, and the
//! whole of that target is then copied onto the surface. Both halves of that are forced rather
//! than chosen. Every acquisition of a surface texture yields a brand-new resource marked wholly
//! uninitialised, so loading from one costs a full clear before any of this frame's commands run;
//! composing into a target that outlives the frame is what makes redrawing only part of a frame
//! possible at all, and copying all of it is what stops the rest coming out black.
//!
//! # One colour encoding, everywhere
//!
//! Compositing, blending and filtering all happen on premultiplied, gamma-encoded values, in every
//! target. That rule has teeth only if the attachment formats agree with it: an `*Srgb` attachment
//! format is a fixed-function decode before every blend and an encode after it, so it silently
//! moves every blend into linear light however the shaders are written. The difference is
//! measurable and large — `rgba(128, 128, 128, 0.5)` over white reads back 191 from a plain
//! attachment, which is what CSS specifies, and 225 from an encoded one — and no image comparison
//! can see it, because when the target and the surface are both encoded the copy between them
//! round-trips to identity. So [`gpu::formats`] pins the surface to an unencoded format, the
//! composed target takes the surface's format with any encoding suffix removed, and where an
//! adapter offers nothing unencoded the encode is either bypassed through a second view of the
//! surface or cancelled in the copy.
//!
//! # Getting one
//!
//! ```no_run
//! use zgui_geom::{Scale, Size};
//! use zgui_render::RenderTarget;
//! use zgui_render_wgpu::Builder;
//!
//! let target = RenderTarget::new(Size::new(256, 256), Scale::new(1.0));
//! // With no window, a texture stands in for the surface — configured by the same format rules,
//! // so what is drawn through it is what a window would have shown.
//! let renderer = Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false)?;
//! # Ok::<(), zgui_render::GpuUnavailable>(())
//! ```
//!
//! When no adapter survives, that is a typed failure naming every one that was tried, and not a
//! quiet fallback: a window that appears and never paints is a worse outcome than a program that
//! says why it will not start.

#![deny(missing_docs)]
#![allow(unsafe_code)]

pub mod atlas_backend;
pub mod bind;
pub mod buffer;
pub mod filter;
pub mod frame;
pub mod gpu;
pub mod pipeline;
pub mod renderer;
pub mod shader;
pub mod target;

/// The graphics API this renderer is written against, re-exported so a caller naming a surface
/// format or creating a surface uses the same version of it that the renderer does.
pub use wgpu;

pub use crate::bind::globals::SubpixelOrder;
pub use crate::gpu::device::Gpu;
pub use crate::gpu::formats::{Formats, SrgbTier};
pub use crate::renderer::WgpuRenderer;
pub use crate::renderer::builder::Builder;
pub use crate::renderer::readback::Pixels;
pub use crate::renderer::shared::SharedGraphics;
pub use crate::target::acquire::{Acquisition, SurfaceAction};
pub use crate::target::group_pool::GroupPool;
pub use crate::target::scale::TargetScale;
