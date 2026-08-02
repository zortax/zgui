//! A renderer that records instead of drawing.
//!
//! It is the second implementation of the renderer contract, and it exists so that everything above
//! the renderer can be tested with no graphics device: the display list is a value, so a frame's
//! whole output can be captured as text on any machine, in any container, with no display server
//! and no driver.
//!
//! # What it is honest about
//!
//! It draws nothing, so it counts nothing a real renderer counts. Its
//! [`FrameStats`](zgui_render::FrameStats) reports zero draw calls, zero damaged pixels and zero
//! bytes uploaded — and those are exactly the three counters [`crate::counters`] refuses to let a
//! test assert on, because zero there means "nobody drew" and not "nothing was drawn".
//!
//! What it *does* report is the vector-pass count, because that is a decision the scene made before
//! any renderer saw it.

pub mod external;
pub mod renderer;

pub use crate::capture::renderer::CaptureRenderer;
