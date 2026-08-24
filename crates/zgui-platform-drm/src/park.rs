//! Turning a park into the wait this loop hands `ppoll`.
//!
//! The state machine is [`zgui_platform::Park`]: every backend parks by the same arithmetic over
//! the same [`IdlePolicy`], so it is stated once, in the contract. What lives here is the console's
//! own half — the conversion of a [`Parked`] into the kernel's own pair of fields, and the cut
//! against moments this loop owes itself.
//!
//! # Why a conversion is needed at all
//!
//! The windowing backend is told the cause of each turn by the platform. This loop owns its wait:
//! [`timeout`] is what it hands `poll`, and a wait that runs to its end is the moment arriving. A
//! wait of no length is a poll rather than a sleep, which is how a turn that already owes a frame
//! reads its descriptors and goes round again.
//!
//! [`Park::resumes`] counts every report, including one made against nothing installed. This loop
//! acts on [`Park::resumed`]'s answer rather than on the count, so the count is a diagnostic here
//! and not a decision.
//!
//! [`IdlePolicy`]: zgui_platform::IdlePolicy

use std::time::{Duration, Instant};

use rustix::event::{Nsecs, Secs, Timespec};

pub(crate) use zgui_platform::{Park, Parked};

/// A wait of no time at all, which is a poll rather than a sleep.
const NONE: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 0,
};

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
        // [`Parked`] is `#[non_exhaustive]`, so a wait this backend has never heard of is a wait it
        // cannot measure. Answering that it outlasts the bound cuts it to the bound, which is a
        // turn taken early rather than a moment of the loop's own missed.
        _ => true,
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
        // A wait this backend has never heard of is answered as no wait at all, so the turn reads
        // its descriptors and comes round again. The alternative is `None`, and a loop parked for
        // ever on a wait it could not read is a frozen application rather than a busy one.
        _ => Some(NONE),
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

    use super::{Park, Parked, outlasts, timeout};
    use std::time::{Duration, Instant};
    use zgui_platform::{IdlePolicy, Install};

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
        park.cancel();
        let parked = Parked::Never;

        assert_eq!(parked, Parked::Never);
        let left = timeout(parked, now).expect("a handed-back turn is a wait of no length");
        assert_eq!((left.tv_sec, left.tv_nsec), (0, 0));
        assert!(
            !park.resumed(),
            "the wait ran to its end, and it was waiting for nothing"
        );
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
