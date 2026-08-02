//! The wake edge: how work that becomes ready outside an input event reaches the frame loop.
//!
//! The task pool's own waker unparks the thread it lives on. That is worthless in a UI, where
//! the thread is not parked but blocked in the platform's event loop waiting for a window
//! message. Without a second edge, a task woken by a worker thread, a resolved async value or a
//! fired timer is marked ready and then waits for the user to move the mouse.
//!
//! So every spawned future is polled through a composite waker: waking it queues the task in
//! the pool *and* pings the [`FrameWaker`], which asks the platform for a redraw. The redraw
//! runs the frame, the frame flushes the executor, and the task is polled.

use std::sync::{Arc, RwLock};

/// The platform's redraw request, as the reactive layer sees it.
///
/// A wake can arrive on any thread at any time, so the implementation must be safe to call from
/// a worker thread, from a timer, and re-entrantly. It must also be *idempotent*: a hundred
/// wakes between two frames must cost one frame, not a hundred.
///
/// The windowing backend provides the real implementation; [`TestWaker`] is the one to use in
/// tests and in headless harnesses.
pub trait FrameWaker: Send + Sync + 'static {
    /// Requests that a frame be run soon.
    ///
    /// Called from arbitrary threads. Must not block, and must not assume a current owner or
    /// the UI thread.
    fn wake(&self);
}

/// Where wakes are sent on one UI thread.
///
/// Shared by every composite waker built on that thread, so installing a waker after effects
/// already exist still reaches them.
#[derive(Default)]
pub(crate) struct WakeTarget {
    /// The installed waker, if the platform has provided one yet.
    waker: RwLock<Option<Arc<dyn FrameWaker>>>,
}

impl WakeTarget {
    /// Pings the installed waker, if there is one.
    pub(crate) fn ping(&self) {
        let waker = self
            .waker
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Installs `waker`, replacing any previous one.
    fn set(&self, waker: Arc<dyn FrameWaker>) {
        *self
            .waker
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(waker);
    }
}

thread_local! {
    /// This thread's wake target. Created eagerly so composite wakers can hold it before a
    /// platform waker exists.
    static TARGET: Arc<WakeTarget> = Arc::new(WakeTarget::default());
}

/// Routes reactive wakes on this thread to `waker`.
///
/// Call once, from the UI thread, as soon as a window exists. Until it is called, a task woken
/// from another thread is queued but nothing asks for the frame that would poll it — so an
/// application that never calls this updates only when something else already caused a frame.
///
/// Wakes raised *during* a flush are not forwarded; they are folded into
/// [`FlushOutcome::needs_another_frame`](crate::executor::FlushOutcome::needs_another_frame)
/// instead, so an effect writing a signal costs one extra frame rather than a redraw request
/// per write.
pub fn set_frame_waker(waker: Arc<dyn FrameWaker>) {
    TARGET.with(|target| target.set(waker));
}

/// This thread's wake target.
pub(crate) fn target() -> Arc<WakeTarget> {
    TARGET.with(Arc::clone)
}

/// A [`FrameWaker`] that counts wakes instead of asking a platform for a frame.
///
/// Useful wherever there is no event loop: a test asserts that work became ready by asserting
/// that the count moved, and a headless harness polls the count to decide whether to run
/// another frame.
///
/// ```
/// use std::sync::Arc;
/// use zgui_reactive::{FrameWaker, TestWaker};
///
/// let waker = Arc::new(TestWaker::default());
/// assert_eq!(waker.count(), 0);
/// waker.wake();
/// assert_eq!(waker.count(), 1);
/// assert_eq!(waker.take(), 1);
/// assert_eq!(waker.count(), 0);
/// ```
#[derive(Debug, Default)]
pub struct TestWaker {
    /// Wakes recorded since the last [`TestWaker::take`].
    count: std::sync::atomic::AtomicUsize,
}

impl TestWaker {
    /// The number of wakes recorded since the last [`TestWaker::take`].
    #[must_use]
    pub fn count(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Returns the number of recorded wakes and resets the count to zero.
    pub fn take(&self) -> usize {
        self.count.swap(0, std::sync::atomic::Ordering::AcqRel)
    }
}

impl FrameWaker for TestWaker {
    fn wake(&self) {
        self.count.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
}
