//! The known-unreachable set: what this build cannot do, why, and what closing it would take.
//!
//! Parity degrades visibly or it degrades silently, and silence is the failure mode that reaches
//! an author as *"I wrote CSS and nothing happened"*. Every row here is a thing this build drops
//! on the floor, stated once, with the shape of its fix — and every row carries a probe, so a row
//! that has quietly become untrue is a test failure rather than stale prose.

pub mod inherited_svg;
pub mod rows;
pub mod scrollbar_gutter;
pub mod text_decoration;

pub use crate::parity::gap::rows::{GAPS, Gap, GapProbe, GapStatus};
