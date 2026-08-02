//! The counter block, when the counters are compiled out.
//!
//! Every function here has an empty or constant body and is marked `#[inline(always)]`, so a call
//! site leaves nothing behind: no storage is reserved, no atomic instruction is issued, and the
//! argument expressions are the only thing that survives.

use crate::counter::table::{Counter, Counters};

/// Discards `amount`.
#[inline(always)]
pub(super) fn add(_counter: Counter, _amount: u64) {}

/// Discards `amount`.
#[inline(always)]
pub(super) fn set(_counter: Counter, _amount: u64) {}

/// Reads zero, because nothing was recorded.
#[inline(always)]
pub(super) fn get(_counter: Counter) -> u64 {
    0
}

/// Reads a snapshot in which every counter is zero.
#[inline(always)]
pub(super) fn snapshot() -> Counters {
    Counters::ZERO
}

/// Does nothing: there is nothing to reset.
#[inline(always)]
pub(super) fn reset() {}
