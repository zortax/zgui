//! The property the whole design rests on: a key never resolves to a value it did not name.
//!
//! Everything else the arena offers is a convenience. This is the guarantee, so it is checked
//! against a model of what the arena is supposed to be doing rather than against itself.

use std::collections::HashSet;

use proptest::prelude::*;
use zgui_arena::{ChunkArena, DomainId, Generation, Key};

/// What a test can ask the arena to do.
#[derive(Copy, Clone, Debug)]
enum Op {
    /// Store a value.
    Insert(u32),
    /// Remove the value the nth resolving key names, deferring the drop.
    Remove(usize),
    /// Take the value the nth resolving key names out immediately.
    Take(usize),
    /// End the frame.
    Recycle,
}

/// What the arena ought to answer, tracked independently of what it does answer.
#[derive(Default)]
struct Model {
    /// Keys that must resolve, with the value they must resolve to and whether it is still live.
    resolving: Vec<(Key<u32>, u32, bool)>,
    /// Keys that must not resolve at all.
    dead: Vec<Key<u32>>,
}

impl Model {
    /// Checks the arena against the model, from both directions.
    fn check(&self, arena: &ChunkArena<u32>) -> Result<(), TestCaseError> {
        for (key, value, _) in &self.resolving {
            prop_assert_eq!(
                arena.get(*key),
                Some(value),
                "a key that was issued and not yet dropped must resolve to its own value"
            );
        }
        for key in &self.dead {
            prop_assert_eq!(
                arena.get(*key),
                None,
                "a key whose value is gone must never resolve, however its slot is reused"
            );
        }
        let live = self.resolving.iter().filter(|(_, _, live)| *live).count();
        prop_assert_eq!(arena.len() as usize, live);
        Ok(())
    }

    /// Every key the arena has ever issued, which must all be distinct.
    fn issued(&self) -> impl Iterator<Item = Key<u32>> {
        self.resolving
            .iter()
            .map(|(key, _, _)| *key)
            .chain(self.dead.iter().copied())
    }
}

fn run(ops: Vec<Op>) -> Result<(), TestCaseError> {
    let mut arena: ChunkArena<u32> = ChunkArena::new(DomainId::FIRST);
    let mut model = Model::default();

    for op in ops {
        match op {
            Op::Insert(value) => {
                let key = arena.insert(value);
                let seen: HashSet<Key<u32>> = model.issued().collect();
                prop_assert!(
                    !seen.contains(&key),
                    "a key is issued at most once for the whole life of the arena"
                );
                model.resolving.push((key, value, true));
            }
            Op::Remove(nth) => {
                if let Some(entry) = pick(&mut model.resolving, nth, |(_, _, live)| *live) {
                    prop_assert!(arena.remove(entry.0));
                    prop_assert!(!arena.remove(entry.0), "removing twice changes nothing");
                    entry.2 = false;
                }
            }
            Op::Take(nth) => {
                let taken = pick(&mut model.resolving, nth, |_| true).map(|entry| *entry);
                if let Some((key, value, _)) = taken {
                    prop_assert_eq!(arena.take(key), Some(value));
                    model.resolving.retain(|(other, _, _)| *other != key);
                    model.dead.push(key);
                }
            }
            Op::Recycle => {
                arena.recycle();
                let Model { resolving, dead } = &mut model;
                resolving.retain(|(key, _, live)| {
                    if *live {
                        true
                    } else {
                        dead.push(*key);
                        false
                    }
                });
            }
        }
        model.check(&arena)?;
    }
    Ok(())
}

/// The nth entry matching a predicate, wrapping around so any index picks something.
fn pick<T>(entries: &mut [T], nth: usize, matches: impl Fn(&T) -> bool) -> Option<&mut T> {
    let positions: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| matches(entry))
        .map(|(index, _)| index)
        .collect();
    let position = *positions.get(nth % positions.len().max(1))?;
    entries.get_mut(position)
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        6 => any::<u32>().prop_map(Op::Insert),
        4 => any::<usize>().prop_map(Op::Remove),
        1 => any::<usize>().prop_map(Op::Take),
        3 => Just(Op::Recycle),
    ]
}

/// The configuration this test runs under.
///
/// It is the default, so `PROPTEST_CASES` and the rest of proptest's environment still apply —
/// except under an interpreter, where two of the defaults do not work. Recording a failing case
/// in a file beside the source needs the working directory, which an interpreter running the
/// process in isolation will not hand over; and a case count chosen for compiled code takes hours
/// when every memory access is checked.
fn config() -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if cfg!(miri) {
        config.failure_persistence = None;
        config.cases = config.cases.min(4);
    }
    config
}

proptest! {
    #![proptest_config(config())]

    /// No sequence of insertions, removals and frame boundaries makes a key lie.
    #[test]
    fn a_key_resolves_to_its_own_value_or_to_nothing(ops in proptest::collection::vec(op(), 0..200)) {
        run(ops)?;
    }
}

#[test]
fn a_slot_that_runs_out_of_counters_is_retired_and_its_keys_stay_dead() {
    let mut arena: ChunkArena<u32> = ChunkArena::new(DomainId::FIRST);
    let mut issued: Vec<Key<u32>> = Vec::new();

    // One slot, cycled until its counter runs out. Every cycle reuses slot 0 until the counter
    // reaches its last value, at which point the slot is retired and the next insert goes
    // somewhere else entirely.
    while arena.retired() == 0 {
        let key = arena.insert(issued.len() as u32);
        assert_eq!(key.index(), 0, "the same slot comes back round every frame");
        assert_eq!(
            usize::from(key.generation().get()),
            issued.len() + 1,
            "each occupant of a slot gets the next counter, and no counter repeats"
        );
        issued.push(key);
        arena.remove(key);
        arena.recycle();
    }

    assert_eq!(issued.len(), usize::from(u16::MAX));
    assert_eq!(issued[0].index(), 0);
    assert_eq!(issued[0].generation(), Generation::FIRST);
    assert_eq!(issued[issued.len() - 1].generation(), Generation::LAST);

    let next = arena.insert(0);
    assert_eq!(
        next.index(),
        1,
        "the retired slot is never handed out again"
    );
    assert_eq!(next.generation(), Generation::FIRST);

    for key in &issued {
        assert_eq!(arena.get(*key), None, "no key from a retired slot resolves");
    }
}
