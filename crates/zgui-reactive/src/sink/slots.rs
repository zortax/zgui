//! The UI thread's side of every sink: its closure, its queue, and the drain that runs them.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex, PoisonError};

use reactive_graph::graph::{ReactiveNode, Subscriber, WithObserver};
use reactive_graph::owner::WeakOwner;

use crate::executor::budget::{Admission, TaskBudget};
use crate::executor::frame;
use crate::executor::wake::{WakeTarget, target};
use crate::sink::node::{Node, subscriber};

/// The name of one slot, and of the sink that holds it.
///
/// The generation changes each time the slot is freed, so a key that was queued before its sink
/// was dropped does not run the sink that takes the slot next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    /// The slot.
    index: u32,
    /// The slot's generation when this key was made.
    generation: NonZeroU32,
}

/// The keys of the sinks that wait for the flush, in the order they were marked.
///
/// Shared with each node, because a signal written on another thread marks its sinks there.
pub(crate) struct Queue {
    /// The marked keys.
    keys: Mutex<VecDeque<Key>>,
    /// Where a mark outside the flush asks for a frame.
    target: Arc<WakeTarget>,
}

impl Queue {
    /// Queues `key`, and asks for a frame unless this thread's flush runs it.
    pub(crate) fn push(&self, key: Key) {
        self.keys().push_back(key);
        if !frame::is_running() {
            self.target.ping();
        }
    }

    /// The next key, if there is one.
    fn pop(&self) -> Option<Key> {
        self.keys().pop_front()
    }

    /// The key list, also after a panic during a poll.
    fn keys(&self) -> std::sync::MutexGuard<'_, VecDeque<Key>> {
        self.keys.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What the UI thread keeps for one live sink.
struct Entry {
    /// The node the graph sees.
    node: Arc<Node>,
    /// The closure. `None` while it runs, and for good after it panics.
    run: Option<Box<dyn FnMut()>>,
    /// The owner that was current when the sink was made. Later runs run under it, and with no
    /// owner once it is disposed of.
    owner: Option<WeakOwner>,
    /// The runs left to the sink in this flush.
    budget: TaskBudget,
}

/// One slot.
struct Slot {
    /// Incremented when the slot is freed.
    generation: NonZeroU32,
    /// The sink in the slot, if one is.
    entry: Option<Entry>,
}

/// Every sink on this thread.
///
/// The slot list keeps its peak length. A freed slot is about 50 bytes and the next sink reuses
/// it.
#[derive(Default)]
struct Slots {
    /// The slots, live and free.
    slots: Vec<Slot>,
    /// The free slots, reused before the list grows.
    free: Vec<u32>,
    /// The number of live sinks.
    live: usize,
}

impl Slots {
    /// Puts a new node in a free slot, with no closure yet.
    fn insert(&mut self, queue: Arc<Queue>) -> (Key, Arc<Node>) {
        let index = self.free.pop().unwrap_or_else(|| {
            let index = u32::try_from(self.slots.len()).expect("fewer than 2^32 sinks");
            self.slots.push(Slot {
                generation: NonZeroU32::MIN,
                entry: None,
            });
            index
        });
        let slot = &mut self.slots[index as usize];
        let key = Key {
            index,
            generation: slot.generation,
        };
        let node = Arc::new(Node::new(key, queue));
        slot.entry = Some(Entry {
            node: Arc::clone(&node),
            run: None,
            owner: None,
            budget: TaskBudget::default(),
        });
        self.live += 1;
        (key, node)
    }

    /// The live sink `key` names.
    fn get_mut(&mut self, key: Key) -> Option<&mut Entry> {
        let slot = self.slots.get_mut(key.index as usize)?;
        if slot.generation != key.generation {
            return None;
        }
        slot.entry.as_mut()
    }

    /// Frees the slot `key` names, and returns what it held for the caller to drop.
    fn remove(&mut self, key: Key) -> Option<Entry> {
        let slot = self.slots.get_mut(key.index as usize)?;
        if slot.generation != key.generation {
            return None;
        }
        let entry = slot.entry.take()?;
        slot.generation = slot.generation.checked_add(1).unwrap_or(NonZeroU32::MIN);
        self.free.push(key.index);
        self.live -= 1;
        Some(entry)
    }
}

thread_local! {
    /// This thread's sinks.
    static SLOTS: RefCell<Slots> = RefCell::default();
    /// This thread's queue.
    static QUEUE: Arc<Queue> = Arc::new(Queue {
        keys: Mutex::new(VecDeque::new()),
        target: target(),
    });
    /// The keys the budget set aside in this flush. They go back in the queue when it ends.
    static DEFERRED: RefCell<Vec<Key>> = const { RefCell::new(Vec::new()) };
}

/// Makes a sink and runs `run` for the first time, with the sink as the observer.
pub(crate) fn create(mut run: impl FnMut() + 'static) -> Key {
    let owner = reactive_graph::owner::Owner::current().map(|owner| owner.downgrade());
    let queue = QUEUE.with(Arc::clone);
    let (key, node) = SLOTS.with_borrow_mut(|slots| slots.insert(queue));
    // Frees the slot if the first run panics.
    let guard = Release(Some(key));
    subscriber(&node).with_observer(&mut run);
    SLOTS.with_borrow_mut(|slots| {
        let entry = slots.get_mut(key).expect("a sink outlives its first run");
        entry.run = Some(Box::new(run));
        entry.owner = owner;
    });
    std::mem::forget(guard);
    key
}

/// Frees a slot when it is dropped.
struct Release(Option<Key>);

impl Drop for Release {
    fn drop(&mut self) {
        if let Some(key) = self.0.take() {
            release(key);
        }
    }
}

/// Frees the slot `key` names. Its closure is dropped after the slots are released, because
/// dropping it can drop other sinks.
pub(crate) fn release(key: Key) {
    let entry = SLOTS
        .try_with(|slots| slots.borrow_mut().remove(key))
        .ok()
        .flatten();
    drop(entry);
}

/// The node of the live sink `key` names.
#[cfg(test)]
pub(crate) fn node(key: Key) -> Option<Arc<Node>> {
    SLOTS.with_borrow_mut(|slots| slots.get_mut(key).map(|entry| Arc::clone(&entry.node)))
}

/// The number of live sinks on this thread.
pub(crate) fn live() -> usize {
    SLOTS.try_with(|slots| slots.borrow().live).unwrap_or(0)
}

/// Runs every queued sink, in queue order, until the queue is empty.
///
/// A key is taken off the queue before its sink runs, so a sink that panics leaves the keys
/// behind it for the next flush.
pub(crate) fn drain() {
    let _ = QUEUE.try_with(|queue| {
        while let Some(key) = queue.pop() {
            run(key);
        }
    });
}

/// Runs the sink `key` names, if it is live and a source changed.
fn run(key: Key) {
    let generation = frame::generation();
    let taken = SLOTS.with_borrow_mut(|slots| {
        let entry = slots.get_mut(key)?;
        match entry.budget.admit(generation) {
            Admission::Run => {}
            Admission::Defer => {
                crate::executor::budget::report(None);
                // The queued mark stays set, so nothing queues the key a second time.
                DEFERRED.with_borrow_mut(|deferred| deferred.push(key));
                frame::note_exhausted();
                return None;
            }
            Admission::AlreadyDeferred => return None,
        }
        let run = entry.run.take()?;
        Some((Arc::clone(&entry.node), run, entry.owner.clone()))
    });
    let Some((node, mut run, owner)) = taken else {
        return;
    };

    node.dequeue();
    let subscriber = subscriber(&node);
    // As a render effect does: memos the sink read are brought up to date with the sink as the
    // observer, so a memo that changes does not mark the sink a second time.
    if subscriber.with_observer(|| node.update_if_necessary()) {
        node.clear_sources(&subscriber);
        match owner.as_ref().and_then(WeakOwner::upgrade) {
            Some(owner) => owner.with(|| subscriber.with_observer(&mut run)),
            None => subscriber.with_observer(&mut run),
        }
    }

    // The sink can be dropped during its own run; its closure is then dropped here.
    let orphan = SLOTS.with_borrow_mut(|slots| match slots.get_mut(key) {
        Some(entry) => {
            entry.run = Some(run);
            None
        }
        None => Some(run),
    });
    drop(orphan);
}

/// Puts the keys the budget set aside back in the queue, and returns whether there were any.
///
/// They are not run again in this flush. The caller asks for another frame for them.
pub(crate) fn release_deferred() -> bool {
    let deferred = DEFERRED
        .try_with(|deferred| std::mem::take(&mut *deferred.borrow_mut()))
        .unwrap_or_default();
    if deferred.is_empty() {
        return false;
    }
    let _ = QUEUE.try_with(|queue| queue.keys().extend(deferred));
    true
}

/// Drops the closure of every live sink, and returns whether there was one.
///
/// The sinks keep their slots, so their handles still free them, and they do not run again. The
/// closures are dropped after the slots are released, because dropping one can drop other sinks.
pub(crate) fn clear() -> bool {
    let runs: Vec<_> = SLOTS
        .try_with(|slots| {
            slots
                .borrow_mut()
                .slots
                .iter_mut()
                .filter_map(|slot| slot.entry.as_mut()?.run.take())
                .collect()
        })
        .unwrap_or_default();
    let cleared = !runs.is_empty();
    drop(runs);
    cleared
}
