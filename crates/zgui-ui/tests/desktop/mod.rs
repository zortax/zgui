//! Driving a component over the platform seam, with the whole framework underneath.
//!
//! # Why this exists beside the view-level harness
//!
//! Every other assertion about an overlay in this package sends an [`EventKind::Click`] straight at
//! the trigger node. That answers *given a click, does the surface open* — and says nothing at all
//! about whether pressing and releasing a real pointer over that trigger ever becomes a click. The
//! two are different questions, they are answered by different code, and a gallery driven through
//! the compositor found the second one unanswered while every test of the first was green.
//!
//! So this opens the real application: the real router, the real hit test, the real focus and
//! activation defaults, the real cascade and the real layout, over the headless platform. The only
//! thing missing is the graphics device, and nothing asserted here is about pixels.
//!
//! [`EventKind::Click`]: zgui::vocab::EventKind::Click

#![allow(
    dead_code,
    unreachable_pub,
    reason = "one support module serves several groups of assertions, none of which uses all of it"
)]

pub mod census;
pub mod grab;
pub mod reader;
pub mod renderer;
pub mod stage;
