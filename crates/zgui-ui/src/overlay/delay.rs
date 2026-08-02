//! Doing something in a moment, unless something else happens first.

use core::time::Duration;
use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::Owner;
use zgui::view::TimeoutHandle;

/// One pending action that a later call replaces, and any call may take back.
///
/// A tooltip that appeared the instant the pointer crossed it would flicker on its way past, and
/// one that vanished the instant the pointer left would be impossible to move onto. Both are the
/// same shape: schedule something, and cancel it if the reason for it goes away.
///
/// At most one is ever pending. Scheduling again replaces what was pending rather than adding to
/// it, because two timers racing to open the same surface is one of them opening it after the
/// other closed it.
///
/// A delay of zero runs **now**, rather than at the start of the next frame. That distinction
/// matters at the only place it shows: a tooltip asked for no delay should be up in the frame the
/// pointer arrived, not the one after.
///
/// ```
/// use core::time::Duration;
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::overlay::Delayed;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let runs = Rc::new(Cell::new(0));
///     let delayed = Delayed::new();
///
///     // No delay is not a delay.
///     let count = Rc::clone(&runs);
///     delayed.after(Duration::ZERO, move || count.set(count.get() + 1));
///     assert_eq!(runs.get(), 1);
///     assert!(!delayed.is_scheduled(), "nothing was scheduled, because nothing had to be");
/// });
/// scope.unmount();
/// ```
#[derive(Clone)]
pub struct Delayed {
    /// The timer, held because dropping a handle cancels it.
    pending: Rc<RefCell<Option<TimeoutHandle>>>,
    /// The scope this was created in, which is where the clock is.
    ///
    /// Captured rather than looked up at the moment of scheduling, because the moment of
    /// scheduling is inside an event handler: a handler runs with no scope current, and a timer
    /// asked for there would have no window to schedule against.
    owner: Option<Owner>,
}

impl Default for Delayed {
    /// The same as [`Delayed::new`], and written out rather than derived.
    ///
    /// A derived one would leave the scope empty, and a `Delayed` with no scope schedules nothing
    /// and says nothing about it — so every delay reached through a `Default` would silently never
    /// run.
    fn default() -> Self {
        Self::new()
    }
}

impl Delayed {
    /// Nothing pending, scheduled against the calling scope's clock.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pending: Rc::new(RefCell::new(None)),
            owner: Owner::current(),
        }
    }

    /// Runs `action` after `delay`, cancelling whatever was pending.
    pub fn after(&self, delay: Duration, action: impl FnOnce() + 'static) {
        self.cancel();
        if delay.is_zero() {
            action();
            return;
        }
        let Some(owner) = &self.owner else {
            // Loud rather than silent, and for the reason every silent one is a bug: an action
            // that is never scheduled looks exactly like an action whose moment has not come yet,
            // and a typeahead whose reset never runs is a menu that stops answering letters.
            debug_assert!(
                false,
                "a delay was created outside a window's scope, so there is no clock to schedule \
                 it against"
            );
            return;
        };
        *self.pending.borrow_mut() = Some(owner.with(|| set_timeout(delay, action)));
    }

    /// Takes back whatever was pending. Doing so when nothing is does nothing.
    pub fn cancel(&self) {
        self.pending.borrow_mut().take();
    }

    /// Whether a timer has been scheduled here and not taken back.
    ///
    /// It says nothing about whether that timer has since fired: a fired one-shot is already over,
    /// and asking the engine to take it back is a no-op, so nothing is gained by chasing it.
    #[must_use]
    pub fn is_scheduled(&self) -> bool {
        self.pending.borrow().is_some()
    }
}
