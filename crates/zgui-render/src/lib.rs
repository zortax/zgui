//! What it means to draw a scene, and what it means to rasterise the vector parts of one.
//!
//! Two contracts live here and nothing implements either of them. [`Renderer`] takes a finished
//! display list and a damage set and puts pixels somewhere; [`VectorRaster`] takes the pass plan the
//! display list already carries and produces coverage for it. Keeping both in one crate that names
//! no graphics API is what makes either of them replaceable — a second renderer, a capture
//! implementation used by tests, a second path rasteriser to fall back to when the first is
//! unavailable.
//!
//! # What a frame reports
//!
//! [`Renderer::draw`] returns a [`FrameOutcome`], not a `Result`. A frame that did not reach the
//! screen is not usually an error: the window was occluded, the surface was resized underneath it,
//! the compositor timed out. What the caller actually has to know is whether the work was submitted,
//! because **damage is retired when the frame's work was submitted, not when a frame was
//! presented** — a frame that drew everything and then failed to acquire a surface has still
//! updated the target it drew into, and redrawing it would be redundant.
//!
//! # What is not here
//!
//! Where vector passes fall, how many there are and what each one clips through are decided in the
//! display list, before any renderer sees it. A [`VectorRaster`] executes that plan and allocates
//! its own resources for it; it does not derive one, and it must not perform a damage cull of its
//! own. Two owners of one decision is how the two come to disagree, and a pass count would stop
//! being something a test could assert without a device.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod capabilities;
pub mod memory;
pub mod outcome;
pub mod pool;
pub mod renderer;
pub mod shift;
pub mod target;
pub mod texture;
pub mod vector;

#[cfg(test)]
mod tests;

pub use crate::capabilities::RenderCapabilities;
pub use crate::memory::MemoryReport;
pub use crate::outcome::{FrameOutcome, FrameStats, GpuUnavailable, RejectedAdapter, SkipReason};
pub use crate::pool::TargetPoolReport;
pub use crate::renderer::Renderer;
pub use crate::shift::ScrollShift;
pub use crate::target::RenderTarget;
pub use crate::texture::{ExternalTexture, TextureHandle};
pub use crate::vector::{
    Decay, Extent, Layering, VectorError, VectorFrame, VectorPass, VectorPlan, VectorRaster,
    VectorTarget,
};
