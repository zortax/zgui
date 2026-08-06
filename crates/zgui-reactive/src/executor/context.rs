//! What the task pool is polled inside.
//!
//! Some futures only work while a particular runtime is *entered* — the thing they need is not the
//! thread they run on but the thread-local handle that lets them find a reactor. `tokio::time`,
//! `tokio::net` and every library built on them are like this: constructed outside a runtime
//! context they panic, however correctly they are polled.
//!
//! That is the difference between "tokio is available for background work" and "tokio is
//! available", and it is one indirection wide. A backend installs a context here, [`flush`] polls
//! the pool inside it, and a UI-thread task can then await a tokio timer or a socket directly —
//! its wake arriving on the reactor's thread and reaching the frame loop through the ordinary wake
//! edge, like every other wake.
//!
//! With nothing installed the pool is polled directly, which is the whole cost of the seam for an
//! application that never installs one.
//!
//! [`flush`]: crate::flush

use std::cell::RefCell;
use std::rc::Rc;

/// A runtime context the reactive task pool should be polled inside.
///
/// The implementation is expected to enter whatever ambient state its futures need, call `poll`
/// exactly once, and leave. It must not poll more than once, and must not swallow a panic: a task
/// that panics is propagated to the frame loop deliberately.
pub trait PollContext: 'static {
    /// Calls `poll` with this context entered.
    fn enter(&self, poll: &mut dyn FnMut());
}

thread_local! {
    /// The context this thread's flush polls inside, if a backend installed one.
    static CONTEXT: RefCell<Option<Rc<dyn PollContext>>> = const { RefCell::new(None) };
}

/// Polls this thread's task pool inside `context` from now on.
///
/// Call once, from the UI thread, before the first frame — `zgui-tokio` calls it for you.
/// Installing a second context replaces the first.
pub fn set_poll_context(context: Rc<dyn PollContext>) {
    CONTEXT.with_borrow_mut(|installed| *installed = Some(context));
}

/// Runs `poll` inside the installed context, or directly if there is none.
pub(crate) fn enter(mut poll: impl FnMut()) {
    let context = CONTEXT.try_with(|context| context.borrow().clone());
    match context {
        Ok(Some(context)) => context.enter(&mut poll),
        // No context installed, or a thread being torn down: poll directly rather than not at all.
        Ok(None) | Err(_) => poll(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn with_no_context_installed_the_poll_still_happens() {
        let polled = Cell::new(0);
        enter(|| polled.set(polled.get() + 1));
        assert_eq!(polled.get(), 1);
    }

    #[test]
    fn an_installed_context_wraps_the_poll() {
        thread_local! {
            static ENTERED: Cell<usize> = const { Cell::new(0) };
        }

        struct Counting;
        impl PollContext for Counting {
            fn enter(&self, poll: &mut dyn FnMut()) {
                ENTERED.set(ENTERED.get() + 1);
                poll();
            }
        }

        set_poll_context(Rc::new(Counting));
        let polled = Cell::new(0);
        enter(|| polled.set(polled.get() + 1));

        assert_eq!(ENTERED.get(), 1, "the context was entered");
        assert_eq!(polled.get(), 1, "and the poll happened inside it");

        CONTEXT.with_borrow_mut(|installed| *installed = None);
    }
}
