//! Model checks for the concurrent half of `DirtyCell`.
//!
//! Run with `cargo test -p zgui-bits --features loom`. Without the feature the crate uses the
//! standard library's atomics and there is no model to explore, so this file compiles to nothing.
//!
//! What is being proved is the property the ancestor walk rests on: however the marking threads
//! interleave, a bit enters the subtree union exactly once, so exactly one marker is told to
//! carry on to the parent. If that were ever reported twice the walk would do redundant work; if
//! it were ever reported zero times an ancestor would never learn it had work below it, and the
//! node's obligation would be silently dropped.

#![cfg(feature = "loom")]

use std::sync::Arc;

use loom::sync::atomic::{AtomicUsize, Ordering};
use zgui_bits::{Dirty, DirtyCell};

/// The number of threads the model explores. `loom` permits four beside the main thread, which is
/// the bound on how wide a check can be; the crate's own test module runs eight real threads over
/// the same protocol for breadth the model cannot reach.
const THREADS: usize = 3;

#[test]
fn one_marker_of_the_same_bit_is_told_to_propagate() {
    loom::model(|| {
        let cell = Arc::new(DirtyCell::clean());
        let propagated = Arc::new(AtomicUsize::new(0));

        let workers: Vec<_> = (0..THREADS)
            .map(|_| {
                let cell = Arc::clone(&cell);
                let propagated = Arc::clone(&propagated);
                loom::thread::spawn(move || {
                    if cell.mark(Dirty::RESTYLE) {
                        propagated.fetch_add(1, Ordering::Relaxed);
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().expect("a marking thread panicked");
        }

        assert_eq!(propagated.load(Ordering::Relaxed), 1);
        assert_eq!(cell.get(), (Dirty::RESTYLE, Dirty::RESTYLE));
    });
}

#[test]
fn marking_different_bits_at_once_never_loses_one() {
    loom::model(|| {
        let cell = Arc::new(DirtyCell::clean());
        let bits = [Dirty::RESTYLE, Dirty::RELAYOUT, Dirty::REPAINT];

        let workers: Vec<_> = bits
            .iter()
            .copied()
            .map(|bit| {
                let cell = Arc::clone(&cell);
                loom::thread::spawn(move || assert!(cell.mark(bit)))
            })
            .collect();
        for worker in workers {
            worker.join().expect("a marking thread panicked");
        }

        let all = Dirty::RESTYLE | Dirty::RELAYOUT | Dirty::REPAINT;
        assert_eq!(cell.get(), (all, all));
    });
}

#[test]
fn retiring_a_phase_beside_a_concurrent_mark_keeps_the_halves_apart() {
    loom::model(|| {
        let cell = Arc::new(DirtyCell::new(Dirty::RELAYOUT, Dirty::RELAYOUT));

        let marker = {
            let cell = Arc::clone(&cell);
            loom::thread::spawn(move || cell.mark(Dirty::A11Y))
        };
        let retirer = {
            let cell = Arc::clone(&cell);
            loom::thread::spawn(move || cell.retire_phase(Dirty::RELAYOUT, Dirty::empty()))
        };
        marker.join().expect("the marking thread panicked");
        retirer.join().expect("the retiring thread panicked");

        let (own, subtree) = cell.get();
        // Retirement touches the subtree union only, so the own bits carry both marks whatever
        // the order was.
        assert_eq!(own, Dirty::RELAYOUT | Dirty::A11Y);
        // The A11Y mark is never lost: either it landed before the retirement, which does not
        // clear it, or after.
        assert!(subtree.contains(Dirty::A11Y));
    });
}
