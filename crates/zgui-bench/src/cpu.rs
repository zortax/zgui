//! The CPU time of measured frames, split between the window's thread and the rest of the process.
//!
//! A frame's wall time says how long the window's thread waited for it. With the compositor on a
//! thread of its own, the process also spends CPU the window's thread never sees, so each band of
//! frames records both clocks: the process's, which counts every thread, and the calling thread's.
//! On a driver that runs on the window's thread the two agree.

use std::cell::RefCell;
use std::collections::BTreeMap;

use rustix::time::{ClockId, Timespec, clock_gettime};

use crate::scenario::band::Spread;

/// A reading of the process's and the calling thread's CPU clocks.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Cpu {
    process: Timespec,
    thread: Timespec,
}

impl Cpu {
    /// Reads both clocks.
    pub(crate) fn now() -> Self {
        Self {
            process: clock_gettime(ClockId::ProcessCPUTime),
            thread: clock_gettime(ClockId::ThreadCPUTime),
        }
    }
}

/// The CPU time of one band's frames, in microseconds, process first.
type Band = (Vec<f64>, Vec<f64>);

thread_local! {
    /// The frames recorded so far, by band.
    static BANDS: RefCell<BTreeMap<&'static str, Band>> = const { RefCell::new(BTreeMap::new()) };
}

/// Records the CPU time spent since `from` as one frame of `band`.
pub(crate) fn record(band: &'static str, from: Cpu) {
    record_span(band, from, Cpu::now());
}

/// Records the CPU time spent from `from` to `to` as one frame of `band`.
pub(crate) fn record_span(band: &'static str, from: Cpu, to: Cpu) {
    BANDS.with_borrow_mut(|bands| {
        let (process, thread) = bands.entry(band).or_default();
        process.push(micros(from.process, to.process));
        thread.push(micros(from.thread, to.thread));
    });
}

/// Prints one `CPU` line per band recorded for `scenario`, with the spread of each clock.
pub(crate) fn print(scenario: &str) {
    BANDS.with_borrow_mut(|bands| {
        for (band, (process, thread)) in bands.iter_mut() {
            if process.is_empty() {
                continue;
            }
            let process_total: f64 = process.iter().sum();
            let thread_total: f64 = thread.iter().sum();
            let process = Spread::of(process);
            let thread = Spread::of(thread);
            println!(
                "CPU\t{scenario}\t{band}\tprocess_p50={:.2}\tprocess_p95={:.2}\tprocess_p99={:.2}\t\
                 thread_p50={:.2}\tthread_p95={:.2}\tthread_p99={:.2}\tn={}\t\
                 process_total={process_total:.0}\tthread_total={thread_total:.0}",
                process.p50,
                process.p95,
                process.p99,
                thread.p50,
                thread.p95,
                thread.p99,
                process.samples,
            );
        }
    });
}

/// Returns the microseconds from `from` to `to`.
#[expect(
    clippy::cast_precision_loss,
    reason = "a frame's CPU time is far under the 2^53 nanoseconds an f64 holds exactly"
)]
fn micros(from: Timespec, to: Timespec) -> f64 {
    let nanos = (to.tv_sec - from.tv_sec) * 1_000_000_000 + (to.tv_nsec - from.tv_nsec);
    nanos as f64 / 1e3
}
