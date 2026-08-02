//! The fixture's counter set, with one skip in it.

counters! {
    /// Work the stage performed.
    Alpha => alpha, Group::BackendNeutral;

    /// Work the stage avoided, read against the work it did instead.
    Beta => beta, Group::Skip { done: Counter::Alpha };
}
