//! One frame: what it is split into, how it is recorded, and where it goes afterwards.
//!
//! A frame is **planned before any pass is opened**. That is not a stylistic preference: a live
//! render pass holds the command encoder mutably borrowed, so a group beginning, a copy between
//! targets or a capture of what lies beneath something cannot happen while one is alive — and
//! those are exactly the points a frame has to be cut at. Discarding the borrow would compile and
//! would hide a real ordering constraint; planning first makes it a value that can be read,
//! asserted and printed.
//!
//! The other half of a frame is which pixels it redraws. A frame composes into a target that
//! outlives it, so pixels outside the damage rectangles are last frame's and need no draw at all.
//! Damage therefore only ever scissors that target — never the surface, which is a brand-new
//! wholly uninitialised resource on every acquisition and comes out black everywhere a partial
//! copy did not write.

pub mod build;
pub mod damage;
pub mod fault;
pub mod pass;
pub mod plan;
pub mod present;
pub mod segment;
pub mod target;
pub mod vector;
