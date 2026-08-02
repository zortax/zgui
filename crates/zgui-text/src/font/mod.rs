//! Faces: where they come from and what identifies one.

pub mod error;
pub mod face;
pub mod source;

pub use crate::font::error::FontError;
pub use crate::font::face::{FaceId, FaceRecord};
pub use crate::font::source::{FontData, FontSource};
