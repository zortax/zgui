//! How the loop parks, and the deadline-expiry edge that turns a reached deadline into a frame.
//!
//! This is the smallest part of the frame loop and the one most easily got wrong, because both ways
//! of getting it wrong look like nothing happening. A deadline that is never turned back into a
//! redraw request is a **stall**: a timer that fires no frame. A deadline installed already expired
//! is a **spin**: the platform reports it reached on every turn, for ever, and the loop runs no
//! frames while burning a core.
//!
//! Only the ratio of resumes to frames separates the second from a correct park, which is why
//! [`Park::assert_invariant`] exists and why every scripted run is checked against it.

use std::time::Instant;

use zgui_platform::IdlePolicy;
use zgui_profile::{Counter, counter};

/// Which reading of the park the harness is modelling.
///
/// The defective one exists so that the invariant guarding the correct one can be shown to fail:
/// an assertion nobody has ever seen fail is an assertion nobody has checked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParkModel {
    /// A deadline is installed only when it is strictly in the future, and reaching one requests a
    /// redraw.
    #[default]
    Correct,
    /// A deadline is installed whether or not it has passed, and reaching one requests nothing.
    ///
    /// This is the loop as it behaves with the wake edge missing: the expired deadline stays
    /// installed and is reported reached on every turn, so resumes accumulate without bound while
    /// no frame ever runs.
    MissingWakeEdge,
}

/// What the loop is parked on, and what has happened to it.
#[derive(Clone, Debug)]
pub struct Park {
    /// Which reading is being modelled.
    model: ParkModel,
    /// How the loop is currently parked.
    policy: IdlePolicy,
    /// How many times an installed deadline has been reported reached.
    resumes: u64,
    /// How many redraw requests have reached the surface.
    redraws: u64,
    /// How many of those came from the frame loop's own "another frame is owed" flag.
    frames_requested: u64,
}

impl Park {
    /// A loop parked on nothing.
    pub fn new(model: ParkModel) -> Self {
        Self {
            model,
            policy: IdlePolicy::Block,
            resumes: 0,
            redraws: 0,
            frames_requested: 0,
        }
    }

    /// Installs the merged deadline the frame asked for, if it should be installed at all.
    ///
    /// A deadline that is not strictly in the future is **not** installed: the platform recomputes
    /// the remaining time on every turn, finds zero, and reports the deadline reached again — for
    /// ever. What a reached deadline deserves is a redraw request and a park with no deadline at
    /// all, which is what [`Park::expire`] does.
    pub fn install(&mut self, deadline: Option<Instant>, now: Instant) {
        self.policy = match (deadline, self.model) {
            (None, _) => IdlePolicy::Block,
            (Some(deadline), ParkModel::Correct) => IdlePolicy::until(deadline, now),
            (Some(deadline), ParkModel::MissingWakeEdge) => IdlePolicy::BlockUntil(deadline),
        };
    }

    /// Reports the clock reaching `now`, and whether that expired the parked deadline.
    ///
    /// This is the platform's deadline-expiry edge. Under the correct reading it does three things,
    /// and the third is the one whose absence is a stall: it counts the resume, it *requests a
    /// redraw*, and it parks with no deadline so the expired one is never reported twice.
    pub fn expire(&mut self, now: Instant) -> bool {
        let Some(deadline) = self.policy.deadline() else {
            return false;
        };
        if now < deadline {
            return false;
        }
        self.resumes += 1;
        counter::bump(Counter::Wakes);
        if self.model == ParkModel::Correct {
            self.policy = IdlePolicy::Block;
            self.redraws += 1;
        }
        true
    }

    /// Records the one redraw request P12 makes when a frame owed another.
    pub fn request_another_frame(&mut self) {
        self.frames_requested += 1;
        self.redraws += 1;
    }

    /// The moment the loop is parked until, if any.
    pub fn deadline(&self) -> Option<Instant> {
        self.policy.deadline()
    }

    /// How the loop is parked.
    pub fn policy(&self) -> IdlePolicy {
        self.policy
    }

    /// How many times a parked deadline has been reported reached.
    pub fn resumes(&self) -> u64 {
        self.resumes
    }

    /// How many redraw requests have reached the surface.
    pub fn redraws_requested(&self) -> u64 {
        self.redraws
    }

    /// How many of those came from a frame owing another frame.
    pub fn frames_requested(&self) -> u64 {
        self.frames_requested
    }

    /// Sets every count back to zero, leaving the park itself alone.
    pub fn reset_counts(&mut self) {
        self.resumes = 0;
        self.redraws = 0;
        self.frames_requested = 0;
    }

    /// Asserts that the loop is parking rather than spinning.
    ///
    /// `frames` counts the frames of the same window the resumes were counted over — the two are
    /// compared, so one of them reaching further back than the other would make the comparison
    /// weaker by exactly that much.
    ///
    /// One resume per frame, plus the one that has been delivered but whose frame has not run yet,
    /// is the whole budget. Anything above it means deadlines are being reported reached without
    /// producing the frames they exist to produce — which is a loop that looks idle, ignores its own
    /// timers and burns a core.
    ///
    /// # Panics
    ///
    /// Panics when more deadlines have been reported reached than frames have run, plus one.
    pub fn assert_invariant(&self, frames: u64) {
        assert!(
            self.resumes <= frames + 1,
            "the loop reported {} expired deadlines against {frames} frames. A deadline reported \
             reached that produces no frame is not a stall: it is a busy loop running no frames at \
             all, and this ratio is the only thing that separates it from a correct park.",
            self.resumes
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use zgui_platform::IdlePolicy;

    use super::{Park, ParkModel};

    #[test]
    fn a_deadline_in_the_past_is_never_installed() {
        let now = Instant::now();
        let mut park = Park::new(ParkModel::Correct);
        park.install(Some(now - Duration::from_millis(1)), now);
        assert_eq!(park.policy(), IdlePolicy::Block);
        assert!(park.deadline().is_none());
    }

    #[test]
    fn reaching_a_deadline_requests_a_redraw_and_clears_the_park() {
        let now = Instant::now();
        let mut park = Park::new(ParkModel::Correct);
        park.install(Some(now + Duration::from_millis(700)), now);

        assert!(!park.expire(now + Duration::from_millis(699)));
        assert_eq!(park.redraws_requested(), 0);

        assert!(park.expire(now + Duration::from_millis(700)));
        assert_eq!(park.resumes(), 1);
        assert_eq!(park.redraws_requested(), 1);
        assert!(
            park.deadline().is_none(),
            "an expired deadline is never re-installed"
        );

        // And it is never reported twice, however long the clock runs on.
        assert!(!park.expire(now + Duration::from_secs(10)));
        assert_eq!(park.resumes(), 1);
    }

    #[test]
    fn the_invariant_holds_for_a_correct_park() {
        let now = Instant::now();
        let mut park = Park::new(ParkModel::Correct);
        for step in 0..100 {
            park.install(Some(now + Duration::from_millis(step + 1)), now);
            park.expire(now + Duration::from_millis(step + 1));
            park.assert_invariant(step + 1);
        }
    }

    #[test]
    #[should_panic(expected = "expired deadlines against 1 frames")]
    fn the_invariant_fails_on_the_loop_with_the_wake_edge_missing() {
        // The positive control. Without it, "resumes <= frames + 1" is an assertion nobody has ever
        // seen fail, and an assertion nobody has seen fail is an assertion nobody has checked.
        let now = Instant::now();
        let mut park = Park::new(ParkModel::MissingWakeEdge);
        park.install(Some(now), now);
        for step in 0..999 {
            park.expire(now + Duration::from_millis(step));
        }
        assert_eq!(
            park.redraws_requested(),
            0,
            "and no frame was ever asked for"
        );
        park.assert_invariant(1);
    }
}
