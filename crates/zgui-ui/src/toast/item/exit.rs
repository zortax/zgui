//! Taking a toast off the stack once it has finished leaving.
//!
//! # Why the toast does this itself
//!
//! [`Presence`](zgui_ui_primitives::Presence) keeps content mounted through an exit animation, and
//! everything in this library that opens and closes uses it. A toast cannot: what is mounted is
//! decided by the queue rather than by a boolean the component was handed, and the row has to leave
//! the queue when the exit ends, so that the stack stops reserving a place for it and the name is
//! free again. So the same three ways of noticing that an exit has finished are arranged here,
//! against the queue instead of against a mount.

use core::cell::RefCell;
use core::time::Duration;
use std::rc::Rc;

use zgui::reactive::RenderEffect;
use zgui::view::{NodeRef, TimeoutHandle, Timers};

use crate::toast::queue::{ToastId, ToastQueue};

/// How long an exit is allowed to take before it is over whether it has finished or not.
///
/// Waiting on an animation means waiting on an event, and an event can fail to arrive: a rule that
/// stopped matching mid-flight, an end delivered to an element that is no longer the one being
/// watched. A row that waited for ever would keep a place on the stack for a toast nobody can see,
/// and would hold the space the toasts above it should have closed. The animation is not the
/// authority on whether a toast goes — only on how long it is allowed to look good.
const DEADLINE: Duration = Duration::from_secs(1);

/// How one toast leaves the queue when its exit has finished.
#[derive(Clone)]
pub(crate) struct Departure {
    /// The queue the row belongs to, when there is one.
    queue: Option<ToastQueue>,
    /// Which row.
    id: ToastId,
    /// The element the exit animation runs on.
    surface: NodeRef,
    /// The one-frame check that catches an exit with no animation at all.
    pending: Rc<RefCell<Option<TimeoutHandle>>>,
    /// The deadline that ends an exit whose end never arrives.
    overdue: Rc<RefCell<Option<TimeoutHandle>>>,
}

impl Departure {
    /// How the toast called `id` leaves `queue`, watching the animations on `surface`.
    pub(crate) fn new(queue: Option<ToastQueue>, id: ToastId, surface: NodeRef) -> Self {
        Self {
            queue,
            id,
            surface,
            pending: Rc::new(RefCell::new(None)),
            overdue: Rc::new(RefCell::new(None)),
        }
    }

    /// Takes the row out, if it is on its way out and nothing on it is still animating.
    ///
    /// Asked rather than counted: a second animation may have started while the first was running,
    /// and an exit that ended on the first end it saw would cut the rest of it off.
    ///
    /// Refused outright before the deferred check below has run. Between the dismissal and the
    /// cascade that starts the exit there is a gap in a live window — the row is marked leaving
    /// the instant the click's handler runs, while the exit begins frames later — and an
    /// `animationend` from anything else, delivered in the same batch as the click itself, lands
    /// in that gap asking "is anything still running?" about an exit that has not begun. The
    /// deadline being armed is what says the leave has been *processed*; the grace check still
    /// being armed is what says the exit has not yet been given its time.
    pub(crate) fn settle(&self) {
        if self.pending.borrow().is_some() || self.overdue.borrow().is_none() {
            return;
        }
        let Some(queue) = self.queue else { return };
        if queue.is_leaving_untracked(self.id) && self.surface.running_animations() == 0 {
            queue.remove(self.id);
        }
    }

    /// Watches whether the toast is leaving, and arranges for it to go when it has.
    ///
    /// The returned effect has to be held for as long as the toast is, because dropping it stops the
    /// watching.
    ///
    /// Only the moment the answer *changes* installs anything. Whether a toast is leaving is read
    /// from the queue, so every other toast's measurement runs this again — and an exit that re-armed
    /// its deadline each time would be an exit with no deadline at all on a busy stack.
    pub(crate) fn watch(&self, leaving: impl Fn() -> bool + 'static) -> RenderEffect<bool> {
        let departure = self.clone();
        RenderEffect::new(move |was: Option<bool>| {
            let going = leaving();
            if was == Some(going) {
                return going;
            }
            if !going {
                // Whatever a previous exit installed is cancelled rather than left to expire
                // harmlessly, so that a row which somehow stopped leaving is not taken away by a
                // deadline armed before that.
                departure.pending.borrow_mut().take();
                departure.overdue.borrow_mut().take();
                return going;
            }
            let Some(clock) = Timers::current() else {
                // No window, so no animation can run and no timer can be scheduled either. The one
                // honest thing left is to go at once — outright, because the ask below refuses
                // until timers this scope will never have are armed.
                if let Some(queue) = departure.queue
                    && queue.is_leaving_untracked(departure.id)
                {
                    queue.remove(departure.id);
                }
                return going;
            };
            // Deferred, because the attribute that starts the exit has not been cascaded yet, so
            // nothing is running: asking now would answer "no animation" for every toast and cut
            // every exit short. Deferred by a few frames' worth rather than by exactly one: a
            // live window schedules its frames against a compositor, and the cascade that starts
            // the exit can land a frame or two after the timer — a check that raced it removed
            // the row at once and the exit was never seen. What this check exists for — an exit
            // with no animation at all — is served just as well fifty milliseconds later.
            let deferred = departure.clone();
            *departure.pending.borrow_mut() = Some(clock.set_timeout(
                Duration::from_millis(50),
                move || {
                    // Cleared before the ask, because a pending check is what the ask refuses on.
                    deferred.pending.borrow_mut().take();
                    deferred.settle();
                },
            ));
            // And the deadline, which is what makes a dismissal that has been asked for happen
            // whatever the animations do.
            let overdue = departure.clone();
            *departure.overdue.borrow_mut() = Some(clock.set_timeout(DEADLINE, move || {
                if let Some(queue) = overdue.queue
                    && queue.is_leaving_untracked(overdue.id)
                {
                    queue.remove(overdue.id);
                }
            }));
            going
        })
    }
}
