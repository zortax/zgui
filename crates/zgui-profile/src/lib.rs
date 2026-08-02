//! Frame instrumentation: named stages and the counters that say what each one did.
//!
//! Two things live here, and they answer different questions.
//!
//! [`Phase`] is the fixed taxonomy of frame stages, each with a `tracing` span. It answers *where
//! did the time go*, and it is fixed so that two recordings are comparable.
//!
//! [`counter`] is a block of per-frame counters. It answers *how much work was done*, in numbers
//! rather than durations, which is what makes it usable in an assertion: a timing is a property of
//! the machine, while "one row hovered, one row restyled" is a property of the design and stays
//! true on a slow machine, a fast one, and under a debugger.
//!
//! ```
//! use zgui_profile::{Counter, Phase, counter};
//!
//! let frame = zgui_profile::phase::frame(0);
//! let _frame = frame.enter();
//!
//! {
//!     let _stage = Phase::Restyle.span().entered();
//!     counter::bump(Counter::ElementsRestyled);
//! }
//!
//! if zgui_profile::COUNTERS_ENABLED {
//!     assert_eq!(counter::get(Counter::ElementsRestyled), 1);
//! }
//! # counter::reset();
//! ```
//!
//! # Why the counters are a real dependency
//!
//! The counters are compiled out of an optimised build, so they cost a release binary nothing.
//! They are switched by a feature rather than by the debug-assertion flag alone, because a test
//! harness has to be able to read them from an optimised build, and because instrumentation that
//! only exists in debug builds is instrumentation that quietly rots. [`COUNTERS_ENABLED`] says
//! which way a given build was compiled.
//!
//! # Which counters mean anything
//!
//! A counter incremented by a stage that does not know what draws the result means the same thing
//! under every renderer, including a stub that draws nothing. A counter incremented by the GPU
//! renderer reads zero under such a stub, and asserting on it there asserts nothing. Which is
//! which is recorded per counter as its [`Group`], so a test harness can be honest about what its
//! assertions cover.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod counter;
pub mod latency;
pub mod phase;

pub use crate::counter::{COUNTERS_ENABLED, Counter, Counters, Group};
pub use crate::phase::Phase;
