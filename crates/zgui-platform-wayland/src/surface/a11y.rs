//! The channel an assistive technology talks to.
//!
//! # Why the tree is built by a closure and not passed in
//!
//! Building an accessibility tree costs a walk of the whole document. On a machine with no
//! assistive technology running — which is most machines, most of the time — that walk would be
//! pure waste on every frame, so the adapter is asked whether anything is listening and the walk
//! only happens if something is.
//!
//! # Why the adapter lives on the surface here
//!
//! On other desktops an adapter owns a window handle or a graph of platform objects and cannot
//! leave the thread that made it, so a backend has to keep it beside the loop and name it by
//! number. This desktop's adapter is a message channel to a bus connection on a thread of its own:
//! it is shareable, and every one of its handlers is documented as being called from another
//! thread. So it lives where it belongs — on the surface it speaks for — and a frame asking to
//! publish reaches it from wherever that frame ran.
//!
//! What the handlers must *not* do is answer. They run on the bus's thread, where the document is
//! not; so each of them turns into a wake, and the loop answers on the thread the document is on.
//!
//! # Why an update is applied on a thread of its own
//!
//! The adapter applies an update to its copy of the tree and works out what to tell the bus, and
//! for a subtree that moved that costs more than building the update did. The loop builds the
//! update and hands it to one worker thread that every surface shares, so a frame pays for the
//! build alone. The worker applies the updates in the order they were built, focus changes with
//! them. The handlers tell the loop whether anything is listening, so a frame still builds nothing
//! while nothing is.
//!
//! A frame takes no lock the worker holds, so it does not wait for the update before it. Only a
//! surface whose updates queue up faster than the worker applies them waits, so the queue stays
//! short.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use accesskit_unix::Adapter;
use zgui_platform::{SurfaceId, WakeReason, Waker};

/// The most updates of one surface that may wait for the worker before a frame waits with them.
const MAX_QUEUED: usize = 4;

/// The adapter of a surface, shared with the worker.
type Shared = Arc<Mutex<Option<Adapter>>>;

/// One surface's accessibility channel.
#[derive(Default)]
pub struct A11y {
    /// The adapter, once one has been made.
    adapter: Shared,
    /// Whether something is listening, as the handlers last said.
    listening: Arc<AtomicBool>,
    /// Whether the adapter waits for a whole tree, after something attached.
    fresh: Arc<AtomicBool>,
    /// How many updates the worker has not started on.
    queued: Arc<AtomicUsize>,
}

impl A11y {
    /// Opens a channel for the surface `id` names, answering through `waker`.
    pub fn open(id: SurfaceId, waker: Arc<dyn Waker>) -> Self {
        let listening = Arc::new(AtomicBool::new(false));
        let fresh = Arc::new(AtomicBool::new(false));
        let adapter = Adapter::new(
            Activation {
                id,
                waker: Arc::clone(&waker),
                listening: Arc::clone(&listening),
                fresh: Arc::clone(&fresh),
            },
            Action {
                waker: Arc::clone(&waker),
            },
            Deactivation {
                listening: Arc::clone(&listening),
            },
        );
        Self {
            adapter: Arc::new(Mutex::new(Some(adapter))),
            listening,
            fresh,
            queued: Arc::default(),
        }
    }

    /// Publishes an update, building it only if something is listening.
    ///
    /// The update is built here and applied on the worker. Nothing listens to a channel that was
    /// never opened or is closed, so neither builds anything.
    pub fn publish(&self, build: &mut dyn FnMut() -> TreeUpdate) {
        if !self.listening.load(Ordering::Acquire) {
            return;
        }
        if self.queued.load(Ordering::Acquire) >= MAX_QUEUED {
            // The worker is behind. Waiting for the update it is applying bounds the queue.
            drop(lock(&self.adapter));
        }
        let update = build();
        let adapter = Arc::clone(&self.adapter);
        let fresh = Arc::clone(&self.fresh);
        let queued = Arc::clone(&self.queued);
        queued.fetch_add(1, Ordering::AcqRel);
        apply(move || {
            queued.fetch_sub(1, Ordering::AcqRel);
            if admits(&fresh, &update)
                && let Some(adapter) = lock(&adapter).as_mut()
            {
                adapter.update_if_active(|| update);
            }
        });
    }

    /// Tells the channel whether this window has the keyboard.
    ///
    /// A screen reader follows the focused window, so one that is never told stays pointed at
    /// whichever window was focused when it attached.
    pub fn focused(&self, focused: bool) {
        let adapter = Arc::clone(&self.adapter);
        apply(move || {
            if let Some(adapter) = lock(&adapter).as_mut() {
                adapter.update_window_focus_state(focused);
            }
        });
    }

    /// Closes the channel, with the surface it spoke for.
    pub fn close(&self) {
        self.listening.store(false, Ordering::Release);
        drop(lock(&self.adapter).take());
    }
}

/// Whether an adapter takes `update`, and records that it took a whole tree.
///
/// An adapter that something attached to takes a whole tree first. A difference built before the
/// attachment refers to nodes the adapter does not hold.
fn admits(fresh: &AtomicBool, update: &TreeUpdate) -> bool {
    if !fresh.load(Ordering::Acquire) {
        return true;
    }
    if update.tree.is_none() {
        return false;
    }
    fresh.store(false, Ordering::Release);
    true
}

/// The adapter in `shared`, recovering from a panic on another thread.
fn lock(shared: &Shared) -> std::sync::MutexGuard<'_, Option<Adapter>> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Work for the worker.
type Job = Box<dyn FnOnce() + Send>;

/// Runs `job` on the worker, after every job handed over before it. Where no worker can run, the
/// job runs here.
fn apply(job: impl FnOnce() + Send + 'static) {
    static WORKER: OnceLock<Option<Mutex<Sender<Job>>>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (sender, jobs) = channel::<Job>();
        std::thread::Builder::new()
            .name("zgui-a11y".into())
            .spawn(move || {
                for job in jobs {
                    // A panic in the adapter loses one update and keeps the worker.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job));
                }
            })
            .ok()
            .map(|_| Mutex::new(sender))
    });
    let job: Job = Box::new(job);
    let unsent = match worker {
        Some(sender) => sender
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .send(job)
            .err()
            .map(|error| error.0),
        None => Some(job),
    };
    if let Some(job) = unsent {
        job();
    }
}

impl core::fmt::Debug for A11y {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("A11y")
            .field("attached", &lock(&self.adapter).is_some())
            .finish()
    }
}

/// Something attached and wants a tree.
struct Activation {
    /// Which surface it wants one for.
    id: SurfaceId,
    /// How the loop is told.
    waker: Arc<dyn Waker>,
    /// Whether something is listening.
    listening: Arc<AtomicBool>,
    /// Whether the adapter waits for a whole tree.
    fresh: Arc<AtomicBool>,
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        // Nothing can be built here: this runs on the bus's thread and the document is on the
        // loop's. The contract has an answer for exactly this — a wake that forces a build even
        // when nothing is dirty, because the tree has never been produced and a dirty check has
        // nothing to notice.
        self.fresh.store(true, Ordering::Release);
        self.listening.store(true, Ordering::Release);
        self.waker.wake(WakeReason::A11yTreeRequested(self.id));
        None
    }
}

/// Something asked for an action.
struct Action {
    /// How the loop is told.
    waker: Arc<dyn Waker>,
}

impl ActionHandler for Action {
    fn do_action(&mut self, request: ActionRequest) {
        // Queued rather than performed, which the trait documents as the preferred behaviour and
        // which is the only correct one here: performing it means dispatching into the document.
        self.waker.wake(WakeReason::A11yAction(request));
    }
}

/// Everything detached.
struct Deactivation {
    /// Whether something is listening.
    listening: Arc<AtomicBool>,
}

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {
        // Nothing to tell the application: what a deactivation means here is that the next
        // publish builds nothing.
        self.listening.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;
    use std::sync::mpsc::channel;

    use super::{A11y, admits, apply};
    use accesskit::{NodeId, Tree, TreeId, TreeUpdate};

    fn update(tree: bool) -> TreeUpdate {
        TreeUpdate {
            nodes: Vec::new(),
            tree: tree.then(|| Tree::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus: NodeId(0),
        }
    }

    #[test]
    fn a_fresh_adapter_takes_a_whole_tree_before_any_difference() {
        let fresh = AtomicBool::new(true);
        assert!(
            !admits(&fresh, &update(false)),
            "a difference waits for the tree"
        );
        assert!(admits(&fresh, &update(true)));
        assert!(
            admits(&fresh, &update(false)),
            "after the tree, differences apply"
        );
    }

    #[test]
    fn the_worker_runs_jobs_in_the_order_they_came() {
        let (sender, received) = channel();
        for index in 0..100 {
            let sender = sender.clone();
            apply(move || sender.send(index).expect("the test listens"));
        }
        let order: Vec<i32> = received.iter().take(100).collect();
        assert_eq!(order, (0..100).collect::<Vec<_>>());
    }

    #[test]
    fn a_panicking_job_keeps_the_worker() {
        apply(|| panic!("an adapter failed"));
        let (sender, received) = channel();
        apply(move || sender.send(()).expect("the test listens"));
        assert!(received.recv().is_ok());
    }

    #[test]
    fn a_surface_with_no_channel_publishes_nothing_and_builds_nothing() {
        // The walk of the document is what publishing costs, so a window nothing is listening to
        // must not perform it — and a window with no channel at all certainly must not.
        let closed = A11y::default();
        let mut built = false;
        closed.publish(&mut || {
            built = true;
            unreachable!("the tree must not be walked when nothing is listening")
        });
        assert!(!built, "the tree was walked with nothing listening");
    }

    #[test]
    fn a_closed_channel_answers_every_later_request_with_nothing() {
        let closed = A11y::default();
        closed.focused(true);
        closed.close();
        let mut built = false;
        closed.publish(&mut || {
            built = true;
            TreeUpdate {
                nodes: Vec::new(),
                tree: Some(Tree::new(NodeId(0))),
                tree_id: TreeId::ROOT,
                focus: NodeId(0),
            }
        });
        assert!(!built);
    }
}
