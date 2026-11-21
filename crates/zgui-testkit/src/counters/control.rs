//! The positive control a low-water assertion cannot be written without.

use zgui_profile::Counter;

/// Evidence that a counter can move at all.
///
/// A control is a second run — deliberately different from the one under test — in which the same
/// counter did move. Without one, "this frame restyled nothing" is indistinguishable from "nothing
/// in this harness restyles", and the second is how a budget assertion decays into a sentence that
/// is true of an empty program.
///
/// It has no public constructor. The only way to obtain one is
/// [`Measurement::control`](crate::counters::Measurement::control), which reads the counter out of
/// a real run and panics if it stayed at zero — so a control cannot be asserted into existence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Control {
    /// Which counter it is evidence about.
    counter: Counter,
    /// How far that counter moved in the control run. Never zero.
    value: u64,
}

impl Control {
    /// Builds the control, having checked that the counter actually moved.
    ///
    /// # Panics
    ///
    /// Panics when `value` is zero, naming the counter: a run that did not move it is not a control
    /// for it, and accepting one would hand back exactly the reassurance this type exists to refuse.
    pub(crate) fn new(counter: Counter, value: u64) -> Self {
        assert!(
            value > 0,
            "the control run left `{}` at zero, so it is not evidence that anything can move it. \
             A zero-against-zero comparison is what a budget assertion must never rest on: make \
             the control run do the work the subject must not.",
            counter.name()
        );
        Self { counter, value }
    }

    /// Which counter this is evidence about.
    pub fn counter(&self) -> Counter {
        self.counter
    }

    /// How far the counter moved in the control run.
    pub fn value(&self) -> u64 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use zgui_profile::Counter;

    use super::Control;

    #[test]
    fn a_control_carries_the_counter_it_is_evidence_about() {
        let control = Control::new(Counter::ElementsRestyled, 3);
        assert_eq!(control.counter(), Counter::ElementsRestyled);
        assert_eq!(control.value(), 3);
    }

    #[test]
    #[should_panic(expected = "left `elements_restyled` at zero")]
    fn a_run_that_moved_nothing_is_not_a_control() {
        let _ = Control::new(Counter::ElementsRestyled, 0);
    }
}
