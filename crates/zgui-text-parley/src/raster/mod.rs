//! Glyphs into pixels, and glyphs into curves.
//!
//! Which of the two a run takes is not decided here. It is a property of the run and the surface
//! it lands on — [`RasterPath`](zgui_text::RasterPath) — so that the stage that measures a run's
//! ink and the stage that draws it cannot come to different answers.

pub(crate) mod faces;
pub mod glyphs;
pub(crate) mod image;
pub(crate) mod outline;

pub use crate::raster::glyphs::Rasteriser;
