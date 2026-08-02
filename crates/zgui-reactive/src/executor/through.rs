//! The adapter that gives every spawned task the wake edge and the iteration budget.

use std::future::Future;
use std::panic::Location;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use futures::task::ArcWake;

use crate::executor::budget::{Admission, TaskBudget};
use crate::executor::frame;
use crate::executor::ui_thread::is_ui_thread;
use crate::executor::wake::WakeTarget;

/// Wraps a spawned future so that waking it also reaches the frame loop.
///
/// Every task the pool holds is wrapped in one of these. It does three things, none of which the
/// task itself can do:
///
/// * polls the inner future through a composite waker, so a wake queues the task *and* asks the
///   platform for the frame that will poll it;
/// * charges the poll against the flush's iteration budget, so a cycle of effects cannot hold
///   the frame;
/// * asserts, in debug builds, that the task is running on the UI thread.
pub(crate) struct WakeThrough<F> {
    /// The wrapped task.
    future: F,
    /// Where this thread's wakes are sent.
    target: Arc<WakeTarget>,
    /// The composite waker handed to `future`, and the pool waker it was built from.
    waker: Option<(Waker, Waker)>,
    /// This task's share of the flush's iteration budget.
    budget: TaskBudget,
    /// Where the task was spawned, when it was spawned through this crate's own API.
    spawned_at: Option<&'static Location<'static>>,
}

impl<F> WakeThrough<F> {
    /// Wraps `future`, recording `spawned_at` as the location to name if it misbehaves.
    pub(crate) fn new(future: F, spawned_at: Option<&'static Location<'static>>) -> Self {
        Self {
            future,
            target: crate::executor::wake::target(),
            waker: None,
            budget: TaskBudget::default(),
            spawned_at,
        }
    }

    /// Returns the composite waker for `pool`, rebuilding it only when the pool's waker changed.
    fn composite(&mut self, pool: &Waker) -> Waker {
        match &self.waker {
            Some((cached, composite)) if cached.will_wake(pool) => composite.clone(),
            _ => {
                let composite = futures::task::waker(Arc::new(Composite {
                    pool: pool.clone(),
                    target: Arc::clone(&self.target),
                }));
                self.waker = Some((pool.clone(), composite.clone()));
                composite
            }
        }
    }
}

impl<F> Future for WakeThrough<F>
where
    F: Future<Output = ()> + Unpin,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        debug_assert!(
            is_ui_thread(),
            "a reactive task was polled off the thread that installed the runtime"
        );
        let this = self.get_mut();

        match this.budget.admit(frame::generation()) {
            Admission::Run => {}
            Admission::Defer => {
                crate::executor::budget::report(this.spawned_at);
                frame::defer(cx.waker().clone());
                return Poll::Pending;
            }
            Admission::AlreadyDeferred => return Poll::Pending,
        }

        let waker = this.composite(cx.waker());
        Pin::new(&mut this.future).poll(&mut Context::from_waker(&waker))
    }
}

/// The waker a task is actually polled with: the pool's, plus the frame loop's.
struct Composite {
    /// Marks the task ready in the pool it lives in.
    pool: Waker,
    /// Asks the platform for the frame that will poll the pool.
    target: Arc<WakeTarget>,
}

impl ArcWake for Composite {
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.pool.wake_by_ref();
        // A wake raised by an effect that is itself running inside the flush must not ask for a
        // redraw from inside the frame that is already running: the flush records that another
        // frame is owed and requests it once, at the end.
        if !frame::note_wake() {
            arc_self.target.ping();
        }
    }
}
