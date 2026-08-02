//! A resize is a level, not an event stream.
//!
//! A window system does not report *that a resize happened*; it reports *what size the window is
//! now*. Every configure supersedes the one before it, and a configure that has been superseded is
//! worth nothing: laying out for it, painting it and rebuilding the swapchain for it produces
//! pixels that are already wrong before they are submitted.
//!
//! Two things follow, and this type is both of them.
//!
//! **The newest size is the only size.** The extent a frame is built for is read from the surface
//! when the frame runs, never from the event that asked for it, so a frame is always built for
//! what the window is rather than for what it was when something asked.
//!
//! **A configure is only worth answering as often as the answer can be seen.** Rebuilding a
//! swapchain waits for the device to go completely idle, so a resize step is expensive in a way
//! that does not get cheaper when the window is small — and answering ten configures inside one
//! refresh interval puts nine of those stalls between the compositor and a picture nobody will
//! ever look at. Above the output's refresh rate the extra frames are not merely wasted: they are
//! commits the compositor has to service, taken out of the budget it needs to show the one frame
//! that will actually be scanned out.
//!
//! So a configure that arrives less than one refresh interval after the last one that was answered
//! does not ask for a frame. It moves the level, and the moment the interval closes is a deadline
//! the park already knows how to wait on. The frame that runs then is built for whatever the
//! window is *at that moment* — which is the newest configure, not the one that installed the
//! deadline.
//!
//! **What is paced is the frame, not the configure.** The stall belongs to the swapchain rebuild
//! and not to whatever asked for it, so while a reconfiguration is owed and could not yet be seen,
//! a pointer sample, an animation tick and a task finishing on another thread are refused exactly
//! as a configure is. Pacing only the configures leaves the whole design conditional on nothing
//! else happening during a drag — and a drag is the one time something always is, which is how a
//! window comes to rebuild its swapchain once per pointer sample while believing it is pacing.
//!
//! An isolated configure is never delayed by any of this: the pacing only ever measures from a
//! resize frame that has already run, so the first configure after a quiet window is answered in
//! the turn it arrives in, and a window that owes no reconfiguration is never refused anything at
//! all.

use std::time::{Duration, Instant};

/// How often a window's size may be answered with a frame, and what that has skipped.
///
/// ```
/// use std::time::{Duration, Instant};
/// use zgui_runtime::ResizePace;
///
/// let interval = Duration::from_millis(16);
/// let mut pace = ResizePace::new();
/// let now = Instant::now();
///
/// // Nothing has been answered yet, so the first configure is answered where it arrives.
/// assert!(pace.admit(now, interval));
/// pace.answered(now);
///
/// // A second one inside the same interval moves the level and waits for the deadline.
/// assert!(!pace.admit(now + Duration::from_millis(1), interval));
/// assert_eq!(pace.due(interval), Some(now + interval));
/// assert_eq!(pace.deferred(), 1);
///
/// // Once the interval has closed, the next one is answered again.
/// assert!(pace.admit(now + interval, interval));
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct ResizePace {
    /// When the last frame that reconfigured the surface began.
    answered: Option<Instant>,
    /// How many configures moved the level without asking for a frame of their own.
    deferred: u64,
}

impl ResizePace {
    /// A window that has answered nothing yet.
    pub const fn new() -> Self {
        Self {
            answered: None,
            deferred: 0,
        }
    }

    /// Whether a configure arriving at `now` may ask for a frame straight away.
    ///
    /// Answering `false` is not a refusal to draw. The level has already moved, the obligation to
    /// reconfigure is already recorded, and [`ResizePace::due`] is when it will be discharged — so
    /// the configure is answered by a frame that is built for whatever the window is by then,
    /// rather than by a frame of its own that a later configure would immediately invalidate.
    pub fn admit(&mut self, now: Instant, interval: Duration) -> bool {
        if self.too_soon(now, interval) {
            self.deferred += 1;
            return false;
        }
        true
    }

    /// Whether a frame that reconfigured the surface at `now` could not be seen yet.
    ///
    /// This is [`ResizePace::admit`] without the bookkeeping, and it is asked of *every* frame
    /// rather than only of a configure. A frame that reconfigures costs the same swapchain rebuild
    /// whatever asked for it — so while one is owed and cannot yet be shown, a pointer sample, an
    /// animation tick or a task finishing on another thread must not buy one either. Counting only
    /// configures leaves the whole pacing conditional on nothing else happening during a drag,
    /// which is the one time something always is.
    ///
    /// A window that has never answered a configure is never too soon: there is nothing on its way
    /// to the screen to wait behind.
    pub fn too_soon(&self, now: Instant, interval: Duration) -> bool {
        self.answered
            .is_some_and(|last| now.saturating_duration_since(last) < interval)
    }

    /// The moment a deferred configure may be answered, if one has ever been answered before.
    ///
    /// The caller asks this only while a reconfiguration is owed; a window whose size is settled
    /// owes nothing and must install no deadline at all, because a deadline installed on a window
    /// with nothing to do is a loop that wakes for ever and draws nothing.
    pub fn due(&self, interval: Duration) -> Option<Instant> {
        self.answered.map(|last| last + interval)
    }

    /// Records that a frame has just been built for the window's current size.
    pub fn answered(&mut self, now: Instant) {
        self.answered = Some(now);
    }

    /// How many configures have moved the level without a frame of their own.
    ///
    /// This is the count of pipeline runs a drag did *not* pay for: layout, paint and a swapchain
    /// rebuild each, for a size that was superseded before anything could have been seen.
    pub const fn deferred(&self) -> u64 {
        self.deferred
    }
}

#[cfg(test)]
mod tests {
    use super::ResizePace;
    use std::time::{Duration, Instant};

    /// One refresh interval of a seventy-five hertz output, near enough.
    const INTERVAL: Duration = Duration::from_micros(13_347);

    #[test]
    fn the_first_configure_a_quiet_window_gets_is_never_delayed() {
        // The regression this guards: pacing that measures from "the last configure" rather than
        // "the last frame that answered one" delays every single resize by a refresh interval,
        // including the one that arrives after a minute of stillness.
        let mut pace = ResizePace::new();
        assert!(pace.admit(Instant::now(), INTERVAL));
        assert_eq!(pace.deferred(), 0);
    }

    #[test]
    fn a_burst_inside_one_interval_asks_for_exactly_one_frame() {
        let mut pace = ResizePace::new();
        let start = Instant::now();
        assert!(pace.admit(start, INTERVAL));
        pace.answered(start);

        let admitted = (1..40)
            .filter(|step| pace.admit(start + Duration::from_micros(100 * step), INTERVAL))
            .count();
        assert_eq!(
            admitted, 0,
            "every configure inside the interval asked for a pipeline run of its own"
        );
        assert_eq!(pace.deferred(), 39);
    }

    #[test]
    fn the_deadline_is_one_interval_from_the_frame_that_was_answered() {
        let mut pace = ResizePace::new();
        let start = Instant::now();
        assert_eq!(pace.due(INTERVAL), None, "nothing has been answered yet");
        pace.answered(start);
        assert_eq!(pace.due(INTERVAL), Some(start + INTERVAL));
    }

    #[test]
    fn a_faster_output_is_answered_more_often() {
        // The whole point of taking the interval from the surface rather than from a constant: the
        // same burst of configures buys four frames on a two-hundred-and-forty hertz output where
        // it buys one on a seventy-five hertz one.
        let fast = Duration::from_micros(4_167);
        let mut slow_pace = ResizePace::new();
        let mut fast_pace = ResizePace::new();
        let start = Instant::now();
        slow_pace.answered(start);
        fast_pace.answered(start);

        let mut slow = 0;
        let mut quick = 0;
        for step in 1..=13 {
            let now = start + Duration::from_millis(step);
            if slow_pace.admit(now, INTERVAL) {
                slow += 1;
                slow_pace.answered(now);
            }
            if fast_pace.admit(now, fast) {
                quick += 1;
                fast_pace.answered(now);
            }
        }
        assert_eq!(slow, 0, "nothing more fits inside one interval at 75 Hz");
        assert!(quick >= 2, "a 240 Hz output answered only {quick} of 13");
    }

    #[test]
    fn what_a_configure_is_refused_for_is_exactly_what_every_other_frame_is_refused_for() {
        // The two must never drift apart. `admit` is what a configure asks and `too_soon` is what
        // the redraw the backend produced for it asks, and a window that answered them differently
        // would pace the configures it can see and rebuild the swapchain for everything else.
        let start = Instant::now();
        for offset in [0, 1, 6_000, 13_346, 13_347, 13_348, 40_000] {
            let now = start + Duration::from_micros(offset);
            let mut paced = ResizePace::new();
            paced.answered(start);
            let early = paced.too_soon(now, INTERVAL);
            assert_eq!(
                paced.admit(now, INTERVAL),
                !early,
                "at +{offset}us a configure and a redraw disagreed about the same moment"
            );
        }
    }

    #[test]
    fn a_window_that_has_answered_nothing_is_never_too_soon_for_anything() {
        // The stall this rules out: a window whose very first frame is refused has no deadline to
        // be asked again by, because the deadline is measured from a frame that has run.
        let pace = ResizePace::new();
        assert!(!pace.too_soon(Instant::now(), INTERVAL));
        assert_eq!(pace.due(INTERVAL), None);
    }

    #[test]
    fn every_moment_that_is_too_soon_has_a_deadline_strictly_after_it() {
        // What makes refusing a frame safe rather than a hang. The refusal drops the request, so
        // the only thing that can ask again is the deadline — and a deadline that is not strictly
        // in the future is one the loop refuses to install.
        let start = Instant::now();
        let mut pace = ResizePace::new();
        pace.answered(start);
        for offset in [0, 1, 6_000, 13_346] {
            let now = start + Duration::from_micros(offset);
            assert!(pace.too_soon(now, INTERVAL));
            assert!(
                pace.due(INTERVAL).is_some_and(|due| due > now),
                "a frame refused at +{offset}us had nothing left to ask for it"
            );
        }
    }

    #[test]
    fn a_frame_that_costs_more_than_an_interval_is_never_paced() {
        // Pacing must never make a slow application slower. When the frame itself outlasts the
        // refresh interval, every configure that arrives is already outside it.
        let mut pace = ResizePace::new();
        let start = Instant::now();
        pace.answered(start);
        assert!(pace.admit(start + INTERVAL * 3, INTERVAL));
        assert_eq!(pace.deferred(), 0);
    }
}
