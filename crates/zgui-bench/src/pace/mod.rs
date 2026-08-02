//! Pacing: whether every interval got a frame, rather than what a frame cost.
//!
//! The distinction is the whole of it. A three-millisecond frame that arrived after its interval
//! had already been repeated is a dropped frame, and every instrument that reports the cost of a
//! frame says it was a good one. So what is reported here is the interval between frames, as a
//! distribution, against the refresh of the output the run happened on — plus the two figures a
//! distribution still cannot say on its own: how many refreshes went by with nothing new on them,
//! and whether the run ended at the pace it started at.
//!
//! **This is not a gate.** It measures on the real output or it does not measure, and an output is
//! not a property of a checkout. It is run by hand at phase exit and its result is published dated;
//! what protects it between those runs is the growth check, which is counts and is therefore
//! portable.

mod load;
mod report;
mod run;

#[cfg(test)]
mod tests;

pub(crate) use crate::pace::run::{Script, run};
