//! The wait after which a toast asks to go, and what stops it.

use core::cell::RefCell;
use core::time::Duration;
use std::rc::Rc;

use zgui::view::{TimeoutHandle, Timers};

/// One toast's deadline, which can be taken away and put back.
///
/// Scheduled on the framework's own heap rather than on a thread, and cancelled while the pointer is
/// on the stack: a message that disappeared while it was being read would be a message that might as
/// well not have been shown. Moving off starts the wait again from the beginning, which is the honest
/// thing to do without a clock to ask how much of it was left.
///
/// A toast with no deadline — [`Toast::persistent`](crate::Toast::persistent) — has nothing to
/// schedule, and every call here does nothing at all rather than being a case its caller has to know
/// about.
#[derive(Clone)]
pub(crate) struct Expiry {
    /// How long the wait is, or `None` for a toast that waits to be dismissed.
    after: Option<Duration>,
    /// The window's clock, taken in the component's body because a listener runs outside it.
    clock: Option<Timers>,
    /// What is pending, held so that dropping it cancels the wait.
    pending: Rc<RefCell<Option<TimeoutHandle>>>,
    /// What the wait ends in.
    ask: Rc<dyn Fn()>,
}

impl Expiry {
    /// A deadline of `after` that calls `ask` when it runs out.
    pub(crate) fn new(after: Option<Duration>, ask: impl Fn() + 'static) -> Self {
        Self {
            after,
            clock: Timers::current(),
            pending: Rc::new(RefCell::new(None)),
            ask: Rc::new(ask),
        }
    }

    /// Starts the wait, if it is not already running.
    pub(crate) fn start(&self) {
        let (Some(after), Some(clock)) = (self.after, self.clock.clone()) else {
            return;
        };
        let mut pending = self.pending.borrow_mut();
        if pending.is_some() {
            return;
        }
        let ask = Rc::clone(&self.ask);
        *pending = Some(clock.set_timeout(after, move || ask()));
    }

    /// Takes the wait away.
    pub(crate) fn stop(&self) {
        if let Some(pending) = self.pending.borrow_mut().take() {
            pending.cancel();
        }
    }

    /// Whether a wait is running.
    #[cfg(test)]
    pub(crate) fn is_waiting(&self) -> bool {
        self.pending.borrow().is_some()
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui::reactive::{Mounted, install};

    use super::Expiry;

    /// Runs `body` inside a mounted reactive scope, which is where asking for the clock is allowed.
    ///
    /// There is no window in it, so the clock is `None` — which is the case being asked about.
    fn mounted(body: impl FnOnce()) {
        install().ok();
        let scope = Mounted::new();
        scope.with(body);
        scope.unmount();
    }

    #[test]
    fn a_toast_that_waits_to_be_dismissed_schedules_nothing() {
        mounted(|| {
            let expiry = Expiry::new(None, || unreachable!("nothing was scheduled"));
            expiry.start();
            assert!(!expiry.is_waiting());
            expiry.stop();
        });
    }

    #[test]
    fn outside_a_window_there_is_nothing_to_schedule_on() {
        mounted(|| {
            let expiry = Expiry::new(Some(Duration::from_secs(1)), || {
                unreachable!("there is no clock")
            });
            expiry.start();
            assert!(!expiry.is_waiting());
        });
    }
}
