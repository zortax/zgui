//! The application the budgets are measured against, and the script that drives it.
//!
//! A gallery: a masthead, twelve panels, twenty-eight lines of real text shaped by the system font
//! engine, and a row of swatches one of which is picked by a signal. It is the `styled` example's
//! document, because that is the one a person reported as slow, and every stage under it is the
//! real one — only the renderer is a stub, so that a measurement is this framework's own cost and
//! not a graphics driver's.

#![allow(dead_code, unreachable_pub)]

mod driver;
mod fixture;
mod renderer;

pub use crate::support::driver::Gallery;
