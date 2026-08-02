//! What has to be redrawn, and how the set of it grows before anything is drawn.

pub mod accumulate;
pub mod ink;
pub mod marks;

pub use crate::damage::accumulate::{Expansion, expand, vacated};
pub use crate::damage::ink::{ReadExtent, cull_rect, read_extent_of};
