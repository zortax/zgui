//! Face metrics: what the cascade has to ask a font system before it can finish.

pub(crate) mod memo;
pub(crate) mod read;
mod source;

pub use crate::metrics::source::{BASE_SIZE, MONOSPACE_BASE_SIZE};
