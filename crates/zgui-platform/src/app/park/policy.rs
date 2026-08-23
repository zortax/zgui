//! The park state machine itself.

use std::time::Instant;

use crate::app::idle::IdlePolicy;

use crate::app::park::install::Install;

/// How the loop is parked right now.
///
/// The three answers are the whole vocabulary of waiting. They are stated here rather than in the
/// windowing library's own terms so that the decision can be made, and asserted on, without a
/// running event loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Parked {
    /// Blocked until the platform has something to report.
    Indefinitely,
    /// Blocked until the platform has something to report, or until this moment arrives.
    ///
    /// The instant is always strictly in the future at the moment it is installed. That is not a
    /// convention but the whole defence against the spin: see [`Park::install`].
    Until(Instant),
    /// Not blocked at all.
    Never,
}

impl Parked {
    /// The moment this park ends by itself, when it ends by itself at all.
    pub const fn deadline(self) -> Option<Instant> {
        match self {
            Self::Until(deadline) => Some(deadline),
            _ => None,
        }
    }
}

/// The loop's waiting, as a state machine with nothing else in it.
///
/// One of these belongs to a running loop. It is asked what to park on once per turn, and it is
/// told when a deadline it installed has arrived. It counts both, because the count is the only
/// thing that separates a loop parking correctly from a loop reporting the same expired deadline
/// for ever while running no frames.
///
/// # The one invariant
///
/// **A moment the application named is either waited for or handed over. It is never dropped.**
///
/// The two ways of breaking it are opposite and both look, from outside, like a window that has
/// stopped. Installing a moment that has already passed is the *spin*: the time remaining is
/// recomputed on every turn, is zero every time, and the arrival is re-derived every time, so the
/// loop reports an expiry per iteration and draws nothing. Dropping it is the *stall*: the loop
/// blocks with nothing to wake it while something is owed.
///
/// [`Park::install`] does neither, and cannot. A moment still in the future is installed. A moment
/// that has passed is returned as [`Install::Overdue`], which cannot be turned into a [`Parked`]
/// without a delivery — so the caller's only options are to hand the moment over or to not park at
/// all. There is no arm in which it is quietly forgotten.
///
/// ```
/// use std::time::{Duration, Instant};
/// use zgui_platform::{IdlePolicy, Install, Park, Parked};
///
/// let mut park = Park::new();
/// let now = Instant::now();
///
/// // A moment in the future is installed as itself, and nothing is owed.
/// let soon = now + Duration::from_millis(700);
/// assert_eq!(
///     park.install(IdlePolicy::BlockUntil(soon), now),
///     Install::Ready(Parked::Until(soon)),
/// );
///
/// // One that has already passed is never installed — and never dropped either.
/// let passed = now - Duration::from_micros(1);
/// assert_eq!(
///     park.install(IdlePolicy::BlockUntil(passed), now),
///     Install::Overdue(passed),
/// );
/// assert_eq!(park.deadline(), None, "nothing expired is ever waited on");
/// ```
#[derive(Debug, Default)]
pub struct Park {
    /// The deadline the loop is parked on, when it is parked on one.
    installed: Option<Instant>,
    /// How many installed deadlines have been reported reached.
    resumes: u64,
}

impl Park {
    /// A loop that is not parked on anything yet.
    pub const fn new() -> Self {
        Self {
            installed: None,
            resumes: 0,
        }
    }

    /// The moment the loop is parked until, if it is parked on a deadline at all.
    pub const fn deadline(&self) -> Option<Instant> {
        self.installed
    }

    /// How many deadline arrivals have been reported to the application.
    ///
    /// Compared against how many frames ran, this is the whole test for the spin: a loop that
    /// reports more expiries than it runs frames is not waiting, it is looping.
    pub const fn resumes(&self) -> u64 {
        self.resumes
    }

    /// Decides what to park on, given what the application asked for and the present moment.
    ///
    /// There are two answers for a moment and no third one, and that is the whole of the design.
    /// A moment strictly in the future is installed and waited for. A moment that has arrived is
    /// handed back as [`Install::Overdue`], whose only route to a [`Parked`] is a delivery. No arm
    /// of this produces a park while holding a moment, so there is nowhere for one to be lost.
    ///
    /// It is not installed, because the time remaining on a deadline that has passed is recomputed
    /// on every turn, yields zero every time, and is reported reached every time, for ever, while
    /// nothing draws. It is not discarded, because `now` is read after the application has already
    /// decided and a moment it picked microseconds ahead can pass in between — a race, not an
    /// application error, and the loop that treated it as one blocked for ever holding a frame
    /// somebody was waiting for.
    ///
    /// The arrival is counted here rather than at the delivery, because it is the same arrival
    /// [`Park::resumed`] would have counted had the loop got as far as waiting for it, and the
    /// count is what the spin is measured by.
    pub fn install(&mut self, policy: IdlePolicy, now: Instant) -> Install {
        match policy {
            IdlePolicy::Spin => {
                self.installed = None;
                Install::Ready(Parked::Never)
            }
            IdlePolicy::BlockUntil(deadline) if deadline > now => {
                self.installed = Some(deadline);
                Install::Ready(Parked::Until(deadline))
            }
            IdlePolicy::BlockUntil(deadline) => {
                self.installed = None;
                self.resumes += 1;
                Install::Overdue(deadline)
            }
            _ => {
                self.installed = None;
                Install::Ready(Parked::Indefinitely)
            }
        }
    }

    /// Takes the expiry edge, reporting whether a deadline this loop installed has arrived.
    ///
    /// The deadline is cleared here rather than in whatever handles the answer, and it is cleared
    /// *before* the answer is acted on, so that a handler installing a fresh deadline from inside
    /// its own callback is not undone by the clearing that would otherwise follow it.
    ///
    /// An arrival reported when nothing was installed is counted and forwarded all the same: a
    /// platform is allowed to wake early, an application asked to service a deadline it has already
    /// serviced does nothing, and swallowing the report would hide a genuine wake.
    pub fn resumed(&mut self) -> bool {
        self.resumes += 1;
        self.installed.take().is_some()
    }

    /// Forgets the deadline without counting an arrival.
    ///
    /// This is for the turn that is cut short for some other reason — an event arriving before the
    /// deadline, the loop being asked to finish — after which the park is recomputed from scratch.
    /// Nothing is owed by a wait that was cancelled: the application is asked again, and names the
    /// moment again if it still wants it.
    pub const fn cancel(&mut self) {
        self.installed = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{Install, Park, Parked};
    use crate::app::idle::IdlePolicy;
    use std::time::{Duration, Instant};

    #[test]
    fn a_deadline_in_the_future_is_installed_as_itself() {
        let mut park = Park::new();
        let now = Instant::now();
        let soon = now + Duration::from_millis(16);
        assert_eq!(
            park.install(IdlePolicy::BlockUntil(soon), now),
            Install::Ready(Parked::Until(soon))
        );
        assert_eq!(park.deadline(), Some(soon));
    }

    #[test]
    fn a_deadline_that_has_already_arrived_is_owed_rather_than_installed() {
        // The property the earlier version of this test had backwards. It asserted that an expired
        // moment produced an indefinite park, which is the stall written down as the expectation:
        // the loop blocks and the frame the application asked for is never run by anybody.
        let mut park = Park::new();
        let now = Instant::now();
        for elapsed in [
            Duration::ZERO,
            Duration::from_nanos(1),
            Duration::from_micros(1),
            Duration::from_secs(9),
        ] {
            let passed = now - elapsed;
            assert_eq!(
                park.install(IdlePolicy::BlockUntil(passed), now),
                Install::Overdue(passed),
                "a deadline {elapsed:?} in the past was dropped instead of owed"
            );
            assert_eq!(park.deadline(), None, "nothing expired is ever waited on");
        }
    }

    #[test]
    fn an_owed_deadline_reaches_a_park_only_through_its_delivery() {
        let mut park = Park::new();
        let now = Instant::now();
        let passed = now - Duration::from_micros(3);
        let mut delivered = Vec::new();
        let parked = park
            .install(IdlePolicy::BlockUntil(passed), now)
            .park(|deadline| delivered.push(deadline));
        assert_eq!(delivered, [passed]);
        assert_eq!(parked, Parked::Indefinitely);
        assert_eq!(park.resumes(), 1, "the delivery is a reported arrival");
    }

    #[test]
    fn an_arrival_clears_the_deadline_so_it_cannot_be_reported_twice() {
        let mut park = Park::new();
        let now = Instant::now();
        let soon = now + Duration::from_millis(1);
        assert_eq!(
            park.install(IdlePolicy::BlockUntil(soon), now),
            Install::Ready(Parked::Until(soon))
        );
        assert!(park.resumed(), "the installed deadline arrived");
        assert_eq!(park.deadline(), None);
        assert!(!park.resumed(), "nothing was installed the second time");
        assert_eq!(park.resumes(), 2, "both reports are counted");
    }

    #[test]
    fn spinning_and_blocking_carry_no_deadline_at_all() {
        let mut park = Park::new();
        let now = Instant::now();
        assert_eq!(
            park.install(IdlePolicy::Spin, now),
            Install::Ready(Parked::Never)
        );
        assert_eq!(park.deadline(), None);
        assert_eq!(
            park.install(IdlePolicy::Block, now),
            Install::Ready(Parked::Indefinitely)
        );
        assert_eq!(park.deadline(), None);
    }

    #[test]
    fn cancelling_forgets_the_deadline_without_counting_an_arrival() {
        let mut park = Park::new();
        let now = Instant::now();
        let soon = now + Duration::from_secs(1);
        assert_eq!(
            park.install(IdlePolicy::BlockUntil(soon), now),
            Install::Ready(Parked::Until(soon))
        );
        park.cancel();
        assert_eq!(park.deadline(), None);
        assert_eq!(park.resumes(), 0);
    }
}
