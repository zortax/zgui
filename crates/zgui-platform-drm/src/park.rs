//! How the loop waits, and the ways of getting that wrong.
//!
//! The same state machine as `zgui-platform-winit`'s own `park` module, over the same
//! [`IdlePolicy`] and with the same invariant. It is written again here because the shipped one
//! lives in the windowing backend: a console backend that named it would pull in a windowing
//! library, X11 and Wayland to reach three lines of arithmetic.
//!
//! # The two failures
//!
//! **The stall.** A deadline arriving is reported as the *cause* of a turn of the loop, and it
//! draws nothing by itself. Nothing is drawn until something asks, so the arrival has to be turned
//! back into a request by hand. Miss that and a timer fires no frame and an animation never
//! advances, and the symptom is an application that ignores its own clock.
//!
//! **The spin.** A deadline that has already passed, installed anyway, waits for no time at all.
//! The loop wakes at once, finds the moment reached, reports it again, and draws nothing. From
//! outside it looks like the stall.
//!
//! # One invariant covering both
//!
//! **A moment the application named is either waited for or handed over — never dropped.**
//!
//! [`Park::install`] installs no moment that is not strictly in the future, which is the defence
//! against the spin. It discards none either, which is the defence against a third failure the
//! clamp on its own creates: the application decides what it wants against one reading of the
//! clock, and the loop installs it against a second reading microseconds later. A moment picked
//! four microseconds ahead is in the future for the first reading and in the past for the second.
//! A clamp that answered "expired, so park on nothing" would block the loop for ever while holding
//! a frame somebody is waiting for.
//!
//! So [`Park::install`] answers an [`Install`]. A moment that has passed comes back as
//! [`Install::Overdue`], and the one route from there to a [`Parked`] is [`Install::park`], which
//! takes the delivery as an argument. Forgetting to look at a flag, an early return and a `match`
//! arm added later all fail to compile.
//!
//! # What differs from the windowing backend
//!
//! That loop is told the cause of each turn by the platform, and the platform re-derives an
//! arrival from the installed instant every time with no memory of having reported it. This loop
//! owns its own wait: [`timeout`] is what it hands `poll`, and a wait that runs to its end is the
//! arrival. So [`Park::resumed`] counts only if a deadline was installed. The loop can tell a
//! reached deadline from a park of no length, and a count for the second would report arrivals the
//! loop never had.

use std::time::{Duration, Instant};

use rustix::event::{Nsecs, Secs, Timespec};
use zgui_platform::IdlePolicy;

/// A wait of no time at all, which is a poll rather than a sleep.
const NONE: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 0,
};

/// How the loop is parked right now.
///
/// Three answers, and every wait is one of them. They are stated apart from a `poll` timeout so
/// that the decision can be made, and asserted on, with no device open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Parked {
    /// Blocked until the device or the wake channel has something to report.
    Indefinitely,
    /// The same, or until this moment arrives.
    ///
    /// The instant is strictly in the future when it is installed, which is the defence against the
    /// spin. See [`Park::install`].
    Until(Instant),
    /// Not blocked at all: the descriptors are read and the turn is handed back.
    Never,
}

/// What installing a park produced: something to wait on, or a deadline that is already owed.
///
/// The two are apart because they are not interchangeable. A park is a thing to wait on and asks
/// nothing further of the caller. An overdue deadline is work the application asked for and has not
/// been given, and a loop that waits on anything before handing it over is asleep on its own debt.
///
/// [`Install::park`] is the one way to obtain a [`Parked`] from one of these, and it demands the
/// delivery as an argument. So an owed deadline can only be discharged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use = "an install that is dropped may be dropping a deadline the application is waiting for"]
pub(crate) enum Install {
    /// Nothing is owed. This is what to wait on.
    Ready(Parked),
    /// The moment the application named had already passed when the loop came to park on it.
    Overdue(Instant),
}

impl Install {
    /// Returns the moment that is owed, if one is.
    ///
    /// For tracing and for the loop's own decision to hand the turn back. Reading it discharges
    /// nothing: the value still carries the obligation.
    pub(crate) const fn overdue(self) -> Option<Instant> {
        match self {
            Self::Overdue(deadline) => Some(deadline),
            Self::Ready(_) => None,
        }
    }

    /// Returns the park to wait on, delivering an owed deadline edge on the way there.
    ///
    /// `deliver` runs exactly when the moment the application named had already passed by the time
    /// the loop came to park on it, and it is given that moment. When nothing is owed, `deliver`
    /// stays unused and the park passes through unchanged.
    pub(crate) fn park(self, deliver: impl FnOnce(Instant)) -> Parked {
        match self {
            Self::Ready(parked) => parked,
            Self::Overdue(deadline) => {
                deliver(deadline);
                // Indefinite: the delivery has asked for the frame that services this moment, and
                // the loop looks for such a frame before it parks. Polling instead would keep a
                // loop whose frame cannot retire the deadline running at the speed of the
                // processor for no drawn pixel.
                Parked::Indefinitely
            }
        }
    }
}

/// The loop's waiting, as a state machine with nothing else in it.
///
/// One of these belongs to a running loop. It is asked what to park on once per turn, and it is
/// told when a wait it installed ran to its end. It counts the arrivals: the count against the
/// frames separates a loop that parks correctly from a loop reporting the same expired moment for
/// ever while drawing nothing.
#[derive(Debug, Default)]
pub(crate) struct Park {
    /// The deadline the loop is parked on, when it is parked on one.
    installed: Option<Instant>,
    /// How many installed deadlines have been reported reached.
    resumes: u64,
}

impl Park {
    /// Returns a loop that is parked on nothing yet.
    pub(crate) const fn new() -> Self {
        Self {
            installed: None,
            resumes: 0,
        }
    }

    /// Returns the moment the loop is parked until, if it is parked on a deadline.
    ///
    /// The loop acts on [`Park::resumed`], which asks and clears in one step. This reads the same
    /// state without taking it, so an assertion can say that nothing expired is ever waited on.
    #[cfg(test)]
    pub(crate) const fn deadline(&self) -> Option<Instant> {
        self.installed
    }

    /// Returns how many deadline arrivals have been reported to the application.
    #[cfg(test)]
    pub(crate) const fn resumes(&self) -> u64 {
        self.resumes
    }

    /// Decides what to park on, given what the application asked for and the present moment.
    ///
    /// A moment has two answers and no third. A moment strictly in the future is installed and
    /// waited for. A moment that has arrived is handed back as [`Install::Overdue`], whose one
    /// route to a [`Parked`] is a delivery. No arm produces a park while holding a moment, so there
    /// is nowhere for one to be lost.
    ///
    /// A moment that has passed is paid at once. Waiting on it is a wait of no length: the loop
    /// wakes at once, reports the arrival, and does it again. Dropping it loses a frame, because
    /// `now` is read after the application has already decided and a moment it picked microseconds
    /// ahead can pass in between.
    pub(crate) fn install(&mut self, policy: IdlePolicy, now: Instant) -> Install {
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
                // Counted here at the install. It is the same arrival `Park::resumed` would have
                // counted had the loop got as far as waiting for it.
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
    /// The loop asks this on every wait that ran to its end, including a wait of no length, so a
    /// park that carried no deadline answers `false` and counts nothing.
    pub(crate) fn resumed(&mut self) -> bool {
        // Cleared here, and *before* the answer is acted on. A handler that installs a fresh
        // deadline from inside its own callback would otherwise be undone by a clearing after it.
        let arrived = self.installed.take().is_some();
        if arrived {
            self.resumes += 1;
        }
        arrived
    }

    /// Forgets the deadline without counting an arrival.
    ///
    /// For the turn that is cut short for some other reason — the device reporting a finished flip,
    /// a wake from another thread, a signal — after which the park is computed again from scratch.
    /// A wait that was cancelled owes nothing: the application is asked again, and names the moment
    /// again if it still wants it.
    pub(crate) const fn cancel(&mut self) {
        self.installed = None;
    }

    /// Hands the turn back without waiting, and forgets whatever the moment was.
    ///
    /// For the turn that already owes a frame: the loop reads its descriptors and goes round again
    /// instead of sleeping. The moment goes with it. A wait of no length is no wait for that
    /// moment, and a moment left installed over one would be reported reached the instant the poll
    /// came back — an arrival the loop never had.
    ///
    /// Forgetting it loses nothing. The application is asked again on the next turn, against a
    /// clock that has moved, and names the moment again if it still wants it.
    pub(crate) const fn handed_back(&mut self) -> Parked {
        self.installed = None;
        Parked::Never
    }
}

/// Returns `true` if a wait on `parked` would outlast `bound`.
///
/// The loop has moments of its own to answer at, apart from the ones the application names: a
/// session that asked for a terminal is due an answer, and a held key is due its next repeat.
/// Nothing on a console wakes a parked loop for either. So the wait is cut to the earlier of the
/// two, and this comparison says which one that is.
///
/// A park with no end is cut short by any moment at all. A park on the application's own moment is
/// cut short by an earlier one. A wait of no length is cut short by nothing, because it is already
/// over, and the turn after it reads the bound again.
///
/// The caller needs the answer as well as the wait. A wait that ended on the loop's own bound is no
/// moment of the application's arriving, and reporting one there is an arrival the loop never had.
pub(crate) fn outlasts(parked: Parked, bound: Instant) -> bool {
    match parked {
        Parked::Indefinitely => true,
        Parked::Until(deadline) => bound < deadline,
        Parked::Never => false,
    }
}

/// Returns how long `poll` waits for `parked`, or nothing if it waits until something happens.
///
/// The one arithmetic in the loop, and the one place a wait can be got wrong with nothing saying
/// so. A deadline that has passed answers a wait of no length:
/// [`Instant::saturating_duration_since`] holds that for every input, including the moment that
/// passes between [`Park::install`] and this call.
pub(crate) fn timeout(parked: Parked, now: Instant) -> Option<Timespec> {
    match parked {
        Parked::Indefinitely => None,
        Parked::Never => Some(NONE),
        Parked::Until(deadline) => Some(span(deadline.saturating_duration_since(now))),
    }
}

/// Returns `left` as the kernel's own pair of fields.
///
/// The nanoseconds go through a 32-bit value because that is the narrowest this field is on any
/// platform. They are under a second by construction, so the same arithmetic is exact everywhere
/// the crate builds.
fn span(left: Duration) -> Timespec {
    // Zero, if the narrowing were ever taken. `ppoll` refuses a `Timespec` whose nanoseconds reach
    // a whole second.
    let nanos = i32::try_from(left.subsec_nanos()).unwrap_or(0);
    Timespec {
        // A wait of more seconds than the field holds is one no program reaches the end of, so it
        // is cut to the longest wait there is rather than wrapped into the past.
        tv_sec: Secs::try_from(left.as_secs()).unwrap_or(Secs::MAX),
        tv_nsec: Nsecs::from(nanos),
    }
}

#[cfg(test)]
mod tests {
    //! The whole of the loop's waiting, with no device and no clock that moves by itself.
    //!
    //! Every assertion here is about arithmetic over an [`IdlePolicy`] and a reading of the clock,
    //! and that arithmetic is what the loop hands `poll`. A wait that is negative, a wait that is
    //! zero for ever and a moment that is dropped are all invisible on hardware until an
    //! application stops drawing, and all three are decided here.

    use super::{Install, Park, Parked, outlasts, timeout};
    use std::time::{Duration, Instant};
    use zgui_platform::IdlePolicy;

    /// Returns the wait `policy` produces, taking the deadline edge into `delivered` where one is
    /// owed.
    fn waits(park: &mut Park, policy: IdlePolicy, now: Instant, delivered: &mut u32) -> Parked {
        park.install(policy, now).park(|_| *delivered += 1)
    }

    #[test]
    fn blocking_waits_until_something_happens_and_no_longer() {
        let mut park = Park::new();
        let now = Instant::now();
        let mut delivered = 0;

        let parked = waits(&mut park, IdlePolicy::Block, now, &mut delivered);

        assert_eq!(parked, Parked::Indefinitely);
        assert_eq!(
            timeout(parked, now),
            None,
            "no timeout at all is `poll` for ever"
        );
        assert_eq!(delivered, 0, "nothing was owed");
    }

    #[test]
    fn a_deadline_in_the_future_waits_for_the_time_that_is_left() {
        let mut park = Park::new();
        let now = Instant::now();
        let soon = now + Duration::from_millis(16);
        let mut delivered = 0;

        let parked = waits(&mut park, IdlePolicy::BlockUntil(soon), now, &mut delivered);

        assert_eq!(parked, Parked::Until(soon));
        assert_eq!(park.deadline(), Some(soon), "the loop is waiting on it");
        let left = timeout(parked, now).expect("a deadline is a wait of a length");
        assert_eq!(left.tv_sec, 0);
        assert_eq!(
            left.tv_nsec, 16_000_000,
            "sixteen milliseconds of it are left"
        );
        assert_eq!(delivered, 0);
    }

    #[test]
    fn a_wait_of_more_than_a_second_carries_both_fields() {
        let now = Instant::now();
        let later = now + Duration::from_millis(2_500);

        let left = timeout(Parked::Until(later), now).expect("a deadline is a wait of a length");

        assert_eq!(left.tv_sec, 2);
        assert_eq!(left.tv_nsec, 500_000_000);
    }

    #[test]
    fn a_deadline_that_has_passed_waits_for_no_time_rather_than_for_a_negative_one() {
        // The moment can pass between the install and this call, which is the one way a park can
        // hold a deadline that is already behind the clock. The kernel refuses a negative
        // `Timespec`, and the loop reports a refused wait as the device no longer being watchable —
        // so a negative pair would end the run. A pair of zeroes reads the descriptors and hands
        // the turn back, which is the answer a moment that has arrived deserves.
        let now = Instant::now();
        for elapsed in [
            Duration::ZERO,
            Duration::from_nanos(1),
            Duration::from_micros(4),
            Duration::from_secs(9),
        ] {
            let left = timeout(Parked::Until(now - elapsed), now)
                .expect("a deadline is a wait of a length");
            assert_eq!(
                (left.tv_sec, left.tv_nsec),
                (0, 0),
                "a moment {elapsed:?} in the past is a wait of no time at all"
            );
        }
    }

    #[test]
    fn a_deadline_that_has_passed_is_owed_rather_than_installed() {
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

        assert_eq!(
            delivered,
            [passed],
            "the moment that had passed was handed over"
        );
        assert_eq!(
            parked,
            Parked::Indefinitely,
            "and only then did the loop park"
        );
        assert_eq!(park.resumes(), 1, "the delivery is a reported arrival");
    }

    #[test]
    fn a_moment_that_has_passed_is_answered_once_a_turn_rather_than_waited_on_for_ever() {
        // The spin written out: a passed moment installed anyway is a wait of no length whose
        // arrival is re-derived every turn while nothing draws. Nothing here can install one, so
        // every turn answers its own moment and then blocks.
        let mut park = Park::new();
        let now = Instant::now();
        let passed = now - Duration::from_micros(1);
        let mut delivered = 0;

        for _ in 0..64 {
            let parked = waits(
                &mut park,
                IdlePolicy::BlockUntil(passed),
                now,
                &mut delivered,
            );
            assert_eq!(
                parked,
                Parked::Indefinitely,
                "an answered moment is never waited on"
            );
            assert_eq!(park.deadline(), None);
            assert_eq!(
                timeout(parked, now),
                None,
                "so no turn polls at zero for it"
            );
        }

        assert_eq!(
            delivered, 64,
            "every turn answered its own moment, and none was dropped"
        );
        assert_eq!(
            park.resumes(),
            64,
            "one arrival per turn, and one frame owed for each"
        );
    }

    #[test]
    fn a_moment_the_loop_owes_cuts_a_wait_that_would_outlast_it() {
        // The loop owes itself two moments and nothing on a console wakes it for either: a session
        // waiting to hear that a terminal it asked for has moved, and the next repeat of a held
        // key. A park left to outlast the nearer of the two would keep the pointer off the screen,
        // or hold a repeat back, until a person pressed something.
        let now = Instant::now();
        let soon = now + Duration::from_millis(16);
        let later = now + Duration::from_millis(500);

        assert!(
            outlasts(Parked::Indefinitely, later),
            "a park with no end outlasts every moment there is"
        );
        assert!(
            outlasts(Parked::Until(later), soon),
            "and so does one that ends after the bound"
        );
        assert!(
            !outlasts(Parked::Until(soon), later),
            "a park that ends first is waited on as it is"
        );
        assert!(
            !outlasts(Parked::Until(soon), soon),
            "and so is one that ends at the same moment, which the application's own arrival then \
             answers"
        );
        assert!(
            !outlasts(Parked::Never, now),
            "a wait of no length is over already, and the turn after it reads the bound again"
        );
    }

    #[test]
    fn spinning_reads_the_descriptors_and_hands_the_turn_back() {
        let mut park = Park::new();
        let now = Instant::now();
        let mut delivered = 0;

        let parked = waits(&mut park, IdlePolicy::Spin, now, &mut delivered);

        assert_eq!(parked, Parked::Never);
        assert_eq!(park.deadline(), None, "a spin waits on no moment");
        let left = timeout(parked, now).expect("a spin is a wait of no length");
        assert_eq!((left.tv_sec, left.tv_nsec), (0, 0));
    }

    #[test]
    fn an_arrival_clears_the_deadline_so_it_cannot_be_reported_twice() {
        let mut park = Park::new();
        let now = Instant::now();
        let soon = now + Duration::from_millis(1);
        let mut delivered = 0;

        waits(&mut park, IdlePolicy::BlockUntil(soon), now, &mut delivered);

        assert!(park.resumed(), "the installed deadline arrived");
        assert_eq!(park.deadline(), None);
        assert!(!park.resumed(), "nothing was installed the second time");
        assert_eq!(
            park.resumes(),
            1,
            "and a wait that carried no deadline counts nothing"
        );
    }

    #[test]
    fn a_turn_that_is_handed_back_reports_no_arrival_when_it_comes_round() {
        // A turn that already owes a frame reads its descriptors and goes round again. The wait
        // has no length, so it always runs to its end — and a moment left installed over it would
        // be reported reached the moment the poll came back, every turn, while the moment itself
        // was still ahead.
        let mut park = Park::new();
        let now = Instant::now();
        let soon = now + Duration::from_secs(1);
        let mut delivered = 0;

        waits(&mut park, IdlePolicy::BlockUntil(soon), now, &mut delivered);
        let parked = park.handed_back();

        assert_eq!(parked, Parked::Never);
        let left = timeout(parked, now).expect("a handed-back turn is a wait of no length");
        assert_eq!((left.tv_sec, left.tv_nsec), (0, 0));
        assert!(
            !park.resumed(),
            "the wait ran to its end, and it was waiting for nothing"
        );
        assert_eq!(park.resumes(), 0, "so no arrival is counted either");
    }

    #[test]
    fn a_wait_that_was_cut_short_forgets_the_deadline_without_counting_an_arrival() {
        let mut park = Park::new();
        let now = Instant::now();
        let soon = now + Duration::from_secs(1);
        let mut delivered = 0;

        waits(&mut park, IdlePolicy::BlockUntil(soon), now, &mut delivered);
        park.cancel();

        assert_eq!(park.deadline(), None);
        assert_eq!(park.resumes(), 0);
    }

    #[test]
    fn a_loop_that_keeps_its_deadlines_reports_one_arrival_per_deadline() {
        // The ratio the two failures move in opposite directions: a loop that is waiting reports
        // one arrival per moment it was given, and a loop that is spinning reports many.
        let mut park = Park::new();
        let mut now = Instant::now();
        let step = Duration::from_millis(16);
        let mut delivered = 0;

        for _ in 0..32 {
            let parked = waits(
                &mut park,
                IdlePolicy::BlockUntil(now + step),
                now,
                &mut delivered,
            );
            let left = timeout(parked, now).expect("a deadline is a wait of a length");
            assert_eq!(left.tv_nsec, 16_000_000, "the whole interval is waited for");
            // The wait runs to its end, which is the moment arriving.
            now += step;
            assert!(park.resumed());
        }

        assert_eq!(park.resumes(), 32, "one arrival per deadline and no more");
        assert_eq!(
            delivered, 0,
            "and none of them was owed rather than waited for"
        );
    }
}
