//! The two versions of the selector-flag write, and what a sanitiser must say about each.
//!
//! Selector flags land on the element being matched *and on that element's parent*, from whichever
//! worker happens to hold the child, while other workers read the same parent. The field behind
//! that write is an `AtomicU32` for exactly this reason, and the first case here is the load that
//! shows why: six workers folding twenty thousand bits each onto one shared word, ending with every
//! bit set.
//!
//! The second case is the same load through a plain integer behind a shared reference — the write
//! the discipline forbids. It is a real data race and it is deliberate, so it runs only when
//! `ZGUI_SANITIZER_CONTROL` is set in the environment, which nothing but a sanitiser run does.
//!
//! # Why a deliberate race is worth carrying
//!
//! A sanitiser run that reports nothing has two readings: the code is clean, or the sanitiser was
//! not watching. Instrumentation that failed to reach this crate, a flag that was dropped by a
//! stale build, a suppression written too wide — each of them produces a silent, green, meaningless
//! run. The only way to tell the readings apart is to hand the sanitiser something it must object
//! to and check that it objected. That is what this case is: it fails the run when it is *not*
//! reported.
//!
//! Without a sanitiser it does nothing at all, because nothing sets the variable.

#![allow(unsafe_code)]

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;

/// The widest pool the style engine supports, which is the width the flag write is contended at.
const WORKERS: u32 = 6;

/// How many times each worker folds its bit onto the shared word.
///
/// Large enough that two workers overlap on essentially every run: the point is not to make a race
/// possible but to make missing one impossible.
const ROUNDS: u32 = 20_000;

/// The variable that arms the deliberate race.
const CONTROL: &str = "ZGUI_SANITIZER_CONTROL";

/// A `u32` shared across threads with no synchronisation whatsoever.
///
/// This is the shape the node record would have if its selector-flag word were a cell instead of an
/// atomic. It exists to be wrong.
struct Unsynchronised(UnsafeCell<u32>);

// SAFETY: none. This promise is false and is made on purpose: it is what lets the case below
// perform the unsynchronised read-modify-write that a thread sanitiser has to report, and it is why
// the case is behind an environment variable that only a sanitiser run sets. Nothing in the crate
// under test uses this type, and no ordinary test run constructs it.
unsafe impl Sync for Unsynchronised {}

/// Whether the deliberate race has been armed.
fn control_is_armed() -> bool {
    std::env::var_os(CONTROL).is_some_and(|value| !value.is_empty())
}

#[test]
fn six_workers_folding_bits_onto_one_atomic_word_lose_none_of_them() {
    let word = AtomicU32::new(0);
    thread::scope(|scope| {
        for worker in 0..WORKERS {
            let word = &word;
            scope.spawn(move || {
                for _ in 0..ROUNDS {
                    word.fetch_or(1 << worker, Ordering::Relaxed);
                }
            });
        }
    });

    let expected = (1u32 << WORKERS) - 1;
    assert_eq!(
        word.load(Ordering::Relaxed),
        expected,
        "every worker's bit has to survive the other five"
    );
}

#[test]
fn the_unsynchronised_word_is_the_race_a_sanitiser_has_to_report() {
    if !control_is_armed() {
        // Deliberate undefined behaviour is not something an ordinary `cargo test` should execute,
        // so the body runs only under the sanitiser runner that is looking for its report. The line
        // is printed so that a run which *thinks* it armed the control can see that it did not.
        println!("sanitizer control disarmed: {CONTROL} is unset, the race below did not run");
        return;
    }

    println!("sanitizer control armed: racing {WORKERS} workers on one unsynchronised word");
    let word = Unsynchronised(UnsafeCell::new(0));
    thread::scope(|scope| {
        for worker in 0..WORKERS {
            let word = &word;
            scope.spawn(move || {
                let cell = word.0.get();
                for _ in 0..ROUNDS {
                    // SAFETY: not safe, and that is the point. Six threads read and write the same
                    // `u32` with nothing ordering them. The accesses are volatile so that the
                    // optimiser cannot fold the loop away and leave the sanitiser nothing to see.
                    unsafe {
                        let seen = cell.read_volatile();
                        cell.write_volatile(seen | (1 << worker));
                    }
                }
            });
        }
    });

    // SAFETY: every worker has joined, so this read is the only access in flight.
    let seen = unsafe { word.0.get().read_volatile() };
    println!("sanitizer control finished with word {seen:#x}");
}
