//! Unit and property tests for [`EpochBitset`](super::EpochBitset).

use std::collections::HashSet;

use proptest::prelude::*;

use super::{EpochBitset, NEVER};

/// One call on a bitset.
#[derive(Copy, Clone, Debug)]
enum Op {
    /// [`EpochBitset::visit`].
    Visit(usize),
    /// [`EpochBitset::contains`].
    Contains(usize),
    /// [`EpochBitset::forget`].
    Forget(usize),
    /// [`EpochBitset::bump`].
    Bump,
}

/// Any call, over a small index space so collisions actually happen.
fn any_op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => (0usize..24).prop_map(Op::Visit),
        3 => (0usize..24).prop_map(Op::Contains),
        1 => (0usize..24).prop_map(Op::Forget),
        3 => Just(Op::Bump),
    ]
}

proptest! {
    /// Membership matches a set that is genuinely emptied on every bump, so no stamp left over
    /// from an earlier epoch is ever mistaken for a current one.
    #[test]
    fn no_visit_survives_an_epoch_bump(ops in prop::collection::vec(any_op(), 0..256)) {
        let mut bitset = EpochBitset::new();
        let mut model: HashSet<usize> = HashSet::new();
        for op in ops {
            match op {
                Op::Visit(index) => {
                    prop_assert_eq!(bitset.visit(index), model.insert(index));
                    prop_assert!(bitset.contains(index));
                }
                Op::Contains(index) => {
                    prop_assert_eq!(bitset.contains(index), model.contains(&index));
                }
                Op::Forget(index) => {
                    bitset.forget(index);
                    model.remove(&index);
                    prop_assert!(!bitset.contains(index));
                }
                Op::Bump => {
                    bitset.bump();
                    model.clear();
                    for index in 0..24 {
                        prop_assert!(!bitset.contains(index), "index {} survived a bump", index);
                    }
                }
            }
        }
    }

    /// Visiting is idempotent within an epoch: only the first call reports.
    #[test]
    fn only_the_first_visit_of_an_epoch_reports(index in 0usize..1024, repeats in 1usize..8) {
        let mut bitset = EpochBitset::new();
        prop_assert!(bitset.visit(index));
        for _ in 0..repeats {
            prop_assert!(!bitset.visit(index));
        }
    }
}

#[test]
fn a_fresh_set_has_visited_nothing() {
    let bitset = EpochBitset::new();
    assert!(!bitset.contains(0));
    assert!(!bitset.contains(usize::MAX));
    assert_eq!(bitset.capacity(), 0);
}

#[test]
fn the_set_grows_to_fit_the_index_it_is_given() {
    let mut bitset = EpochBitset::with_capacity(4);
    assert!(bitset.visit(63));
    assert!(bitset.contains(63));
    assert!(bitset.capacity() >= 64);
    assert!(!bitset.contains(62));
}

#[test]
fn a_bump_costs_nothing_but_still_empties_the_set() {
    let mut bitset = EpochBitset::new();
    for index in 0..64 {
        assert!(bitset.visit(index));
    }
    let before = bitset.epoch();
    bitset.bump();
    assert_eq!(bitset.epoch(), before + 1);
    for index in 0..64 {
        assert!(!bitset.contains(index));
        assert!(bitset.visit(index));
    }
}

#[test]
fn the_epoch_that_would_wrap_clears_the_stamps_instead() {
    let mut bitset = EpochBitset::new();
    bitset.epoch = u32::MAX;
    assert!(bitset.visit(3));
    assert!(bitset.visit(9));

    bitset.bump();

    assert_eq!(bitset.epoch(), NEVER + 1);
    assert!(
        !bitset.contains(3),
        "a stamp from the last epoch was read as current"
    );
    assert!(!bitset.contains(9));
    assert!(bitset.stamps.iter().all(|stamp| *stamp == NEVER));
}

#[test]
fn resetting_returns_to_the_first_epoch_with_nothing_visited() {
    let mut bitset = EpochBitset::new();
    bitset.visit(1);
    bitset.bump();
    bitset.visit(2);
    bitset.reset();
    assert_eq!(bitset.epoch(), NEVER + 1);
    assert!(!bitset.contains(1));
    assert!(!bitset.contains(2));
}
