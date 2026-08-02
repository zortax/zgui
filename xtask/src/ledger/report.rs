//! What a ledger check reports.

use std::fmt::{self, Display};

/// One thing that is wrong with the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Violation {
    /// Where it is, relative to the tree root.
    pub(crate) at: String,
    /// What is wrong, phrased so that the fix is obvious.
    pub(crate) message: String,
}

impl Violation {
    /// Records a violation at `at`.
    pub(crate) fn new(at: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            at: at.into(),
            message: message.into(),
        }
    }
}

impl Display for Violation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.at, self.message)
    }
}

/// What one check found.
#[derive(Debug, Default)]
pub(crate) struct Report {
    /// Everything wrong, in discovery order.
    pub(crate) violations: Vec<Violation>,
    /// Conditions the check could not evaluate, reported but not fatal.
    pub(crate) skipped: Vec<String>,
}

impl Report {
    /// A report with nothing in it.
    pub(crate) fn clean() -> Self {
        Self::default()
    }

    /// Records a violation.
    pub(crate) fn violation(&mut self, at: impl Into<String>, message: impl Into<String>) {
        self.violations.push(Violation::new(at, message));
    }

    /// Records that part of the check could not run.
    pub(crate) fn skip(&mut self, reason: impl Into<String>) {
        self.skipped.push(reason.into());
    }

    /// Whether the check passed.
    pub(crate) fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}
