//! What every reference workload needs and none of them should own a copy of.
//!
//! A reference workload is a document, a gesture and a sweep: the document is driven at four sizes
//! in one process, each size is timed the same way, and what comes out is a *slope* — cost per box,
//! per row, per control. The slope in microseconds is keyed to the machine that took it and gates
//! nothing. What gates is the ratio of that slope to a second slope measured in the same process
//! over the same documents, which is dimensionless and therefore the same number on a laptop and on
//! a loaded CI host.
//!
//! The parts:
//!
//! - [`sample`] — warm-up, repeats, and the median of them, so one point of a slope is not one
//!   scheduling accident.
//! - [`fit`] — the least-squares slope through those points.
//! - [`verdict`] — what a dimensionless number is allowed to be, and what a run says when it is
//!   not. Including the case a gate acquires silently: the measurement stopped working, there is no
//!   number at all, and a comparison written as "not worse than" holds against nothing.
//! - [`watch`] — a renderer that records what each frame damaged, for the workloads whose claim is
//!   about damage rather than about time.

pub mod fit;
pub mod sample;
pub mod verdict;
pub mod watch;
