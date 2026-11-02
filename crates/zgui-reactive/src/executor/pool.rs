//! The task pool: one per UI thread, polled only by [`flush`](crate::executor::flush).
//!
//! The pool is a slab of tasks and a queue of the ready ones. Each slot keeps one waker for the
//! life of the thread, and every task that takes the slot uses it. Thus the pool allocates nothing
//! for its tasks once the slab is at its peak. A cancelled task leaves its slot at once and is not
//! polled again.
//!
//! The pool does three things for every task that the task itself cannot:
//!
//! * the slot's waker queues the slot *and* asks the platform for the frame that will poll it;
//! * each poll is charged against the flush's iteration budget, so a cycle of effects cannot hold
//!   the frame;
//! * in debug builds, the poll asserts that it runs on the UI thread.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::Future;
use std::num::NonZeroU32;
use std::panic::Location;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll, Wake, Waker};

use any_spawner::{CustomExecutor, PinnedFuture, PinnedLocalFuture};

use crate::executor::budget::{Admission, TaskBudget};
use crate::executor::frame;
use crate::executor::ui_thread::is_ui_thread;
use crate::executor::wake::WakeTarget;
use crate::task::Cancellable;

/// The name of one task in its pool.
///
/// The generation changes each time the slot is freed, so a key kept after its task left the pool
/// does not name the task that takes the slot next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Key {
    /// The slot.
    index: u32,
    /// The slot's generation when the task took it.
    generation: NonZeroU32,
}

/// What a slot polls.
pub(crate) enum Body {
    /// A future spawned through the global executor, such as the task of an effect.
    Future(PinnedLocalFuture<()>),
    /// A task spawned through this crate. [`remove`] can cancel it.
    Task(Cancellable),
}

impl Body {
    /// Polls the future or the task.
    fn poll(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        match self {
            Self::Future(future) => future.as_mut().poll(cx),
            Self::Task(task) => Pin::new(task).poll(cx),
        }
    }
}

/// The part of a pool that its wakers reach, also from other threads.
struct Home {
    /// Slots woken on another thread, or while the pool was borrowed. The next poll queues them.
    remote: Mutex<Vec<u32>>,
    /// Where a wake outside the flush asks for a frame.
    target: Arc<WakeTarget>,
}

impl Home {
    /// The remote list, also after a panic during a poll.
    fn remote(&self) -> std::sync::MutexGuard<'_, Vec<u32>> {
        self.remote.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The waker of one slot. It wakes the task that holds the slot when the wake arrives.
///
/// A waker that a finished task left behind can thus wake the next task in its slot. That is a
/// spurious wake, which every future must accept.
struct SlotWaker {
    /// The pool of the slot.
    home: Arc<Home>,
    /// The slot.
    index: u32,
    /// Whether the slot is in the remote list. Many remote wakes between two polls push it once.
    remote: AtomicBool,
}

impl Wake for SlotWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        // Only a UI thread has a pool. The check keeps other threads from creating one.
        let queued = is_ui_thread()
            && POOL
                .try_with(|pool| match pool.try_borrow_mut() {
                    Ok(mut pool) if Arc::ptr_eq(&pool.home, &self.home) => {
                        pool.wake(self.index);
                        true
                    }
                    _ => false,
                })
                .unwrap_or(false);
        if !queued && !self.remote.swap(true, Ordering::AcqRel) {
            self.home.remote().push(self.index);
        }
        // A wake raised by an effect that is itself running inside the flush must not ask for a
        // redraw from inside the frame that is already running: the flush records that another
        // frame is owed and requests it once, at the end.
        if !frame::note_wake() {
            self.home.target.ping();
        }
    }
}

/// One task in the slab.
struct Entry {
    /// The task. `None` while it is polled.
    body: Option<Body>,
    /// This task's share of the flush's iteration budget.
    budget: TaskBudget,
    /// Where the task was spawned, when it was spawned through this crate's own API.
    spawned_at: Option<&'static Location<'static>>,
}

/// One slot.
struct Slot {
    /// Incremented when the slot is freed.
    generation: NonZeroU32,
    /// Whether the slot is in the ready queue for its current task.
    queued: bool,
    /// The slot's waker. `None` until the first poll, and while a poll holds it.
    waker: Option<Waker>,
    /// The state behind [`Slot::waker`]. `None` until the first poll.
    shared: Option<Arc<SlotWaker>>,
    /// The task in the slot, if one is.
    entry: Option<Entry>,
}

/// One thread's tasks.
///
/// The slot list keeps its peak length. The next task reuses a freed slot and its waker.
struct Pool {
    /// The slots, live and free.
    slots: Vec<Slot>,
    /// The free slots, most recently freed first.
    free: Vec<u32>,
    /// The slots to poll, in the order they were woken.
    ready: VecDeque<u32>,
    /// The slots the budget set aside in this flush. They go back in the queue when it ends.
    deferred: Vec<u32>,
    /// The buffer that the remote list is swapped with, so a drain allocates nothing.
    drained: Vec<u32>,
    /// How many tasks this thread has spawned.
    spawned: u64,
    /// What this pool's wakers share.
    home: Arc<Home>,
}

/// What [`Pool::next`] found.
enum Next {
    /// A task to poll, taken out of its slot with the slot's waker.
    Poll {
        /// The slot.
        index: u32,
        /// The task.
        body: Body,
        /// The slot's waker.
        waker: Waker,
    },
    /// A task the budget set aside, to report.
    Deferred(Option<&'static Location<'static>>),
}

impl Pool {
    /// An empty pool whose wakes go to this thread's wake target.
    fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            ready: VecDeque::new(),
            deferred: Vec::new(),
            drained: Vec::new(),
            spawned: 0,
            home: Arc::new(Home {
                remote: Mutex::new(Vec::new()),
                target: crate::executor::wake::target(),
            }),
        }
    }

    /// Puts `body` in a free slot and queues it.
    fn insert(&mut self, body: Body, spawned_at: Option<&'static Location<'static>>) -> Key {
        let index = self.free.pop().unwrap_or_else(|| {
            let index = u32::try_from(self.slots.len()).expect("fewer than 2^32 tasks");
            self.slots.push(Slot {
                generation: NonZeroU32::MIN,
                queued: false,
                waker: None,
                shared: None,
                entry: None,
            });
            index
        });
        let slot = &mut self.slots[index as usize];
        slot.entry = Some(Entry {
            body: Some(body),
            budget: TaskBudget::default(),
            spawned_at,
        });
        slot.queued = true;
        self.ready.push_back(index);
        self.spawned += 1;
        Key {
            index,
            generation: slot.generation,
        }
    }

    /// Queues the task in slot `index`, if there is one and it is not queued yet.
    fn wake(&mut self, index: u32) {
        if let Some(slot) = self.slots.get_mut(index as usize)
            && slot.entry.is_some()
            && !slot.queued
        {
            slot.queued = true;
            self.ready.push_back(index);
        }
    }

    /// Queues the slots that were woken on other threads. Returns whether there were any.
    fn drain_remote(&mut self) -> bool {
        std::mem::swap(&mut *self.home.remote(), &mut self.drained);
        if self.drained.is_empty() {
            return false;
        }
        let drained = std::mem::take(&mut self.drained);
        for &index in &drained {
            // Cleared before the wake, so a remote wake after this point pushes the slot again.
            if let Some(shared) = self
                .slots
                .get(index as usize)
                .and_then(|s| s.shared.as_ref())
            {
                shared.remote.store(false, Ordering::Release);
            }
            self.wake(index);
        }
        self.drained = drained;
        self.drained.clear();
        true
    }

    /// The next queued task the budget admits, or the next one it sets aside.
    fn next(&mut self) -> Option<Next> {
        loop {
            let Some(index) = self.ready.pop_front() else {
                if self.drain_remote() {
                    continue;
                }
                return None;
            };
            // A slot that is not queued was polled after this wake already, or was freed.
            let Some(slot) = self.slots.get_mut(index as usize) else {
                continue;
            };
            if !std::mem::replace(&mut slot.queued, false) {
                continue;
            }
            let Some(entry) = slot.entry.as_mut() else {
                continue;
            };
            if entry.body.is_none() {
                continue;
            }
            match entry.budget.admit(frame::generation()) {
                Admission::Run => {}
                Admission::Defer => {
                    self.deferred.push(index);
                    return Some(Next::Deferred(entry.spawned_at));
                }
                Admission::AlreadyDeferred => continue,
            }
            let body = entry.body.take()?;
            let waker = slot.waker.take().unwrap_or_else(|| {
                let shared = Arc::new(SlotWaker {
                    home: Arc::clone(&self.home),
                    index,
                    remote: AtomicBool::new(false),
                });
                slot.shared = Some(Arc::clone(&shared));
                Waker::from(shared)
            });
            return Some(Next::Poll { index, body, waker });
        }
    }

    /// Puts a polled task back in its slot. Returns the task if the slot is gone.
    fn restore(&mut self, index: u32, body: Body, waker: Waker) -> Option<Body> {
        let slot = &mut self.slots[index as usize];
        slot.waker = Some(waker);
        match slot.entry.as_mut() {
            Some(entry) => {
                entry.body = Some(body);
                None
            }
            None => Some(body),
        }
    }

    /// Frees slot `index`, and returns what it held for the caller to drop.
    fn release(&mut self, index: u32) -> Option<Entry> {
        let slot = self.slots.get_mut(index as usize)?;
        let entry = slot.entry.take()?;
        slot.generation = slot.generation.checked_add(1).unwrap_or(NonZeroU32::MIN);
        slot.queued = false;
        self.free.push(index);
        Some(entry)
    }

    /// Frees the slot `key` names, unless a poll holds its task.
    fn remove(&mut self, key: Key) -> Option<Entry> {
        let slot = self.slots.get(key.index as usize)?;
        let held = slot.entry.as_ref()?.body.is_some();
        if slot.generation != key.generation || !held {
            return None;
        }
        self.release(key.index)
    }

    /// Frees every slot, and returns the tasks for the caller to drop.
    fn take_all(&mut self) -> Vec<Entry> {
        self.ready.clear();
        self.deferred.clear();
        (0..self.slots.len())
            .filter_map(|index| self.release(u32::try_from(index).ok()?))
            .collect()
    }
}

thread_local! {
    /// This thread's tasks.
    ///
    /// Borrowed only for queue and slab updates. Tasks are polled and dropped outside the borrow,
    /// because a task can spawn, wake and cancel tasks.
    static POOL: RefCell<Pool> = RefCell::new(Pool::new());
    /// Whether a poll runs on this thread. A second poll is then a no-op.
    static POLLING: Cell<bool> = const { Cell::new(false) };
}

/// Creates this thread's pool.
///
/// Called while installing, so the pool and its wake target exist before the first spawn.
pub(crate) fn prepare() {
    POOL.with(|_| ());
}

/// The single-threaded executor the whole framework runs on.
///
/// There is no thread pool and no reactor behind it: tasks live on the UI thread and advance
/// only when the frame loop flushes them.
pub(crate) struct UiExecutor;

impl CustomExecutor for UiExecutor {
    /// Spawns a `Send` future — on the UI thread, exactly like a local one.
    ///
    /// The reactive engine reaches this path from constructors that have no local variant, so
    /// treating it as anything other than `spawn_local` would put reactive work on a second
    /// thread by accident.
    fn spawn(&self, future: PinnedFuture<()>) {
        debug_assert!(
            is_ui_thread(),
            "reactive tasks may only be spawned from the thread that installed the runtime"
        );
        spawn(Body::Future(future), None);
    }

    fn spawn_local(&self, future: PinnedLocalFuture<()>) {
        debug_assert!(
            is_ui_thread(),
            "reactive tasks may only be spawned from the thread that installed the runtime"
        );
        spawn(Body::Future(future), None);
    }

    fn poll_local(&self) {
        poll();
    }
}

/// Queues `body` on this thread's pool, recording `spawned_at` as the location to name if it
/// misbehaves.
///
/// Returns `None`, and drops `body`, on a thread whose pool is already gone.
pub(crate) fn spawn(body: Body, spawned_at: Option<&'static Location<'static>>) -> Option<Key> {
    POOL.try_with(|pool| pool.borrow_mut().insert(body, spawned_at))
        .ok()
}

/// Frees the slot `key` names and drops its task, without a poll and without a wake.
///
/// Does nothing while a poll holds the task: a task that cancels itself returns `Ready` from that
/// poll, and the poll frees the slot.
pub(crate) fn remove(key: Key) {
    let entry = POOL
        .try_with(|pool| {
            let borrowed = pool.try_borrow_mut();
            debug_assert!(
                borrowed.is_ok(),
                "a task was cancelled while its pool was borrowed, so its slot stays taken"
            );
            borrowed.ok()?.remove(key)
        })
        .ok()
        .flatten();
    drop(entry);
}

/// Frees the slot of a task when a poll of it unwinds.
struct Running(Option<u32>);

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(index) = self.0.take() {
            let entry = POOL
                .try_with(|pool| pool.try_borrow_mut().ok()?.release(index))
                .ok()
                .flatten();
            drop(entry);
        }
    }
}

/// Clears the polling flag however the poll ends.
struct Polling;

impl Drop for Polling {
    fn drop(&mut self) {
        let _ = POLLING.try_with(|polling| polling.set(false));
    }
}

/// Polls every ready task to a stall.
///
/// A poll that arrives while one is already running — an effect that flushes — does nothing. The
/// outer poll runs what the inner one would have run.
///
/// A task that panics is dropped, and the panic propagates to the caller. The tasks still queued
/// stay queued for the next poll.
pub(crate) fn poll() {
    if POLLING
        .try_with(|polling| polling.replace(true))
        .unwrap_or(true)
    {
        tracing::debug!("a reactive flush was requested from inside one, and was ignored");
        return;
    }
    let _polling = Polling;

    loop {
        let next = POOL
            .try_with(|pool| pool.borrow_mut().next())
            .ok()
            .flatten();
        let (index, mut body, waker) = match next {
            None => break,
            Some(Next::Deferred(spawned_at)) => {
                crate::executor::budget::report(spawned_at);
                frame::note_exhausted();
                continue;
            }
            Some(Next::Poll { index, body, waker }) => (index, body, waker),
        };
        debug_assert!(
            is_ui_thread(),
            "a reactive task was polled off the thread that installed the runtime"
        );

        let mut running = Running(Some(index));
        let done = body.poll(&mut Context::from_waker(&waker)).is_ready();
        running.0 = None;

        // What the slot gives back is dropped after the borrow, because dropping a task can
        // spawn, wake and cancel tasks.
        if done {
            let entry = POOL.with_borrow_mut(|pool| {
                pool.slots[index as usize].waker = Some(waker);
                pool.release(index)
            });
            drop(entry);
            drop(body);
        } else {
            let orphan = POOL.with_borrow_mut(|pool| pool.restore(index, body, waker));
            drop(orphan);
        }
    }
}

/// Puts the tasks the budget set aside back in the queue, and returns whether there were any.
///
/// This asks for no frame. The caller asks for one for them.
pub(crate) fn release_deferred() -> bool {
    POOL.try_with(|pool| {
        let mut pool = pool.borrow_mut();
        let deferred = std::mem::take(&mut pool.deferred);
        for &index in &deferred {
            pool.wake(index);
        }
        let any = !deferred.is_empty();
        pool.deferred = deferred;
        pool.deferred.clear();
        any
    })
    .unwrap_or(false)
}

/// Drops every task the pool holds.
///
/// The slots are freed before the tasks are dropped, so a task that a drop spawns lands in a free
/// slot. Returns `false` when a drop spawned a task, or when a poll is running.
pub(crate) fn clear() -> bool {
    if POLLING.with(Cell::get) {
        return false;
    }
    let (spawned, entries) = POOL.with_borrow_mut(|pool| (pool.spawned, pool.take_all()));
    drop(entries);
    POOL.with_borrow(|pool| pool.spawned) == spawned
}

/// The number of live tasks on this thread.
#[cfg(test)]
pub(crate) fn live() -> usize {
    POOL.with_borrow(|pool| pool.slots.iter().filter(|s| s.entry.is_some()).count())
}

/// The number of slots on this thread, live and free.
#[cfg(test)]
pub(crate) fn slots() -> usize {
    POOL.with_borrow(|pool| pool.slots.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A task that never finishes.
    fn pending() -> Body {
        Body::Future(Box::pin(std::future::pending()))
    }

    #[test]
    fn a_stale_key_does_not_remove_the_next_task_in_its_slot() {
        let first = spawn(pending(), None).unwrap();
        remove(first);
        assert_eq!(live(), 0, "the remove freed the slot at once");

        let second = spawn(pending(), None).unwrap();
        assert_eq!(second.index, first.index, "the next task reused the slot");
        remove(first);
        assert_eq!(live(), 1, "the old key named nothing");

        remove(second);
        assert_eq!(live(), 0);
    }

    #[test]
    fn a_finished_task_gives_its_slot_and_waker_to_the_next() {
        crate::executor::install().unwrap();
        for _ in 0..100 {
            spawn(Body::Future(Box::pin(async {})), None);
            poll();
        }
        assert_eq!(live(), 0);
        assert_eq!(slots(), 1, "every task used the same slot");
    }

    #[test]
    fn a_task_woken_many_times_between_polls_is_polled_once() {
        crate::executor::install().unwrap();
        let polls = std::rc::Rc::new(Cell::new(0));
        let waker = std::rc::Rc::new(RefCell::new(None::<Waker>));
        spawn(
            Body::Future(Box::pin({
                let polls = std::rc::Rc::clone(&polls);
                let waker = std::rc::Rc::clone(&waker);
                std::future::poll_fn(move |cx| {
                    polls.set(polls.get() + 1);
                    *waker.borrow_mut() = Some(cx.waker().clone());
                    Poll::<()>::Pending
                })
            })),
            None,
        );
        poll();
        assert_eq!(polls.get(), 1);

        let waker = waker.borrow_mut().take().unwrap();
        for _ in 0..1000 {
            waker.wake_by_ref();
        }
        poll();
        assert_eq!(polls.get(), 2, "a thousand wakes cost one poll");
    }

    #[test]
    fn a_wake_from_another_thread_is_polled_by_the_next_poll() {
        crate::executor::install().unwrap();
        let frames = Arc::new(crate::executor::TestWaker::default());
        crate::executor::set_frame_waker(frames.clone());
        let polls = std::rc::Rc::new(Cell::new(0));
        let (send, receive) = std::sync::mpsc::channel::<Waker>();
        spawn(
            Body::Future(Box::pin({
                let polls = std::rc::Rc::clone(&polls);
                std::future::poll_fn(move |cx| {
                    polls.set(polls.get() + 1);
                    let _ = send.send(cx.waker().clone());
                    Poll::<()>::Pending
                })
            })),
            None,
        );
        poll();
        let waker = receive.recv().unwrap();

        std::thread::spawn(move || {
            for _ in 0..1000 {
                waker.wake_by_ref();
            }
        })
        .join()
        .unwrap();
        assert_eq!(polls.get(), 1, "the wakes ran nothing on the other thread");
        assert_eq!(frames.count(), 1000, "each wake asked for a frame");
        let remote = POOL.with_borrow(|pool| pool.home.remote().len());
        assert_eq!(remote, 1, "and queued the slot once");
        poll();
        assert_eq!(polls.get(), 2, "the next poll found it");
    }
}
