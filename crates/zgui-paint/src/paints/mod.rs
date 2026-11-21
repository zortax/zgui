//! The paint sources boxes and shapes are filled and stroked with.
//!
//! [`PaintTable`] interns paints by content, so two shapes that computed to the same paint share an
//! entry. An owned entry is named after a box and one of its paints instead, and an animation
//! rewrites it in place. See [`PaintEntry`].

mod reference;
mod source;
mod table;

pub use crate::paints::reference::{PaintKind, PaintRef};
pub use crate::paints::source::{GradientKind, Paint};
pub use crate::paints::table::{PaintEntry, PaintOwner, PaintRewrite, PaintRole, PaintTable};
