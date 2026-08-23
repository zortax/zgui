//! The outcome of installing a park, and the obligation it can carry.

use std::time::Instant;

use crate::app::park::policy::Parked;

/// What installing a park produced: something to wait on, or a deadline that is already owed.
///
/// The two are kept apart because they are not interchangeable. A park is a thing to wait on and
/// nothing further is required of the caller. An overdue deadline is work the application asked
/// for and has not yet been given, and waiting on anything at all before it has been handed over
/// is a loop asleep on its own debt — which, from outside, is a window that has stopped.
///
/// The type is the guarantee. [`Install::park`] is the only way to obtain a [`Parked`] from one of
/// these, and it demands the delivery as an argument, so an owed deadline cannot be dropped by
/// forgetting to look at a flag, by an early return, or by a `match` arm added later. It can only
/// be discharged.
///
/// ```
/// use std::time::{Duration, Instant};
/// use zgui_platform::{IdlePolicy, Park, Parked};
///
/// let mut park = Park::new();
/// let now = Instant::now();
/// let passed = now - Duration::from_micros(1);
///
/// let mut delivered = None;
/// let parked = park
///     .install(IdlePolicy::BlockUntil(passed), now)
///     .park(|deadline| delivered = Some(deadline));
///
/// assert_eq!(delivered, Some(passed), "the moment that had passed was handed over");
/// assert_eq!(parked, Parked::Indefinitely, "and only then did the loop park");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use = "an install that is dropped may be dropping a deadline the application is waiting for"]
#[non_exhaustive]
pub enum Install {
    /// Nothing is owed. This is what to wait on.
    Ready(Parked),
    /// The moment the application named had already passed when the loop came to park on it.
    ///
    /// Its edge has not been reported yet, and the loop must not block until it has been.
    Overdue(Instant),
}

impl Install {
    /// The moment that is owed, when one is.
    ///
    /// For tracing and for assertions. Reading it discharges nothing: the obligation is still
    /// carried by the value and still has to be handed to [`Install::park`].
    pub const fn overdue(self) -> Option<Instant> {
        match self {
            Self::Overdue(deadline) => Some(deadline),
            Self::Ready(_) => None,
        }
    }

    /// The park to wait on, delivering an owed deadline edge on the way there.
    ///
    /// `deliver` runs exactly when the moment the application named had already passed by the time
    /// the loop came to park on it, and is given that moment. It is the one chance to turn a
    /// deadline into the frame it was asking for; whatever it asks for is what brings the loop back
    /// out of the indefinite park this then returns.
    ///
    /// When nothing is owed, `deliver` does not run and the park passes through unchanged.
    pub fn park(self, deliver: impl FnOnce(Instant)) -> Parked {
        match self {
            Self::Ready(parked) => parked,
            Self::Overdue(deadline) => {
                deliver(deadline);
                // Indefinite and not a poll: the delivery has asked for the frame that services
                // this moment, and a frame that has been asked for wakes a blocked loop by itself.
                // Polling instead would keep a loop whose frame cannot retire the deadline — a
                // hidden window — running at the speed of the processor for no drawn pixel.
                Parked::Indefinitely
            }
        }
    }
}
