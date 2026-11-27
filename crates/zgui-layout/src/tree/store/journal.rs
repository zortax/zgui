//! Bounded histories of marks that readers catch up on from their own revision.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use zgui_space::ChangeCoverage;

/// The identity and revision of one journal at one moment.
///
/// A version from another journal asks for a full refresh.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JournalVersion {
    instance: u64,
    revision: u64,
}

/// Distinguishes journals whose revisions happen to hold the same number.
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// A bounded history of marks, each a copy of what was marked.
#[derive(Debug)]
pub(crate) struct Journal<T> {
    instance: u64,
    revision: u64,
    marks: VecDeque<(u64, T)>,
    forgotten: u64,
}

impl<T: Copy> Journal<T> {
    /// How many marks a reader may always fall behind before it has to refresh everything.
    pub(crate) const HISTORY: usize = 4096;

    pub(crate) fn new() -> Self {
        Self {
            instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            marks: VecDeque::new(),
            forgotten: 0,
        }
    }

    /// Marks `item`, keeping the newest [`Journal::HISTORY`] marks.
    pub(crate) fn mark(&mut self, item: T) {
        self.mark_keeping(item, Self::HISTORY);
    }

    /// Marks `item`, keeping at least `history` of the newest marks.
    pub(crate) fn mark_keeping(&mut self, item: T, history: usize) {
        self.revision += 1;
        self.marks.push_back((self.revision, item));
        while self.marks.len() > history.max(Self::HISTORY) {
            if let Some((revision, _)) = self.marks.pop_front() {
                self.forgotten = revision;
            }
        }
    }

    pub(crate) fn version(&self) -> JournalVersion {
        JournalVersion {
            instance: self.instance,
            revision: self.revision,
        }
    }

    /// Appends every item marked since `version` into `marked`.
    ///
    /// [`ChangeCoverage::All`] means the reader has to refresh everything instead. It is returned
    /// for a version of another journal, and for one older than the history kept.
    pub(crate) fn since(&self, version: JournalVersion, marked: &mut Vec<T>) -> ChangeCoverage {
        if version.instance != self.instance
            || version.revision > self.revision
            || version.revision < self.forgotten
        {
            return ChangeCoverage::All;
        }
        // Marks are kept in revision order, so the newer ones are a suffix.
        let start = self
            .marks
            .partition_point(|(revision, _)| *revision <= version.revision);
        marked.extend(self.marks.range(start..).map(|(_, item)| *item));
        ChangeCoverage::Delta
    }
}
