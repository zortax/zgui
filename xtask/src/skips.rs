//! The standing gate over counters of avoided work.
//!
//! A counter of work *performed* is falsifiable on its own. A counter of work *avoided* is not: it
//! reads zero when the stage is skipping perfectly, zero when the stage has stopped skipping, and
//! zero when nobody ever incremented it. So a bound written against one alone is green from the day
//! it is written whatever happens underneath.
//!
//! The counter table says which counters are of the second kind, by declaring them
//! `Group::Skip { done: Counter::… }`, and this refuses any that does not carry both a pair and a
//! non-vacuity assertion. It is the [`skips`](crate::ledger::check::skips) ledger check run on its
//! own, so the two can never say different things.

use std::path::Path;

use crate::error::Result;
use crate::ledger;

/// The check this runs.
const CHECK: &str = "skips";

/// Runs the gate.
pub(crate) fn run(root: &Path) -> Result<()> {
    ledger::run(root, Some(CHECK))
}
