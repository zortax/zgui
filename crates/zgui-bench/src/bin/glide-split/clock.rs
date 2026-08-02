//! What reading the clock costs, measured on the machine that is about to read it a great many
//! times.
//!
//! Every timed descent is bracketed by two reads of [`std::time::Instant`], and a moved subtree is
//! one descent or three. The cost of the reads is therefore inside every number this probe prints,
//! and it cancels out of the ones that matter — a duty is a difference between two descents and
//! both were bracketed the same way — but it does not cancel out of the comparison between one
//! fused descent and three divided ones. Which is exactly the comparison that says whether the
//! subtraction is sound, so what the reads cost has to be on the page beside it.

use std::hint::black_box;
use std::time::Instant;

/// How many reads the calibration makes.
const READS: u32 = 100_000;

/// Nanoseconds one read of the clock costs on this machine.
pub(crate) fn read_ns() -> f64 {
    // Warm: the first read of a process faults in the page the kernel exports its clock through.
    for _ in 0..1_000 {
        black_box(Instant::now());
    }
    let started = Instant::now();
    for _ in 0..READS {
        black_box(Instant::now());
    }
    started.elapsed().as_secs_f64() * 1e9 / f64::from(READS)
}
