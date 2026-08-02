//! What an element means, as opposed to what it looks like.
//!
//! Everything here is plain data. It describes a control to whatever is asking — a screen reader,
//! a braille display, an automation script — and it holds no callbacks, no signals and no
//! platform handles, so it can be written by a view, stored on a node and read by a projection
//! without any of the three depending on the others.

pub mod semantics;

mod builder;
mod enums;
mod role;

pub use crate::a11y::builder::A11y;
pub use crate::a11y::enums::{
    AriaCurrent, AutoComplete, HasPopup, Invalid, Live, NodeId, Orientation, SortDirection,
    TextDirection, Toggled,
};
pub use crate::a11y::role::Role;
pub use crate::a11y::semantics::Semantics;
