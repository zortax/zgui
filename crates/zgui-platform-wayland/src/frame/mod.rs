//! When to draw.
//!
//! Four questions, one per module, and they are separable because each is answered from a
//! different source. [`pace`] decides whether a redraw is delivered, from what the surface owes
//! the compositor. [`visibility`] decides whether the surface is worth drawing at all, from what
//! the compositor said about repainting it. [`timing`] says when frames actually reached the
//! screen, from the presentation feedback. [`Watchdog`] bounds how long any of it may wait, and
//! is the contract's own: a display controller owes the same report and can lose it the same way.
//!
//! [`presentation`] is where those timings come from: it is the one module here that names a
//! protocol object, because the answers it accumulates have no other source.
//!
//! Everything else is exercised against a clock a test moves rather than against a compositor.

pub mod pace;
pub mod presentation;
pub mod timing;
pub mod visibility;

pub use crate::frame::pace::Pacer;
pub use crate::frame::presentation::Presentation;
pub use crate::frame::timing::Timing;
pub use crate::frame::visibility::Visibility;
pub use zgui_platform::Watchdog;
