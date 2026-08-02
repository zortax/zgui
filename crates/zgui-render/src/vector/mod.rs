//! Rasterising the vector parts of a scene into coverage a compositing draw can read.

pub mod decay;
pub mod error;
pub mod frame;
pub mod pack;
pub mod pass;
pub mod plan;
pub mod raster;
pub mod target;

pub use crate::vector::decay::{Decay, Extent};
pub use crate::vector::error::VectorError;
pub use crate::vector::frame::VectorFrame;
pub use crate::vector::pack::Layering;
pub use crate::vector::pass::VectorPass;
pub use crate::vector::plan::VectorPlan;
pub use crate::vector::raster::VectorRaster;
pub use crate::vector::target::VectorTarget;
