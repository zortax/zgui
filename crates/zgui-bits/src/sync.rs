//! The atomic primitives every shared cell in this crate is built out of.
//!
//! Ordinary builds use the standard library's. A build with the `loom` feature uses the model
//! checker's instrumented equivalents instead, so a test can enumerate the interleavings of a
//! marking protocol rather than hope a stress loop happens to hit the bad one.

#[cfg(not(feature = "loom"))]
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "loom")]
pub(crate) use loom::sync::atomic::{AtomicU64, Ordering};
