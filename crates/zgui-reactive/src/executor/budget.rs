//! The per-flush iteration budget.
//!
//! Polling "until nothing is ready" does not terminate when two effects write each other's
//! sources: each write makes the other ready again, inside a single poll, with no frame
//! presented and nothing logged. The budget caps how many times one task may be polled within
//! one flush. A task that reaches the cap is set aside — its waker is kept, so it resumes at the
//! next flush — and the cycle is reported once.
//!
//! An ordinary task is polled once or twice per flush, so the cap is never approached by
//! correct code.

use std::panic::Location;

/// How many times one task may be polled during a single flush.
///
/// Debug builds allow more, because a legitimately chatty dependency chain should be diagnosed
/// rather than truncated while the cause is being investigated; release builds cut the cycle
/// off sooner, because the frame still has to present.
pub(crate) const BUDGET: u32 = if cfg!(debug_assertions) { 32 } else { 8 };

/// One task's poll count within one flush.
#[derive(Debug, Default)]
pub(crate) struct TaskBudget {
    /// The flush this count belongs to; a newer flush resets it.
    generation: u64,
    /// Polls so far in that flush.
    polls: u32,
    /// Whether this task has already been set aside in that flush.
    exhausted: bool,
}

impl TaskBudget {
    /// Records a poll in flush `generation`, returning whether the task may run.
    ///
    /// The first call that returns `false` in a given flush is the one that should report the
    /// cycle; later calls in the same flush return `false` silently.
    pub(crate) fn admit(&mut self, generation: u64) -> Admission {
        if self.generation != generation {
            self.generation = generation;
            self.polls = 0;
            self.exhausted = false;
        }
        self.polls += 1;
        if self.polls <= BUDGET {
            Admission::Run
        } else if std::mem::replace(&mut self.exhausted, true) {
            Admission::AlreadyDeferred
        } else {
            Admission::Defer
        }
    }
}

/// What a task should do with the poll it just asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Admission {
    /// Under budget: poll the inner future.
    Run,
    /// Over budget for the first time this flush: report the cycle and set the task aside.
    Defer,
    /// Over budget, already reported and already set aside this flush.
    AlreadyDeferred,
}

/// Reports a task that exhausted its budget, naming where it was spawned if that is known.
pub(crate) fn report(spawned_at: Option<&'static Location<'static>>) {
    match spawned_at {
        Some(location) => tracing::error!(
            budget = BUDGET,
            file = location.file(),
            line = location.line(),
            "a reactive task re-ran more times in one frame than the iteration budget allows; \
             it is almost certainly writing a source it depends on. It has been set aside until \
             the next frame so this one can present."
        ),
        None => tracing::error!(
            budget = BUDGET,
            "an effect re-ran more times in one frame than the iteration budget allows; two \
             effects are almost certainly writing each other's sources. It has been set aside \
             until the next frame so this one can present."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_budget_is_per_flush() {
        let mut budget = TaskBudget::default();
        for _ in 0..BUDGET {
            assert_eq!(budget.admit(1), Admission::Run);
        }
        assert_eq!(budget.admit(1), Admission::Defer);
        assert_eq!(budget.admit(1), Admission::AlreadyDeferred);
        assert_eq!(budget.admit(2), Admission::Run);
    }
}
