//! Closing what is open, and deciding which open thing a press belongs to.
//!
//! [`DismissableLayer`] is one open surface: it hears about presses past itself and about Escape,
//! and reports rather than closing anything. [`LayerStack`] is the answer to *which* open surface
//! a press belongs to — exactly one, the topmost, so a popover inside a dialog is dismissed on its
//! own and the dialog behind it stays open.

mod layer;
mod stack;

pub use crate::dismiss::layer::{DismissReason, DismissableLayer, DismissableLayerProps};
pub use crate::dismiss::stack::{LayerId, LayerStack};
