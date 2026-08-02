//! Turning what the style engine computed into what the rest of the frame owes.
//!
//! | Module | Contents |
//! |---|---|
//! | [`bits`] | the damage bits above the engine's own four |
//! | [`layout_damage`] | whether a change costs a re-shape or only a re-break |
//! | [`paint_key`] | the identity a repaint is decided by |
//! | [`a11y_key`] | the identity an accessibility rebuild is decided by |
//! | [`mod@translate`] | the lattice, the first-time-cascade branch and the two key comparisons |

pub mod a11y_key;
pub mod bits;
pub mod layout_damage;
pub mod paint_key;
pub mod translate;

pub use crate::damage::layout_damage::TextWork;
pub use crate::damage::translate::{DamageSink, translate};
