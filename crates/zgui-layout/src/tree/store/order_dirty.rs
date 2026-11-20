//! The boxes whose children may paint in a different order than they did.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use zgui_dom::side::BoxKey;
use zgui_space::ChangeCoverage;

/// The identity and revision of one store's order journal at one moment.
///
/// A version from another store asks [`LayoutStore::order_dirty_since`] for a full refresh.
///
/// [`LayoutStore::order_dirty_since`]: crate::tree::store::LayoutStore::order_dirty_since
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OrderVersion {
    instance: u64,
    revision: u64,
}

/// Distinguishes journals whose revisions happen to hold the same number.
static NEXT_INSTANCE: AtomicU64 = AtomicU64::new(1);

/// A bounded history of the boxes marked order-dirty.
#[derive(Debug)]
pub(crate) struct OrderJournal {
    instance: u64,
    revision: u64,
    marks: VecDeque<(u64, BoxKey)>,
    forgotten: u64,
}

impl OrderJournal {
    /// How many marks a reader may fall behind before it has to refresh every box.
    const HISTORY: usize = 4096;

    pub(crate) fn new() -> Self {
        Self {
            instance: NEXT_INSTANCE.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            marks: VecDeque::new(),
            forgotten: 0,
        }
    }

    /// Marks the paint order of the children of `key` as possibly changed.
    pub(crate) fn mark(&mut self, key: BoxKey) {
        self.revision += 1;
        self.marks.push_back((self.revision, key));
        while self.marks.len() > Self::HISTORY {
            if let Some((revision, _)) = self.marks.pop_front() {
                self.forgotten = revision;
            }
        }
    }

    pub(crate) fn version(&self) -> OrderVersion {
        OrderVersion {
            instance: self.instance,
            revision: self.revision,
        }
    }

    pub(crate) fn since(&self, version: OrderVersion, marked: &mut Vec<BoxKey>) -> ChangeCoverage {
        if version.instance != self.instance
            || version.revision > self.revision
            || version.revision < self.forgotten
        {
            return ChangeCoverage::All;
        }
        marked.extend(
            self.marks
                .iter()
                .filter(|(revision, _)| *revision > version.revision)
                .map(|(_, key)| *key),
        );
        ChangeCoverage::Delta
    }
}
