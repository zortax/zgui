//! The counters whose producing stage has not been built yet.
//!
//! A counter is the vocabulary a budget is written in, so the set is allowed to describe stages
//! that do not exist: deleting `Counter::DrawCalls` until there is a renderer would mean rewriting
//! the plan's sentences rather than keeping them. What is not allowed is for such a counter to be
//! indistinguishable from one whose producer was lost — because a budget naming either of them
//! passes while measuring nothing.
//!
//! So each one is listed here with the stage that will move it. An entry is a promise with a name
//! on it, and the list only ever shrinks: once a counter has a producer, its entry is a violation.

/// A counter that nothing increments yet, and what will.
pub(crate) struct Awaiting {
    /// The counter's variant name.
    pub(crate) counter: &'static str,
    /// The stage that will increment it.
    pub(crate) stage: &'static str,
}

/// Every counter that is deliberately unproduced, with the stage that will produce it.
///
/// Empty, which is the state this list is meant to reach: every counter the tree declares now has
/// a producer, and the check that reads this is what says so.
pub(crate) const AWAITING: &[Awaiting] = &[];

/// The stage promised for `counter`, if it is on the list.
pub(crate) fn stage_of(counter: &str) -> Option<&'static str> {
    AWAITING
        .iter()
        .find(|awaiting| awaiting.counter == counter)
        .map(|awaiting| awaiting.stage)
}

#[cfg(test)]
mod tests {
    use super::{AWAITING, stage_of};

    #[test]
    fn every_entry_names_a_stage_and_appears_once() {
        for awaiting in AWAITING {
            assert!(
                !awaiting.stage.is_empty(),
                "{} promises nothing",
                awaiting.counter
            );
            let entries = AWAITING
                .iter()
                .filter(|other| other.counter == awaiting.counter)
                .count();
            assert_eq!(entries, 1, "{} is listed twice", awaiting.counter);
        }
    }

    #[test]
    fn a_counter_that_is_not_listed_has_no_stage() {
        // Read off the list rather than written out, because the list only ever shrinks: a
        // hard-coded name here would fail the day the stage that promised it lands, and the list
        // reaching empty is the state it is meant to reach rather than a reason to fail.
        if let Some(listed) = AWAITING.first() {
            assert_eq!(stage_of(listed.counter), Some(listed.stage));
        }
        assert_eq!(stage_of("NodesVisited"), None);
    }
}
