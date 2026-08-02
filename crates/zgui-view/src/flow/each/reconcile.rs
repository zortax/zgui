//! Turning one ordered list of keys into another with as few moves as it can.

use core::hash::Hash;
use std::collections::HashMap;

/// What has to happen to one position of the new list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Step {
    /// The item at this index of the old list is reused, and is already in the right place.
    Keep(usize),
    /// The item at this index of the old list is reused, and has to be moved.
    Move(usize),
    /// There was no such item: build a new one.
    Create,
}

/// The whole plan for one reconciliation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct Plan {
    /// One step per position of the new list, in order.
    pub(super) steps: Vec<Step>,
    /// The indices of the old list that are not reused, in ascending order.
    pub(super) removed: Vec<usize>,
}

/// Works out how to turn `old` into `new`.
///
/// The plan is built from the right, which is what makes it cheap and what makes it correct: the
/// suffix that has already been placed never moves again, so an item whose remaining neighbours
/// are already in the right order is left alone.
///
/// A duplicated key would make "which item is this" unanswerable, so the first occurrence wins and
/// every later one is treated as new.
pub(super) fn plan<K: Eq + Hash + Clone>(old: &[K], new: &[K]) -> Plan {
    let mut positions: HashMap<&K, usize> = HashMap::with_capacity(old.len());
    for (index, key) in old.iter().enumerate() {
        positions.entry(key).or_insert(index);
    }

    // Which old index each new position reuses, if any.
    let mut reuse: Vec<Option<usize>> = Vec::with_capacity(new.len());
    let mut taken = vec![false; old.len()];
    for key in new {
        match positions.get(key) {
            Some(&index) if !taken[index] => {
                taken[index] = true;
                reuse.push(Some(index));
            }
            _ => reuse.push(None),
        }
    }

    // The old indices that survive, in their old order: the sequence a right-to-left walk
    // compares against.
    let mut surviving: Vec<usize> = (0..old.len()).filter(|index| taken[*index]).collect();

    let mut steps = vec![Step::Create; new.len()];
    for position in (0..new.len()).rev() {
        match reuse[position] {
            None => steps[position] = Step::Create,
            Some(index) => {
                if surviving.last() == Some(&index) {
                    surviving.pop();
                    steps[position] = Step::Keep(index);
                } else {
                    if let Some(at) = surviving.iter().position(|candidate| *candidate == index) {
                        surviving.remove(at);
                    }
                    steps[position] = Step::Move(index);
                }
            }
        }
    }

    let removed = (0..old.len()).filter(|index| !taken[*index]).collect();
    Plan { steps, removed }
}

#[cfg(test)]
mod tests {
    use super::{Step, plan};

    #[test]
    fn an_unchanged_list_moves_nothing() {
        let made = plan(&[1, 2, 3], &[1, 2, 3]);
        assert_eq!(
            made.steps,
            vec![Step::Keep(0), Step::Keep(1), Step::Keep(2)]
        );
        assert!(made.removed.is_empty());
    }

    #[test]
    fn an_appended_item_moves_nothing_that_was_there() {
        let made = plan(&[1, 2], &[1, 2, 3]);
        assert_eq!(made.steps, vec![Step::Keep(0), Step::Keep(1), Step::Create]);
    }

    #[test]
    fn an_item_inserted_at_the_front_moves_nothing_that_was_there() {
        let made = plan(&[1, 2], &[0, 1, 2]);
        assert_eq!(made.steps, vec![Step::Create, Step::Keep(0), Step::Keep(1)]);
    }

    #[test]
    fn a_removed_item_is_reported_once() {
        let made = plan(&[1, 2, 3], &[1, 3]);
        assert_eq!(made.removed, vec![1]);
        assert_eq!(made.steps, vec![Step::Keep(0), Step::Keep(2)]);
    }

    #[test]
    fn a_reversal_leaves_one_item_alone_and_moves_the_rest_past_it() {
        // Placing from the right: the item that ends up first never has to move, because
        // everything that was after it has been carried out to the right in order.
        let made = plan(&[1, 2, 3], &[3, 2, 1]);
        assert_eq!(
            made.steps,
            vec![Step::Keep(2), Step::Move(1), Step::Move(0)]
        );
    }

    #[test]
    fn a_repeated_key_is_reused_once_and_built_again_after_that() {
        let made = plan(&[1], &[1, 1]);
        assert_eq!(made.steps, vec![Step::Keep(0), Step::Create]);
    }
}
