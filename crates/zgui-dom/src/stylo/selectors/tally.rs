//! Counting the questions the matcher asks.
//!
//! A restyle's cost is not the number of elements it touched: one element carrying a compound
//! selector against a large rule set is many more questions than one element against a small one,
//! and a bloom filter that stops working shows up here long before it shows up in the element
//! count. So every predicate on the matching surface passes its answer through [`tested`] on the
//! way out.
//!
//! What is counted is a *simple selector against one element* — a name, a class, an identifier, an
//! attribute, a state, a structural position. Stepping to a parent or a sibling is not counted:
//! walking to the next candidate is not a test, and counting it would make the number depend on the
//! shape of the tree rather than on the shape of the rule set.

use zgui_profile::{Counter, counter};

/// Records that the matcher asked one question, and hands back the answer it got.
///
/// Costs nothing in a build with the counters compiled out, which is every optimised build that
/// does not ask for them.
#[inline]
pub(crate) fn tested<T>(answer: T) -> T {
    counter::bump(Counter::SelectorMatches);
    answer
}

#[cfg(test)]
mod tests {
    use zgui_profile::{COUNTERS_ENABLED, Counter, counter};

    use super::tested;

    #[test]
    fn the_answer_passes_through_unchanged_and_the_counter_moves() {
        // Both halves matter: a wrapper that dropped the answer would break matching, and one that
        // forgot the counter would leave every budget written against it measuring nothing.
        let before = counter::get(Counter::SelectorMatches);
        assert!(tested(true));
        assert!(!tested(false));
        assert_eq!(tested(7u32), 7);
        if COUNTERS_ENABLED {
            assert_eq!(counter::get(Counter::SelectorMatches) - before, 3);
        }
    }
}
