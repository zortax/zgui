//! What one measured run did, and the assertions that may be made about it.

use zgui_profile::{Counter, Counters};

use crate::counters::control::Control;
use crate::counters::meaning;

/// The counters one run moved.
///
/// Taken between frames, it is exact: the counters are read one at a time, but nothing is writing
/// them while a measurement is being taken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    /// Every counter's value at the end of the run, having started from zero.
    counters: Counters,
}

impl Measurement {
    /// The measurement of a run that ended with these counters.
    pub(crate) fn new(counters: Counters) -> Self {
        Self { counters }
    }

    /// Every counter, as a snapshot, with the renderer-specific ones poisoned.
    ///
    /// This is the read without a control, and it is here because a test that asserts an *exact
    /// non-zero* count is already its own control. Reach for [`Measurement::assert_zero`] and
    /// [`Measurement::assert_at_most`] for everything else.
    ///
    /// A counter only a graphics backend increments reads
    /// [`POISON`](crate::counters::meaning::POISON) here and never zero: refusing it by name would
    /// leave this route wide open, and a zero on this route is the whole of what the refusal is
    /// for.
    pub fn counters(&self) -> Counters {
        meaning::poisoned(self.counters)
    }

    /// One counter's value.
    ///
    /// # Panics
    ///
    /// Panics for a renderer-specific counter, which reads zero here because nothing drew.
    pub fn get(&self, counter: Counter) -> u64 {
        assert!(
            meaning::is_meaningful(counter),
            "{}",
            meaning::refusal(counter)
        );
        self.counters.get(counter)
    }

    /// This run as a control for `counter`.
    ///
    /// # Panics
    ///
    /// Panics when the counter did not move in this run, because a run that did not move it proves
    /// nothing about a run in which it must stay still.
    pub fn control(&self, counter: Counter) -> Control {
        Control::new(counter, self.get(counter))
    }

    /// Asserts that `counter` did not move, against a control that proves it can.
    ///
    /// # Panics
    ///
    /// Panics when the counter moved, or when `control` is evidence about a different counter.
    pub fn assert_zero(&self, counter: Counter, control: &Control) {
        self.check_control(counter, control);
        let value = self.get(counter);
        assert_eq!(
            value,
            0,
            "expected `{}` to stay at zero, but it reached {value} (the control run reached {})",
            counter.name(),
            control.value()
        );
    }

    /// Asserts that `counter` stayed at or below `bound`, against a control that proves it can move.
    ///
    /// An upper bound needs a control for the same reason a zero does: `nodes_visited < 64` is true
    /// of a traversal that visited sixty-three nodes and of one that never ran.
    ///
    /// # Panics
    ///
    /// Panics when the counter exceeded the bound, when `control` is evidence about a different
    /// counter, or when the control run itself stayed inside the bound — in which case the bound
    /// separates nothing, since the run it was supposed to exclude would have passed it too.
    pub fn assert_at_most(&self, counter: Counter, bound: u64, control: &Control) {
        self.check_control(counter, control);
        assert!(
            control.value() > bound,
            "the control run left `{}` at {}, which is already within the bound of {bound}: this \
             assertion would hold for the run it was written to exclude, so it separates nothing",
            counter.name(),
            control.value()
        );
        let value = self.get(counter);
        assert!(
            value <= bound,
            "expected `{}` to stay at or below {bound}, but it reached {value}",
            counter.name()
        );
    }

    /// Asserts that `counter` reached exactly `expected`.
    ///
    /// No control is required, and none is possible: a non-zero expectation is its own evidence
    /// that the mechanism ran.
    ///
    /// # Panics
    ///
    /// Panics when the count differs, and when `expected` is zero — a zero expectation is the case
    /// a control exists for, and [`Measurement::assert_zero`] is where it lives.
    pub fn assert_exactly(&self, counter: Counter, expected: u64) {
        assert!(
            expected > 0,
            "`assert_exactly({}, 0)` is the assertion that passes on a harness where nothing moves \
             that counter at all. Use `assert_zero` with a control run that does move it.",
            counter.name()
        );
        let value = self.get(counter);
        assert_eq!(
            value,
            expected,
            "expected `{}` to reach exactly {expected}, but it reached {value}",
            counter.name()
        );
    }

    /// Rejects a control that is evidence about some other counter.
    fn check_control(&self, counter: Counter, control: &Control) {
        assert_eq!(
            control.counter(),
            counter,
            "the control is evidence about `{}`, which says nothing about `{}`",
            control.counter().name(),
            counter.name()
        );
    }
}

#[cfg(test)]
mod tests {
    use zgui_profile::{Counter, Counters};

    use super::Measurement;

    /// A measurement in which `counter` reached `value`.
    fn measured(counter: Counter, value: u64) -> Measurement {
        Measurement::new(Counters::from_fn(
            |held| {
                if held == counter { value } else { 0 }
            },
        ))
    }

    #[test]
    fn a_zero_assertion_holds_when_it_is_backed_by_a_control() {
        let control = measured(Counter::ElementsRestyled, 7).control(Counter::ElementsRestyled);
        measured(Counter::ElementsRestyled, 0).assert_zero(Counter::ElementsRestyled, &control);
    }

    #[test]
    #[should_panic(expected = "reached 2")]
    fn a_zero_assertion_fails_when_the_counter_moved() {
        let control = measured(Counter::ElementsRestyled, 7).control(Counter::ElementsRestyled);
        measured(Counter::ElementsRestyled, 2).assert_zero(Counter::ElementsRestyled, &control);
    }

    #[test]
    #[should_panic(expected = "says nothing about")]
    fn a_control_for_another_counter_is_refused() {
        let control = measured(Counter::SelectorMatches, 7).control(Counter::SelectorMatches);
        measured(Counter::ElementsRestyled, 0).assert_zero(Counter::ElementsRestyled, &control);
    }

    #[test]
    #[should_panic(expected = "separates nothing")]
    fn a_bound_the_control_run_also_satisfies_is_refused() {
        // The bound is supposed to exclude the control run. One that does not is a bound written
        // against no behaviour at all, which is how "assert!(x < 64)" survives on a harness where x
        // never exceeds three.
        let control = measured(Counter::NodesVisited, 3).control(Counter::NodesVisited);
        measured(Counter::NodesVisited, 1).assert_at_most(Counter::NodesVisited, 64, &control);
    }

    #[test]
    fn a_bound_that_excludes_the_control_run_holds() {
        let control = measured(Counter::NodesVisited, 9997).control(Counter::NodesVisited);
        measured(Counter::NodesVisited, 4).assert_at_most(Counter::NodesVisited, 64, &control);
    }

    #[test]
    #[should_panic(expected = "reads zero under a renderer that submits no work")]
    fn a_renderer_specific_counter_cannot_be_read_here() {
        let _ = measured(Counter::DrawCalls, 0).get(Counter::DrawCalls);
    }

    #[test]
    #[should_panic(expected = "Use `assert_zero` with a control")]
    fn an_exact_zero_expectation_is_sent_back_to_the_control_path() {
        measured(Counter::Repaints, 0).assert_exactly(Counter::Repaints, 0);
    }

    #[test]
    fn an_exact_non_zero_expectation_needs_no_control() {
        measured(Counter::Repaints, 1).assert_exactly(Counter::Repaints, 1);
    }
}
