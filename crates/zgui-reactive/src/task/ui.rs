//! Getting from another thread back onto the UI thread.
//!
//! A signal is `Send`, so a worker thread *can* write one and the wake edge will turn that write
//! into a frame. It should not. `Observer` and `Owner` are thread-local, so a `get` on a worker
//! silently fails to subscribe, and a `set` there walks the subscriber graph from a thread with no
//! frame around it — correct, but impossible to reason about, and one refactor away from touching
//! a `LocalStorage` handle and panicking.
//!
//! [`Ui`] is the supported answer. It is a `Send + Clone` handle to a UI thread; posting to it
//! queues a closure that runs at the start of the next [`flush`](crate::flush) — before the task
//! pool is polled, so what it writes settles in the same frame — and asks the platform for that
//! frame on the way.
//!
//! ```no_run
//! # use zgui_reactive::{RwSignal, ui};
//! # use zgui_reactive::prelude::*;
//! # fn example(progress: RwSignal<u8>) {
//! let ui = ui(); // taken on the UI thread
//! std::thread::spawn(move || {
//!     for step in 0..=100 {
//!         // ... do a step of the work ...
//!         ui.post(move || progress.set(step));
//!     }
//! });
//! # }
//! ```

use std::future::Future;
use std::sync::{Arc, Mutex};

use futures::channel::oneshot;

use crate::executor::assert_ui_thread;
use crate::executor::wake::WakeTarget;

/// One closure waiting to run on the UI thread.
type Job = Box<dyn FnOnce() + Send>;

/// The queue of one UI thread, and the way to ask that thread for a frame.
struct UiQueue {
    /// Closures posted but not yet run, oldest first.
    pending: Mutex<Vec<Job>>,
    /// Where this thread's wakes go, so a post asks for the frame that will run it.
    target: Arc<WakeTarget>,
}

thread_local! {
    /// This thread's queue. Created eagerly, like the wake target, so a handle can be taken
    /// before anything has been posted.
    static QUEUE: Arc<UiQueue> = Arc::new(UiQueue {
        pending: Mutex::new(Vec::new()),
        target: crate::executor::wake::target(),
    });
}

/// A handle to a UI thread, usable from any thread.
///
/// Take one on the UI thread with [`ui`] and carry it wherever the work goes: into a
/// `std::thread::spawn`, into a background task, into a callback a C library will call back on a
/// thread of its own.
#[derive(Clone)]
pub struct Ui {
    /// The queue this handle posts to.
    queue: Arc<UiQueue>,
}

impl Ui {
    /// Runs `f` on the UI thread, at the start of the next flush.
    ///
    /// Returns immediately; `f` has not run yet. Posting from the UI thread is allowed and means
    /// "not now, at the next flush" — including from inside a flush, where the closure joins the
    /// frame that is already owed rather than asking for a second one.
    pub fn post(&self, f: impl FnOnce() + Send + 'static) {
        self.queue
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Box::new(f));

        // A wake raised inside a flush is folded into the frame that flush already owes, exactly
        // as a task's wake is; only a post from outside one asks the platform for a redraw.
        if !crate::executor::frame::note_wake() {
            self.queue.target.ping();
        }
    }

    /// Runs `f` on the UI thread and resolves with what it returned.
    ///
    /// The form to use from a background task that needs an answer only the UI thread can give —
    /// reading a signal, asking the document something — before it carries on.
    ///
    /// # Panics
    ///
    /// When awaited, if `f` panicked, or if the UI thread went away before it ran.
    pub fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> impl Future<Output = T> + Send {
        let (send, receive) = oneshot::channel();
        self.post(move || {
            let _ = send.send(f());
        });
        async move {
            receive.await.expect(
                "a closure posted to the UI thread did not produce a value: it panicked, or the \
                 UI thread went away before the next flush",
            )
        }
    }
}

impl core::fmt::Debug for Ui {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Ui")
    }
}

/// A handle to this UI thread.
///
/// # Panics
///
/// In debug builds, if called off the UI thread. A handle is taken on the thread it refers to;
/// carrying one to another thread is the whole point, but making one there is a mistake.
#[track_caller]
#[must_use]
pub fn ui() -> Ui {
    assert_ui_thread("ui");
    Ui {
        queue: QUEUE.with(Arc::clone),
    }
}

/// Runs everything posted to this thread since the last drain.
///
/// Called by [`flush`](crate::flush) before the task pool is polled, so a posted closure's writes
/// are settled by the same frame that runs it rather than by the next one.
///
/// The queue is taken before any closure runs, so a closure that posts is queued for the next
/// flush instead of extending the walk it is part of — which is what stops a self-posting closure
/// from holding the frame for ever.
pub(crate) fn drain() {
    let Some(queue) = QUEUE.try_with(Arc::clone).ok() else {
        return; // the thread is being torn down; there is no frame left to run anything in
    };
    let jobs = {
        let mut pending = queue
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::mem::take(&mut *pending)
    };
    for job in jobs {
        job();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::executor::{TestWaker, flush, install, set_frame_waker};

    #[test]
    fn a_post_from_another_thread_runs_at_the_flush_and_asks_for_one_frame() {
        install().unwrap();
        let waker = Arc::new(TestWaker::default());
        set_frame_waker(waker.clone());
        waker.take();

        let ran = Arc::new(AtomicUsize::new(0));
        let handle = ui();
        let counter = Arc::clone(&ran);
        std::thread::spawn(move || {
            handle.post(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            });
        })
        .join()
        .unwrap();

        assert_eq!(ran.load(Ordering::SeqCst), 0, "nothing ran off the frame");
        assert_eq!(waker.take(), 1, "and one frame was asked for");

        flush();
        assert_eq!(ran.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_closure_that_posts_does_not_hold_the_flush() {
        install().unwrap();
        set_frame_waker(Arc::new(TestWaker::default()));

        let ran = Arc::new(AtomicUsize::new(0));
        fn repost(ran: Arc<AtomicUsize>) {
            let handle = ui();
            handle.post(move || {
                ran.fetch_add(1, Ordering::SeqCst);
                repost(ran);
            });
        }
        repost(Arc::clone(&ran));

        flush();
        assert_eq!(ran.load(Ordering::SeqCst), 1, "one flush ran one round");
        flush();
        assert_eq!(ran.load(Ordering::SeqCst), 2);
    }
}
