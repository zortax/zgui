//! When a running animation's next frame is due.
//!
//! An animation's frames are due at a **phase**, not after a delay. The next one belongs one
//! refresh interval after the moment the last one was due — not one refresh interval after whatever
//! else last happened to the window. The distinction has no visible consequence on a loop that
//! wakes only for the animation, and it is the whole of the behaviour on any loop that does not: a
//! deadline recomputed as *now plus an interval* is pushed a full interval into the future by every
//! unrelated wake, so a window that is also seeing pointer samples, frame callbacks or a compositor
//! re-stating something ticks at a fraction of the rate its output can show. Two unrelated wakes
//! per interval halve the frame rate, and nothing about the animation itself looks wrong while it
//! happens: the values are interpolated correctly against the clock, so the only symptom is that
//! the motion is made of half as many steps.
//!
//! So the moment is held rather than derived, moved only by the frames that reach it, and this is
//! the whole of the type that holds it.
//!
//! Two degradations are part of the definition rather than afterthoughts.
//!
//! **A late frame catches up without bursting.** A frame that runs after its due moment has passed
//! advances the phase by whole intervals until it is in the future again — one deadline, on the
//! original phase, however many were missed. Advancing by exactly one interval instead would leave
//! the deadline in the past and buy a frame immediately, once per interval that was missed, which
//! is a backlog drawn as fast as the machine can draw it.
//!
//! **A window that fell far behind starts again.** Past a bounded number of missed intervals the
//! phase carries no information worth keeping: what put the window there is a stall, an occlusion
//! or a suspend rather than a slow frame, and stepping back onto the old phase from an arbitrary
//! distance is a first interval of an arbitrary length. Such a window is anchored where it is.

use std::time::{Duration, Instant};

/// How many missed intervals a phase survives.
///
/// Below it a late frame is a slow frame and the phase is worth keeping; at it and above, the gap
/// is something that stopped the window rather than something that slowed it, and the phase it
/// left behind describes a rate nothing was running at.
const MAX_CATCH_UP: u128 = 8;

/// When the next animation frame is due, if any animation is running.
///
/// ```
/// use std::time::{Duration, Instant};
/// use zgui_runtime::AnimationCadence;
///
/// let interval = Duration::from_micros(4_167);
/// let start = Instant::now();
/// let mut cadence = AnimationCadence::parked();
/// assert_eq!(cadence.due(), None, "nothing is animating");
///
/// // The first frame of an animation is what starts the phase.
/// cadence.advance(start, interval);
/// assert_eq!(cadence.due(), Some(start + interval));
///
/// // A frame that ran for some other reason, before the moment that was owed, moves nothing.
/// cadence.advance(start + interval / 2, interval);
/// assert_eq!(cadence.due(), Some(start + interval));
///
/// // The frame that services it lands a little late, and the next one is owed on the phase
/// // rather than an interval after it arrived.
/// cadence.advance(start + interval + Duration::from_micros(300), interval);
/// assert_eq!(cadence.due(), Some(start + 2 * interval));
///
/// // Nothing is animating any more.
/// cadence.park();
/// assert_eq!(cadence.due(), None);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationCadence {
    /// The moment the next frame is owed at, or `None` while nothing is animating.
    due: Option<Instant>,
}

impl AnimationCadence {
    /// A window with nothing animating.
    pub const fn parked() -> Self {
        Self { due: None }
    }

    /// The moment the next animation frame is owed at, if one is owed at all.
    pub const fn due(&self) -> Option<Instant> {
        self.due
    }

    /// Forgets the phase, because nothing is animating any more.
    ///
    /// This is the whole of how an animation stops asking to be woken, and it is a separate act
    /// from advancing rather than a value advancing can produce: a window that is not animating owes
    /// no deadline at all, and a deadline left standing over a finished animation is a loop that
    /// wakes for ever and draws nothing.
    pub const fn park(&mut self) {
        self.due = None;
    }

    /// Moves the phase on for a frame that ran at `now` with an animation still running.
    ///
    /// Called by every frame while anything animates, and not only by the frames the deadline
    /// itself bought: a frame that ran for a keystroke halfway through an interval has already
    /// advanced the animation, and what it must not do is push the moment the next one is owed at.
    pub fn advance(&mut self, now: Instant, interval: Duration) {
        self.due = Some(match self.due {
            // The first frame of an animation, which is where its phase begins.
            None => now + interval,
            // Not yet reached: this frame ran for something else, and owes the phase nothing.
            Some(due) if due > now => due,
            Some(due) => Self::caught_up(due, now, interval),
        });
    }

    /// The next moment on `due`'s own phase that is still in the future at `now`.
    ///
    /// A phase that is more than [`MAX_CATCH_UP`] intervals behind is abandoned rather than
    /// stepped back onto, because the interval that would follow it is whatever the arithmetic
    /// happens to leave rather than a frame of the output.
    fn caught_up(due: Instant, now: Instant, interval: Duration) -> Instant {
        // A zero interval would divide by zero, and would ask for a frame at the moment it is
        // asked for — which is the busy loop this whole type exists to keep the window out of.
        // No output reports one, and one that did would be paced like the fallback instead.
        let period = interval.as_nanos().max(1);
        let missed = now.saturating_duration_since(due).as_nanos() / period;
        if missed >= MAX_CATCH_UP {
            return now + interval;
        }
        // Missed whole intervals, plus the one that carries the moment past `now`. Both halves
        // matter: without the first the deadline stays in the past and a backlog is drawn at once,
        // and without the second it lands exactly on `now` and buys a frame that is already late.
        let steps = u32::try_from(missed + 1).unwrap_or(u32::MAX);
        due + interval * steps
    }
}

#[cfg(test)]
mod tests {
    use super::{AnimationCadence, MAX_CATCH_UP};
    use std::time::{Duration, Instant};

    /// One frame of a two-hundred-and-forty hertz output.
    const FAST: Duration = Duration::from_micros(4_167);

    /// One frame of a sixty hertz output.
    const SLOW: Duration = Duration::from_micros(16_667);

    #[test]
    fn unrelated_frames_between_two_ticks_do_not_move_the_moment_that_is_owed() {
        // The defect this type exists for. Frames arrive for reasons of their own — a pointer
        // sample, a compositor re-stating something, a task finishing — and a deadline derived as
        // "now plus an interval" is re-anchored by every one of them, which is an animation that
        // ticks at a fraction of the rate of the output it is on.
        let start = Instant::now();
        let mut cadence = AnimationCadence::parked();
        cadence.advance(start, FAST);
        let owed = cadence.due().expect("the animation is running");

        for step in 1..=9 {
            cadence.advance(start + FAST * step / 10, FAST);
            assert_eq!(
                cadence.due(),
                Some(owed),
                "a frame that ran for something else pushed the animation's own deadline"
            );
        }
    }

    #[test]
    fn a_run_of_ticks_keeps_exactly_one_interval_between_them() {
        // Every tick lands a little late, as a real one does: the loop wakes at the deadline and
        // the frame begins after it. What must not accumulate is that lateness.
        let start = Instant::now();
        let late = Duration::from_micros(400);
        let mut cadence = AnimationCadence::parked();
        cadence.advance(start, FAST);

        let mut owed = Vec::new();
        for _ in 0..64 {
            let due = cadence.due().expect("the animation is running");
            owed.push(due);
            cadence.advance(due + late, FAST);
        }

        assert!(
            owed.windows(2).all(|pair| pair[1] - pair[0] == FAST),
            "the deadlines drifted apart: {:?}",
            owed.windows(2)
                .map(|pair| pair[1] - pair[0])
                .collect::<Vec<_>>()
        );
        assert_eq!(
            owed.last().copied(),
            Some(start + FAST * 64),
            "sixty-four ticks did not cover sixty-four intervals"
        );
    }

    #[test]
    fn a_frame_that_overran_catches_up_to_one_deadline_and_not_to_a_backlog() {
        // A frame that costs more than an interval is the ordinary case on a fast output, and the
        // wrong answer is a queue: one deadline per interval that was missed, all of them already
        // in the past, each buying a frame the moment the loop looks at it.
        let start = Instant::now();
        let mut cadence = AnimationCadence::parked();
        cadence.advance(start, FAST);

        let overran = start + FAST * 3 + Duration::from_micros(200);
        cadence.advance(overran, FAST);
        let due = cadence.due().expect("the animation is running");

        assert!(
            due > overran,
            "the deadline was left in the past: a backlog"
        );
        assert_eq!(due, start + FAST * 4, "the phase was not kept");
    }

    #[test]
    fn a_window_that_fell_far_behind_is_anchored_where_it_is() {
        // What an occlusion, a suspend or a stalled compositor leaves behind. Stepping back onto a
        // phase from an arbitrary distance makes the first interval an arbitrary length.
        let start = Instant::now();
        let mut cadence = AnimationCadence::parked();
        cadence.advance(start, SLOW);

        let resumed = start + SLOW * 400 + SLOW / 3;
        cadence.advance(resumed, SLOW);

        assert_eq!(
            cadence.due(),
            Some(resumed + SLOW),
            "a window minutes behind stepped back onto a phase nothing was running at"
        );
    }

    #[test]
    fn the_last_interval_of_the_catch_up_window_still_keeps_its_phase() {
        // The boundary itself, so that the bound is a decision rather than an accident.
        let start = Instant::now();
        let mut cadence = AnimationCadence::parked();
        cadence.advance(start, SLOW);

        let steps = u32::try_from(MAX_CATCH_UP).expect("the bound fits");
        cadence.advance(start + SLOW * steps, SLOW);
        assert_eq!(
            cadence.due(),
            Some(start + SLOW * (steps + 1)),
            "the phase was kept one interval past the bound"
        );

        let mut other = AnimationCadence::parked();
        other.advance(start, SLOW);
        let far = start + SLOW * (steps + 1);
        other.advance(far, SLOW);
        assert_eq!(other.due(), Some(far + SLOW), "the bound did not apply");
    }

    #[test]
    fn parking_and_starting_again_begins_a_fresh_phase() {
        // An animation that finished and another that started later share nothing. The second one's
        // first interval is measured from its own first frame, which is what stops a transition
        // begun a second after the last one ended from ticking immediately.
        let start = Instant::now();
        let mut cadence = AnimationCadence::parked();
        cadence.advance(start, SLOW);
        cadence.park();
        assert_eq!(cadence.due(), None);

        let later = start + Duration::from_secs(3);
        cadence.advance(later, SLOW);
        assert_eq!(cadence.due(), Some(later + SLOW));
    }
}
