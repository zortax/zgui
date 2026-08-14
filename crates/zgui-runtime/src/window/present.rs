//! When a frame that has been asked for is allowed to start.
//!
//! A loop that draws the instant it is asked hands the window system a picture of the world as it
//! was when the last buffer was released — which, on a swap chain with an image to spare, is a
//! whole refresh interval before anybody sees it. The frame then spends that interval blocked
//! inside the call that asks for a surface to present into, on the thread that reads input, and the
//! pointer sample that arrives one microsecond later waits for all of it.
//!
//! Starting the frame **later** costs nothing and buys the difference. Nothing about when the
//! picture is shown changes; what changes is how old it is when it is shown, and how long the loop
//! spends unable to answer a pointer.
//!
//! # The hold is a servo, not a schedule
//!
//! No phase is estimated and nothing is asked of the window system. The one observation is
//! [`Renderer::acquire_block`](zgui_render::Renderer::acquire_block) — how long the last frame
//! waited to be handed a surface — and a frame that waited a long time was started far too early
//! while one that did not wait at all has already missed the handover it was aiming at. Driving
//! that wait towards a small positive `BUDGET` is the same thing as starting each frame just
//! early enough to finish.
//!
//! Half the error is taken each time. The observation carries a refresh interval's worth of
//! quantisation, and correcting it in full chases the sample instead of the phase.
//!
//! The servo's fixed point when nothing ever blocks is a hold of zero, so a window presenting to
//! something that never makes it wait — an offscreen surface, a swap chain with headroom to spare —
//! is paced exactly as it would be with none of this here.
//!
//! # The gate
//!
//! **A window that cannot keep up with its display is not held at all.** There is no slack to
//! schedule into when a frame costs more than a refresh interval: the acquisition never blocks, the
//! frames are already late, and a hold left over from when the window *could* keep up delays frames
//! that had nothing to spare. Waiting for the servo to unwind at half the error a frame is several
//! intervals of exactly that, so the gate drops the hold outright instead.
//!
//! What the gate reads is the hold *plus* the frame, because that is what the frame occupied of the
//! interval. A frame released four milliseconds late and running for three keeps up on a display
//! whose interval is seven only if the four are not counted — and the hold that pushed it over is
//! then left in place to push the next one over as well.
//!
//! # The reservation
//!
//! The servo converges on a margin measured against the frame that has just run, which is the right
//! answer for a window whose frames all cost the same and the wrong one for every other window. A
//! window with a tail — a row mounting under a fling, a document relaying out under a drag — has a
//! median that fits with most of the interval to spare, so the servo gives that spare time away and
//! the dear frame has nothing left to run in. It then costs a whole interval instead of a few
//! milliseconds, which is a visible stutter that the average frametime does not contain.
//!
//! So the hold is capped a second time, at what leaves room for the dearest frame seen lately. It
//! costs a steady window nothing, because there the dearest frame is the usual one.

use std::cell::Cell;
use std::time::{Duration, Instant};

/// How long the acquisition should still block once the hold has converged.
///
/// Not zero. A hold tuned until the frame is handed a surface the moment it asks leaves the frame
/// no margin at all, and the first one that costs a little more than its predecessor misses the
/// handover entirely — which is a whole refresh interval spent, to save two milliseconds. This is
/// the margin the loop keeps for a frame that runs long.
const BUDGET: Duration = Duration::from_micros(2_000);

/// What the recent worst frame cost is multiplied by each frame.
///
/// A window that has slowed down has to be believed at once, and one that has sped up has to be
/// believed slowly: the frame that was dear will very likely be dear again, and giving its margin
/// away on the strength of one cheap frame after it is how a burst of expensive frames comes to
/// drop every second one. Halves in about thirty-five frames, which is a quarter of a second on a
/// fast display and half of one on a slow one: long enough to hold a fling's margin across the
/// cheap frames between its dear ones, short enough that a window is not paced by a resize it
/// finished with.
///
/// What it guarantees is one frame deep — the frame after a dear one is never pushed past the
/// interval, because the dear one's cost is still whole. Beyond that it is an estimate, and the
/// gate is what catches a spike nothing could have predicted.
const DECAY: f32 = 0.98;

/// When a frame that has been asked for is allowed to start, and how that moment is arrived at.
///
/// ```
/// use std::time::{Duration, Instant};
/// use zgui_runtime::PresentPace;
///
/// let interval = Duration::from_micros(13_347);
/// let mut pace = PresentPace::free_running();
/// let now = Instant::now();
///
/// // Nothing has been observed yet, so a frame asked for is a frame that runs.
/// assert_eq!(pace.hold(), Duration::ZERO);
/// assert!(!pace.holds_a_frame(now));
///
/// // A frame that spent eight milliseconds waiting to be handed a surface was started about that
/// // much too early, and half the error is taken back.
/// pace.observed(Duration::from_millis(8), Duration::from_micros(500), interval);
/// assert_eq!(pace.hold(), Duration::from_millis(3));
///
/// // The next frame asked for is held until that moment, and released when it arrives.
/// assert!(pace.holds_a_frame(now));
/// assert_eq!(pace.due(), Some(now + Duration::from_millis(3)));
/// assert!(!pace.holds_a_frame(now + Duration::from_millis(3)));
/// ```
#[derive(Clone, Debug, Default)]
pub struct PresentPace {
    /// How long the next frame asked for is held back before it starts.
    hold: Duration,
    /// The moment the frame being held is let go at, or `None` while none is held.
    ///
    /// A cell because deciding whether an offered frame runs is a question a window answers
    /// without being borrowed mutably, and the answer is the whole of what is remembered about it.
    until: Cell<Option<Instant>>,
    /// What a frame has recently cost at its worst, decaying towards what one costs now.
    ///
    /// The hold is reserved out of the same interval the frame has to finish in, so how much of it
    /// can be given away is a question about the *dearest* frame and not the usual one. Kept as a
    /// decaying maximum rather than a window of samples: one number, no allocation, and a frame
    /// that was expensive once stops being paid for once the window has settled.
    worst: Duration,
}

impl PresentPace {
    /// A window that holds nothing, which is what one that has never presented a frame is.
    pub const fn free_running() -> Self {
        Self {
            hold: Duration::ZERO,
            until: Cell::new(None),
            worst: Duration::ZERO,
        }
    }

    /// How long the next frame asked for is held back.
    pub const fn hold(&self) -> Duration {
        self.hold
    }

    /// The moment a held frame is owed at, if one is being held.
    pub fn due(&self) -> Option<Instant> {
        self.until.get()
    }

    /// Whether a frame offered at `now` is still being held back.
    ///
    /// Asking is what starts the hold: the first offer after a frame has run is the moment the
    /// frame was asked for, and the moment it is let go at is measured from there. Later offers
    /// inside the same hold do not move it — a stream of pointer samples during the hold would
    /// otherwise push the frame away from the display for as long as the finger kept moving, which
    /// is the opposite of what holding it is for.
    ///
    /// The events those offers arrived with are not lost and are not deferred with the frame: they
    /// are queued where they always were, and the frame that eventually runs is built from all of
    /// them. That is the second half of what the hold buys — one frame answering the whole burst,
    /// as late and therefore as current as it can be.
    pub fn holds_a_frame(&self, now: Instant) -> bool {
        if self.hold.is_zero() {
            self.until.set(None);
            return false;
        }
        match self.until.get() {
            Some(until) if until > now => true,
            Some(_) => {
                self.until.set(None);
                false
            }
            None => {
                self.until.set(Some(now + self.hold));
                true
            }
        }
    }

    /// Records what a frame that has just run cost, and moves the hold on.
    ///
    /// `blocked` is how long that frame waited to be handed a surface to present into, `cost` how
    /// long the frame itself took once it was released, and `interval` one frame of the output.
    pub fn observed(&mut self, blocked: Duration, cost: Duration, interval: Duration) {
        self.until.set(None);
        // Held plus spent, because that is what the frame occupied of the interval. Measuring the
        // cost alone reads a frame that was released four milliseconds late and ran for three as
        // keeping up on a display whose interval is seven, so the frame that missed its handover
        // reports as the frame that made it and the hold that caused the miss survives it.
        let occupied = self.hold + cost;
        self.decay(cost);
        if occupied >= interval {
            // The gate. Not a step towards zero: a window that has fallen behind its display owes
            // every frame it can produce as early as it can produce it, and unwinding a hold built
            // up while it was keeping up would delay several of them on the way down.
            self.hold = Duration::ZERO;
            return;
        }
        // Two ceilings, and the lower one wins.
        //
        // The first is the interval itself: a frame held past the moment its buffer could have been
        // handed over is not started late, it is started for the interval after the one it was
        // asked in — a dropped frame dressed up as a schedule.
        //
        // The second is what the window's own frames cost. The servo converges by driving the
        // acquisition's wait down to the budget, which is a margin measured against the frame that
        // has *just* run; a window whose frames vary — a row mounting under a fling, an icon
        // re-encoded, a resize — then has no margin left for the dear one, and every one of those
        // costs a whole interval instead of a few milliseconds. Reserving the dearest recent frame
        // gives the tail back what the average would otherwise spend, and it costs a steady window
        // nothing, because there the dearest frame *is* the usual one.
        let ceiling = interval
            .saturating_sub(BUDGET)
            .min(interval.saturating_sub(self.worst + BUDGET));
        self.hold = if blocked > BUDGET {
            (self.hold + (blocked - BUDGET) / 2).min(ceiling)
        } else {
            self.hold.saturating_sub((BUDGET - blocked) / 2).min(ceiling)
        };
    }

    /// How much a frame recently cost at its worst, decayed towards `cost`.
    fn decay(&mut self, cost: Duration) {
        self.worst = self.worst.mul_f32(DECAY).max(cost);
    }
}

#[cfg(test)]
mod tests {
    use super::{BUDGET, PresentPace};
    use std::time::{Duration, Instant};

    /// One frame of the output every figure in this module was settled against.
    const INTERVAL: Duration = Duration::from_micros(13_347);

    /// What a frame started a whole interval too early waits for.
    const FREE_RUNNING: Duration = Duration::from_micros(9_960);

    /// A frame that costs almost nothing, which is what a steady window's frames are.
    const CHEAP: Duration = Duration::from_micros(300);

    #[test]
    fn a_window_that_is_never_made_to_wait_is_never_held() {
        // The offscreen case, and the swap chain with an image to spare. Both report a block of
        // zero, and the servo's fixed point there has to be a hold of zero — otherwise every
        // window that presents to nothing acquires a schedule it has no display to keep.
        let mut pace = PresentPace::free_running();
        for _ in 0..64 {
            pace.observed(Duration::ZERO, CHEAP, INTERVAL);
        }
        assert_eq!(pace.hold(), Duration::ZERO);
        assert!(!pace.holds_a_frame(Instant::now()));
        assert_eq!(pace.due(), None);
    }

    #[test]
    fn a_free_running_loop_converges_on_a_hold_that_leaves_the_budget() {
        // The steady state the whole mechanism exists to reach: a loop blocking most of an
        // interval inside the acquisition settles at a hold that leaves it blocking the budget
        // and no more. Modelled as the real thing behaves — the block falls by whatever the hold
        // grew by, because the frame now starts that much later against the same display.
        let mut pace = PresentPace::free_running();
        let mut blocked = FREE_RUNNING;
        for _ in 0..32 {
            pace.observed(blocked, CHEAP, INTERVAL);
            blocked = FREE_RUNNING.saturating_sub(pace.hold());
        }
        assert!(
            pace.hold() >= FREE_RUNNING - BUDGET - Duration::from_micros(100),
            "the hold stopped short of the slack there was: {:?}",
            pace.hold()
        );
        assert!(
            pace.hold() <= FREE_RUNNING,
            "the hold overshot the slack there was: {:?}",
            pace.hold()
        );
    }

    #[test]
    fn the_hold_the_frame_and_the_budget_never_exceed_one_frame_of_the_output() {
        // A frame held past the moment its buffer could have been handed over is not late, it is
        // in the next interval — the dropped frame this bound exists to refuse. Driven with a
        // block far larger than any real one, which is what a compositor that stalled produces.
        let mut pace = PresentPace::free_running();
        for _ in 0..64 {
            pace.observed(Duration::from_millis(500), CHEAP, INTERVAL);
        }
        assert_eq!(pace.hold(), INTERVAL - CHEAP - BUDGET);
        assert!(pace.hold() + CHEAP + BUDGET <= INTERVAL);
    }

    #[test]
    fn a_frame_that_did_not_keep_up_drops_the_hold_at_once() {
        // The gate, and the reason it is not a step towards zero. A window that reaches this has
        // frames costing more than its display can show; every one of them is already late, and
        // half an error a frame is several more of them delayed on the way down.
        let mut pace = PresentPace::free_running();
        for _ in 0..8 {
            pace.observed(FREE_RUNNING, CHEAP, INTERVAL);
        }
        assert!(pace.hold() > Duration::ZERO, "nothing was there to drop");

        pace.observed(Duration::ZERO, INTERVAL, INTERVAL);
        assert_eq!(pace.hold(), Duration::ZERO);
        assert!(!pace.holds_a_frame(Instant::now()));
    }

    /// A frame that only fits because the hold was not counted did not fit.
    ///
    /// The block the servo reads is the *acquisition's*, and a frame released too late does not
    /// block at all — it has already missed the handover it was aiming at, so a swap-chain image is
    /// waiting for it. Reading that as "kept up" leaves the hold that caused the miss in place, and
    /// the servo then unwinds it at half the error a frame while the window drops one after
    /// another.
    #[test]
    fn a_frame_the_hold_pushed_past_the_interval_did_not_keep_up() {
        let mut pace = PresentPace::free_running();
        for _ in 0..16 {
            pace.observed(FREE_RUNNING, CHEAP, INTERVAL);
        }
        let held = pace.hold();
        assert!(held > Duration::ZERO, "nothing was there to drop");

        // Cheaper than the interval on its own, and dearer than it once the hold is added back.
        let cost = INTERVAL - held + Duration::from_micros(1);
        assert!(cost < INTERVAL, "the frame has to fit when measured alone");
        pace.observed(Duration::ZERO, cost, INTERVAL);
        assert_eq!(pace.hold(), Duration::ZERO);
    }

    /// The frame after a dear one is never pushed past the interval.
    ///
    /// This is the guarantee, and it is one frame deep on purpose. A spike nothing has seen before
    /// cannot be reserved for and the gate is what catches it; what can be reserved for is the next
    /// one, and frames that cost a lot arrive in runs — a fling mounting rows, a drag relaying out,
    /// a document whose icons all re-encode at once.
    #[test]
    fn a_frame_as_dear_as_the_last_one_still_fits_the_interval() {
        let dear = INTERVAL / 2;
        let mut pace = PresentPace::free_running();
        for _ in 0..64 {
            pace.observed(FREE_RUNNING, CHEAP, INTERVAL);
        }
        pace.observed(FREE_RUNNING, dear, INTERVAL);
        assert!(
            pace.hold() + dear + BUDGET <= INTERVAL,
            "a second frame costing {dear:?} would be pushed past the interval by a hold of {:?}",
            pace.hold()
        );
    }

    /// A window with a tail is held less than one without, and gets the margin back when it settles.
    ///
    /// A window whose frames vary has a median that fits with most of an interval to spare and a
    /// tail that does not. Converging on the median spends that spare time and leaves the tail
    /// nothing, so every dear frame costs a whole interval of judder — which the average frametime
    /// never shows. The reservation is the part of the interval the median is not allowed to spend.
    #[test]
    fn a_window_with_a_tail_keeps_more_of_its_interval_and_gives_it_back() {
        let dear = INTERVAL / 3;
        let mut steady = PresentPace::free_running();
        let mut varying = PresentPace::free_running();
        for index in 0..96 {
            steady.observed(FREE_RUNNING, CHEAP, INTERVAL);
            // One frame in eight is dear; the rest cost almost nothing.
            let cost = if index % 8 == 0 { dear } else { CHEAP };
            varying.observed(FREE_RUNNING, cost, INTERVAL);
        }
        assert!(
            steady.hold() > varying.hold(),
            "the window with nothing to reserve for was held no longer than the one with a tail: \
             {:?} against {:?}",
            steady.hold(),
            varying.hold()
        );

        // And the reservation is given back once the tail stops, so a burst does not pace a window
        // for the rest of its life. It decays rather than being dropped, so this takes a few
        // hundred frames — a second or two of a window that has settled.
        for _ in 0..300 {
            varying.observed(FREE_RUNNING, CHEAP, INTERVAL);
        }
        assert_eq!(varying.hold(), steady.hold());
    }

    #[test]
    fn a_burst_of_offers_inside_one_hold_does_not_push_the_frame_away() {
        // What a scroll actually looks like: a wheel notch asks for a frame, is held, and every
        // notch behind it asks again while the hold runs. Restarting the hold on each of them is a
        // frame that recedes for as long as the finger keeps moving.
        let mut pace = PresentPace::free_running();
        pace.observed(FREE_RUNNING, CHEAP, INTERVAL);
        let start = Instant::now();
        let owed = {
            assert!(pace.holds_a_frame(start));
            pace.due().expect("a frame is being held")
        };
        for step in 1..=8 {
            assert!(pace.holds_a_frame(start + Duration::from_micros(100) * step));
            assert_eq!(
                pace.due(),
                Some(owed),
                "an offer moved the moment that was owed"
            );
        }
        assert!(!pace.holds_a_frame(owed), "the frame was not let go");
        assert_eq!(pace.due(), None);
    }

    #[test]
    fn the_hold_starts_again_for_the_frame_after_the_one_it_released() {
        // One hold per frame, and the next frame gets its own. Without this the first frame of a
        // scroll is scheduled and every one after it free-runs.
        let mut pace = PresentPace::free_running();
        pace.observed(FREE_RUNNING, CHEAP, INTERVAL);
        let start = Instant::now();
        assert!(pace.holds_a_frame(start));
        let owed = pace.due().expect("a frame is being held");
        assert!(!pace.holds_a_frame(owed));

        pace.observed(BUDGET, CHEAP, INTERVAL);
        assert!(
            pace.holds_a_frame(owed),
            "the frame after the released one was not held"
        );
    }
}
