//! Cancellation, and dropping what a cancelled task captured.
//!
//! The pool a task lives in hands out no handle: `futures::executor::LocalPool` can be told to
//! run, and nothing else. Cancellation is therefore a wrapper rather than a pool operation — the
//! future is boxed into a slot the wrapper and the canceller both hold, and cancelling empties
//! that slot.
//!
//! Emptying it *synchronously* is the point. An owner is disposed of before the frame that
//! follows it — see [`Mounted`](crate::Mounted) — so a task cancelled by an unmount must release
//! the node handles, element references and view state it captured inside `unmount`, not at the
//! next flush. A task whose captures outlive its owner by a frame is the same defect the owner
//! tree exists to rule out.

use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// A task's future, once boxed, as the slot holds it.
type Boxed = Pin<Box<dyn Future<Output = ()>>>;

/// The state a task shares with its handle and with the set its owner holds.
///
/// One of these exists per spawned task. It is what a [`Task`](crate::Task) points at, what a
/// [`TaskSet`](crate::task::set::TaskSet) collects, and what the pool's wrapper polls through.
pub(crate) struct Cancel {
    /// Whether the task has been cancelled or has run to completion.
    ///
    /// One flag for both, because nothing may distinguish them: a task that has finished needs no
    /// cancelling, and a task that was cancelled will never finish.
    over: Cell<bool>,
    /// The future, until it completes or is cancelled.
    ///
    /// Borrowed for the duration of a poll, which is what makes a cancel raised from inside the
    /// task's own body fall back to the flag rather than panic.
    slot: RefCell<Option<Boxed>>,
    /// The waker of the last poll, so a cancelled task is reaped by the pool rather than left in
    /// it holding a shell.
    waker: RefCell<Option<Waker>>,
    /// What to run once the task is over, to take it out of the set its owner holds.
    ///
    /// A hook rather than a back-pointer, so this file knows nothing about how tasks are
    /// collected. It runs exactly once, on whichever of the two paths reaches the end first.
    release: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl Cancel {
    /// Wraps `future` in fresh cancellation state.
    pub(crate) fn new(future: impl Future<Output = ()> + 'static) -> Rc<Self> {
        Rc::new(Self {
            over: Cell::new(false),
            slot: RefCell::new(Some(Box::pin(future))),
            waker: RefCell::new(None),
            release: RefCell::new(None),
        })
    }

    /// Whether the task has been cancelled or has finished.
    pub(crate) fn is_over(&self) -> bool {
        self.over.get()
    }

    /// Arranges for `release` to run when the task is over.
    ///
    /// Called once, by whoever is holding the task, straight after it is spawned. A task that is
    /// already over runs it immediately rather than never — a future that finished on its first
    /// poll must not be left in the set for ever.
    pub(crate) fn on_release(&self, release: impl FnOnce() + 'static) {
        if self.over.get() {
            release();
            return;
        }
        *self.release.borrow_mut() = Some(Box::new(release));
    }

    /// Marks the task over and takes it out of whatever is holding it.
    fn finish(&self) {
        self.over.set(true);
        let release = self
            .release
            .try_borrow_mut()
            .ok()
            .and_then(|mut release| release.take());
        if let Some(release) = release {
            release();
        }
    }

    /// Cancels the task, dropping what its future captured.
    ///
    /// Dropping happens here whenever it can, so an unmount frees a task's captures before it
    /// returns. The one case it cannot is a cancel raised from inside the task's own poll — an
    /// effect that disposes of its scope while the task that owns it is running — where the slot
    /// is already borrowed. That path sets the flag instead, and the poll in progress empties the
    /// slot the moment it returns.
    pub(crate) fn cancel(&self) {
        if self.over.get() {
            return;
        }
        self.finish();
        if let Ok(mut slot) = self.slot.try_borrow_mut() {
            *slot = None;
        }
        // The pool still holds the wrapper. Waking it is what makes the next flush poll it once,
        // see the flag and drop it, rather than leaving an empty task in the pool for ever.
        let waker = self
            .waker
            .try_borrow_mut()
            .ok()
            .and_then(|mut waker| waker.take());
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

/// The future the pool actually holds: a task, plus the state that can call it off.
///
/// `Unpin` by construction — it owns nothing but an `Rc` — which is what the pool's own wrapper
/// requires.
pub(crate) struct Cancellable {
    /// The task's shared state.
    cancel: Rc<Cancel>,
}

impl Cancellable {
    /// Wraps `cancel` as the future to spawn.
    pub(crate) fn new(cancel: Rc<Cancel>) -> Self {
        Self { cancel }
    }
}

impl Future for Cancellable {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let cancel = &self.cancel;
        if cancel.is_over() {
            return Poll::Ready(());
        }

        // Kept so a later cancel can have the pool reap this task rather than hold an empty one.
        if let Ok(mut waker) = cancel.waker.try_borrow_mut() {
            *waker = Some(cx.waker().clone());
        }

        let Ok(mut slot) = cancel.slot.try_borrow_mut() else {
            // A re-entrant poll. The outer one owns the future; this one has nothing to do and
            // must not report the task finished.
            return Poll::Pending;
        };
        let Some(future) = slot.as_mut() else {
            cancel.finish();
            return Poll::Ready(());
        };

        match future.as_mut().poll(cx) {
            Poll::Ready(()) => {
                *slot = None;
                cancel.finish();
                Poll::Ready(())
            }
            // A cancel raised by the body just now could not empty the slot, because this poll
            // holds it. Emptying it here is what keeps "cancelling drops the captures" true on
            // that path too, one statement later rather than one frame later.
            Poll::Pending if cancel.is_over() => {
                *slot = None;
                Poll::Ready(())
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell as StdCell;
    use std::rc::Rc as StdRc;

    use super::*;

    /// A value that records when it is dropped, to prove a cancel released the captures.
    struct Witness(StdRc<StdCell<bool>>);

    impl Drop for Witness {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    #[test]
    fn cancelling_drops_the_futures_captures_at_once() {
        let dropped = StdRc::new(StdCell::new(false));
        let witness = Witness(StdRc::clone(&dropped));
        let cancel = Cancel::new(async move {
            let _held = witness;
            std::future::pending::<()>().await;
        });

        assert!(!dropped.get());
        cancel.cancel();
        assert!(
            dropped.get(),
            "the captures went away inside `cancel`, not at the next poll"
        );
        assert!(cancel.is_over());
    }

    #[test]
    fn cancelling_twice_does_nothing_the_second_time() {
        let cancel = Cancel::new(async {});
        cancel.cancel();
        cancel.cancel();
        assert!(cancel.is_over());
    }
}
