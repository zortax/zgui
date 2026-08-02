//! The growth check: what a run is still holding after a thousand ticks that it was not holding
//! after ten.
//!
//! Every other instrument in this harness measures how long something took. None of them can see a
//! table that gains a hundred and seventy entries per wheel notch, because the frame that adds them
//! is not slower — it is the frame six thousand notches later that is slower, and by then nothing
//! attributes the cost to what caused it. A resident-set figure is no better: it is smeared by the
//! allocator and lags by whole seconds, and the one measurement that discriminates is the length of
//! the table itself.
//!
//! So the check is a count band of zero over the lengths, taken twice in one run. The counts are
//! [`Group::Live`](zgui_profile::Group::Live), the samples are ten ticks in and a thousand ticks in,
//! and the rule is that they are equal.

mod compare;
mod run;

#[cfg(test)]
mod tests;

pub(crate) use crate::growth::run::{report, run};
