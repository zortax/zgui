//! The part of a sink that the reactive graph sees.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use reactive_graph::graph::{AnySource, AnySubscriber, ReactiveNode, Source, Subscriber};
use smallvec::SmallVec;

use crate::sink::slots::{Key, Queue};

/// A source has definitely changed: run without asking the sources.
const DIRTY: u8 = 1;
/// The key is in the queue, or set aside by the budget.
const QUEUED: u8 = 2;

/// The state of one sink that the graph and other threads can reach.
///
/// Sources hold this by a weak reference, and signals can be written on any thread, so it is
/// `Send + Sync` and holds nothing that is tied to the UI thread. The closure stays in the
/// thread's slots, under `key`.
pub(crate) struct Node {
    /// [`DIRTY`] and [`QUEUED`].
    state: AtomicU8,
    /// Where the closure is.
    key: Key,
    /// Where a mark sends `key`.
    queue: Arc<Queue>,
    /// What the last run read.
    sources: Mutex<SmallVec<[AnySource; 1]>>,
}

impl Node {
    /// Creates the node for the sink in `key`.
    pub(crate) fn new(key: Key, queue: Arc<Queue>) -> Self {
        Self {
            state: AtomicU8::new(0),
            key,
            queue,
            sources: Mutex::new(SmallVec::new()),
        }
    }

    /// Marks the sink, and queues it if it is not queued yet.
    fn mark(&self, bits: u8) {
        if self.state.fetch_or(bits | QUEUED, Ordering::AcqRel) & QUEUED == 0 {
            self.queue.push(self.key);
        }
    }

    /// Clears the queued mark before a run, so a write during the run queues the sink again.
    pub(crate) fn dequeue(&self) {
        self.state.fetch_and(!QUEUED, Ordering::AcqRel);
    }

    /// The number of sources the last run read.
    #[cfg(test)]
    pub(crate) fn source_count(&self) -> usize {
        self.sources().len()
    }

    /// The source list, also after a panic during a poll.
    fn sources(&self) -> std::sync::MutexGuard<'_, SmallVec<[AnySource; 1]>> {
        self.sources.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The type-erased subscriber the graph keeps for `node`.
///
/// The orphan rule forbids a [`ToAnySubscriber`](reactive_graph::graph::ToAnySubscriber) impl
/// for an `Arc`, so this is a function.
pub(crate) fn subscriber(node: &Arc<Node>) -> AnySubscriber {
    AnySubscriber(
        Arc::as_ptr(node) as usize,
        Arc::downgrade(node) as Weak<dyn Subscriber + Send + Sync>,
    )
}

impl ReactiveNode for Node {
    fn mark_dirty(&self) {
        self.mark(DIRTY);
    }

    fn mark_check(&self) {
        self.mark(0);
    }

    /// A sink has no subscribers.
    fn mark_subscribers_check(&self) {}

    /// Returns whether a source changed, and brings each memo up to date on the way.
    ///
    /// The lock is released before each source is asked, because a memo that re-runs can reach
    /// this node again.
    fn update_if_necessary(&self) -> bool {
        if self.state.fetch_and(!DIRTY, Ordering::AcqRel) & DIRTY != 0 {
            return true;
        }
        let mut index = 0;
        loop {
            let Some(source) = self.sources().get(index).cloned() else {
                return false;
            };
            if source.update_if_necessary() {
                return true;
            }
            index += 1;
        }
    }
}

impl Subscriber for Node {
    fn add_source(&self, source: AnySource) {
        let mut sources = self.sources();
        if !sources.contains(&source) {
            sources.push(source);
        }
    }

    /// Unsubscribes from every source, and keeps the list's allocation for the next run.
    fn clear_sources(&self, subscriber: &AnySubscriber) {
        let mut taken = std::mem::take(&mut *self.sources());
        for source in taken.drain(..) {
            source.remove_subscriber(subscriber);
        }
        let mut sources = self.sources();
        if sources.is_empty() {
            *sources = taken;
        }
    }
}
