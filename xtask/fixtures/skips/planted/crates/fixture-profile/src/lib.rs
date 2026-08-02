//! The fixture's counter set, with a skip nothing proves can move.

counters! {
    /// Work the stage performed.
    Alpha => alpha, Group::BackendNeutral;

    /// Work the stage avoided, read against the work it did instead.
    ///
    /// The planted violation: the test target beside this one drives the stage and asserts an
    /// upper bound on this counter, which is exactly the assertion that holds over a stage that
    /// skips nothing at all — and there is no assertion anywhere that it can ever move.
    Beta => beta, Group::Skip { done: Counter::Alpha };
}
