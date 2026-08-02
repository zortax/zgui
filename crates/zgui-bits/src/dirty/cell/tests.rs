//! Model tests for the packing, plus a real-threads stress run over the marking protocol.
//!
//! The concurrency *proof* is the model check under `tests/dirty_cell_loom.rs`; what runs here is
//! the single-threaded algebra and a stress run wide enough to catch a packing mistake that only
//! shows up when several threads mark at once.

#![cfg(not(feature = "loom"))]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering as StdOrdering};
use std::thread;

use proptest::prelude::*;

use super::DirtyCell;
use crate::dirty::bits::Dirty;

/// A direct restatement of what each operation means, written separately from the packing
/// that implements it, so the property test compares two independent statements of the rule.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct Model {
    /// The node's own obligations.
    own: Dirty,
    /// The union of everything at or below it.
    subtree: Dirty,
}

/// One call on a cell.
#[derive(Copy, Clone, Debug)]
enum Op {
    /// [`DirtyCell::mark`].
    Mark(Dirty),
    /// [`DirtyCell::mark_subtree`].
    MarkSubtree(Dirty),
    /// [`DirtyCell::clear_own`].
    ClearOwn(Dirty),
    /// [`DirtyCell::retire_phase`].
    RetirePhase(Dirty, Dirty),
}

impl Model {
    /// Applies `op`, returning the boolean the marking calls answer with.
    fn apply(&mut self, op: Op) -> Option<bool> {
        match op {
            Op::Mark(bits) => {
                let gained = !self.subtree.contains(bits);
                self.own |= bits;
                self.subtree |= bits;
                Some(gained)
            }
            Op::MarkSubtree(bits) => {
                let gained = !self.subtree.contains(bits);
                self.subtree |= bits;
                Some(gained)
            }
            Op::ClearOwn(bits) => {
                self.own -= bits;
                None
            }
            Op::RetirePhase(phase, keep) => {
                self.subtree -= phase;
                self.subtree |= keep;
                None
            }
        }
    }
}

/// Any set of lattice bits, including the empty one.
fn any_dirty() -> impl Strategy<Value = Dirty> {
    (0u32..=Dirty::all().bits()).prop_map(Dirty::from_bits_truncate)
}

/// Any single call.
fn any_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        any_dirty().prop_map(Op::Mark),
        any_dirty().prop_map(Op::MarkSubtree),
        any_dirty().prop_map(Op::ClearOwn),
        (any_dirty(), any_dirty()).prop_map(|(phase, keep)| Op::RetirePhase(phase, keep)),
    ]
}

proptest! {
    /// The two halves stay in step with the model over arbitrary operation sequences, which is
    /// the same as saying no operation on one half ever leaks into the other.
    #[test]
    fn the_halves_track_the_model_and_never_alias(ops in prop::collection::vec(any_op(), 0..64)) {
        let cell = DirtyCell::clean();
        let mut model = Model::default();
        for op in ops {
            let expected = model.apply(op);
            let actual = match op {
                Op::Mark(bits) => Some(cell.mark(bits)),
                Op::MarkSubtree(bits) => Some(cell.mark_subtree(bits)),
                Op::ClearOwn(bits) => { cell.clear_own(bits); None }
                Op::RetirePhase(phase, keep) => { cell.retire_phase(phase, keep); None }
            };
            prop_assert_eq!(actual, expected);
            prop_assert_eq!(cell.own(), model.own);
            prop_assert_eq!(cell.subtree(), model.subtree);
            prop_assert_eq!(cell.get(), (model.own, model.subtree));
        }
    }

    /// Marking is the only operation that reports, and it reports exactly once per bit: the
    /// early-out the ancestor walk rests on.
    #[test]
    fn a_bit_is_reported_gained_exactly_once(bits in any_dirty(), repeats in 1usize..8) {
        prop_assume!(!bits.is_clean());
        let cell = DirtyCell::clean();
        prop_assert!(cell.mark(bits));
        for _ in 0..repeats {
            prop_assert!(!cell.mark(bits));
            prop_assert!(!cell.mark_subtree(bits));
        }
    }

    /// Round-tripping any pair through the constructor recovers both halves unchanged.
    #[test]
    fn construction_round_trips(own in any_dirty(), subtree in any_dirty()) {
        let cell = DirtyCell::new(own, subtree);
        prop_assert_eq!(cell.get(), (own, subtree));
    }
}

#[test]
fn a_clean_cell_owes_nothing() {
    let cell = DirtyCell::clean();
    assert!(cell.is_clean());
    assert_eq!(cell.get(), (Dirty::empty(), Dirty::empty()));
}

#[test]
fn retiring_a_phase_leaves_the_other_pending_unions_alone() {
    let cell = DirtyCell::clean();
    cell.mark(Dirty::RELAYOUT | Dirty::A11Y);
    cell.retire_phase(Dirty::RELAYOUT, Dirty::empty());
    assert_eq!(cell.subtree(), Dirty::A11Y);
    assert_eq!(cell.own(), Dirty::RELAYOUT | Dirty::A11Y);
}

#[test]
fn retiring_a_phase_restores_what_the_walk_found_outstanding() {
    let cell = DirtyCell::clean();
    cell.mark_subtree(Dirty::RESTYLE | Dirty::REPAINT);
    cell.retire_phase(Dirty::RESTYLE | Dirty::REPAINT, Dirty::RESTYLE);
    assert_eq!(cell.subtree(), Dirty::RESTYLE);
}

#[test]
fn clearing_own_bits_never_disturbs_the_subtree_union() {
    let cell = DirtyCell::new(Dirty::all(), Dirty::all());
    cell.clear_own(Dirty::all());
    assert_eq!(cell.own(), Dirty::empty());
    assert_eq!(cell.subtree(), Dirty::all());
}

#[test]
fn eight_threads_marking_at_once_report_each_bit_gained_exactly_once() {
    const THREADS: u32 = 8;
    const ROUNDS: u32 = 512;

    let cell = Arc::new(DirtyCell::clean());
    let gained: Arc<Vec<AtomicU32>> = Arc::new((0..THREADS).map(|_| AtomicU32::new(0)).collect());

    let workers: Vec<_> = (0..THREADS)
        .map(|index| {
            let cell = Arc::clone(&cell);
            let gained = Arc::clone(&gained);
            thread::spawn(move || {
                let bit = Dirty::from_bits_truncate(1 << index);
                for _ in 0..ROUNDS {
                    if cell.mark(bit) {
                        gained[index as usize].fetch_add(1, StdOrdering::Relaxed);
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().expect("a marking thread panicked");
    }

    let expected = Dirty::from_bits_truncate((1 << THREADS) - 1);
    assert_eq!(cell.get(), (expected, expected));
    for counter in gained.iter() {
        assert_eq!(counter.load(StdOrdering::Relaxed), 1);
    }
}
