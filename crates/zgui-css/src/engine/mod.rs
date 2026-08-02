//! The types the style engine asks for and answers in, so that no crate above the document names
//! them.
//!
//! Most of the engine's surface can be kept behind this crate by re-exporting value types. Two
//! things cannot: the exact length unit the engine measures in, and the tagged geometry its
//! container-query hook answers in. Both appear in *return* position of a required method, where
//! no inference trick and no re-export through the engine itself avoids naming their crates — so
//! they are named here, once, and re-exported from [`geometry`].

pub mod geometry;
pub mod threads;

pub use crate::engine::geometry::{Au, ContainerSize, Scale, Size2D};
pub use crate::engine::threads::MAX_STYLE_THREADS;
