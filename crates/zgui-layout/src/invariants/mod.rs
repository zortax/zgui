//! The checks that say the three levels still agree with each other.
//!
//! An element, a box and a fragment are three levels of one structure, and every link between them
//! is stored twice: an element lists the boxes it generated and each box points back at it, a box
//! lists the pieces it was painted as and each piece names its box. Two records of one fact drift,
//! and when they do the symptom is a click that lands on nothing or a rectangle painted for
//! something that no longer exists — never an error at the point of the mistake.
//!
//! So the links are checked, and they are checked in every test rather than only in a suite of
//! their own: the value of an invariant is that it fails in the test that broke it. Outside tests it
//! is opt-in through `ZGUI_INVARIANTS=1`, which is what makes it usable on a real application whose
//! symptoms are hard to reduce.

pub mod levels;

pub use crate::invariants::levels::{Violation, check, check_if_enabled, enabled};
