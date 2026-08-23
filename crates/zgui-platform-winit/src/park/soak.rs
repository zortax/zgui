//! A long run over the boundary, because the failure it guards is one turn in four hundred thousand.
//!
//! The properties next door are each one shape of the mistake, asserted once. This is the same
//! invariant asserted after every turn of a run long enough that a defect surviving it would have
//! to be rarer than the one that froze three campaigns.
//!
//! The invariant is stated as a state and not as a count, because the failure is a state: the loop
//! parked indefinitely, nothing pending that would end the park, and the application still wanting
//! a moment that has already passed. Nothing further ever happens from there. One turn in that
//! state is a frozen window, so the bound is zero rather than small.
//!
//! Both dimensions move on every turn. The distance the application asks for sweeps the range
//! either side of the gap, so the run contains moments the gap swallows and moments it does not
//! and every tie in between; the gap itself moves too, because a fixed one only ever tests one
//! cut of the space. The sequence is a fixed generator with a fixed seed, so a failure is a
//! failure anybody can reproduce.

use std::time::Duration;

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
};

use crate::park::model::{Cause, Model, Reading};
use zgui_platform::Parked;

/// How many turns the soak takes.
///
/// Sized against the rate the frozen loop was reproduced at, which was about one turn in four
/// hundred thousand: a run of this length would have met it many times over. It is shorter under
/// the unoptimised build only because the same number of turns there buys the same coverage at ten
/// times the cost, and the coverage is what the number is for.
#[cfg(debug_assertions)]
const TURNS: u32 = 400_000;
/// How many turns the soak takes.
#[cfg(not(debug_assertions))]
const TURNS: u32 = 4_000_000;

/// The widest distance ahead the application ever asks for.
const REACH: u64 = 40;

/// The widest gap between the application deciding and the loop installing.
const GAP: u64 = 20;

/// An application that asks for a frame a little ahead, over and over, for ever.
///
/// It re-arms from inside its own frame, which is what an animation does and what makes every turn
/// of the soak a fresh trip through the boundary rather than one trip and a long sleep.
struct Ticking {
    /// The moment it wants a frame at, in microseconds from the model's start.
    ahead: u64,
    /// How many frames it has been given.
    frames: u64,
    /// How many times it was told a moment had arrived.
    arrivals: u64,
    /// Whether it currently wants a frame at all.
    due: Option<std::time::Instant>,
}

impl AppHandler for Ticking {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        cx.create_surface(&SurfaceAttributes::new("soak"))
            .expect("a surface over the model is always creatable");
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
        if !matches!(event, SurfaceEvent::RedrawRequested) {
            return;
        }
        self.frames += 1;
        self.due = Some(cx.clock().now() + Duration::from_micros(self.ahead));
    }

    fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {
        // Nothing reaches this application from another thread; its only clock is the one the
        // model moves.
    }

    fn idle(&mut self, _cx: &dyn PlatformCx) -> IdlePolicy {
        // Reported without clamping, exactly as the runtime reports it: the moment is chosen
        // against this reading of the clock and installed against a later one, and closing the gap
        // here would leave the thing under test unexercised.
        self.due.map_or(IdlePolicy::Block, IdlePolicy::BlockUntil)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.arrivals += 1;
        for surface in cx.surfaces() {
            surface.request_redraw();
        }
    }
}

/// A fixed sequence, so a failure is one anybody can reproduce.
struct Sequence(u64);

impl Sequence {
    /// The next value below `modulus`.
    const fn next(&mut self, modulus: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % modulus
    }
}

/// What a soak run observed.
struct Run {
    /// How many frames ran.
    frames: u64,
    /// How many moments were handed over because they had passed before they could be installed.
    swallowed: u64,
    /// How many moments were handed over by the loop waking on them as planned.
    waited: u64,
    /// The last turn on which a frame ran.
    ///
    /// The freeze is not "no frames"; the loop that shipped drew perfectly well until the first
    /// moment fell into the gap, and only then stopped for ever. So the number that separates a
    /// healthy run from a frozen one is when it last drew, not how much.
    last_frame: u32,
}

/// Runs the soak over `reading` and reports what happened, asserting the invariant every turn.
///
/// The assertion is inside the loop rather than at the end because the state it names is absorbing:
/// once the loop is parked on nothing with work owed, every later turn looks identical and a
/// summary taken at the end cannot say which turn was the first to be wrong.
fn soak(reading: Reading, assert_invariant: bool) -> Run {
    let mut model = Model::misreading(
        Ticking {
            ahead: 0,
            frames: 0,
            arrivals: 0,
            due: None,
        },
        reading,
    );
    let mut sequence = Sequence(0x2545_f491_4f6c_dd1d);
    let mut swallowed = 0;
    let mut waited = 0;
    let mut last_frame = 0;

    model.app_mut().ahead = sequence.next(REACH);
    model.app_mut().due = Some(model.now() + Duration::from_micros(model.app().ahead));

    for turn in 0..TURNS {
        // The loop was blocked, and time passed while it was. Without this the only thing moving
        // the clock would be the gap itself, and every moment would be met at the install — the
        // run would never once take the ordinary route of waking on a moment it waited for.
        model.advance(Duration::from_micros(sequence.next(REACH)));
        model.app_mut().ahead = sequence.next(REACH);
        model.set_skew(Duration::from_micros(sequence.next(GAP)));
        let arrivals = model.app().arrivals;
        let frames = model.app().frames;
        let cause = model.turn();
        if model.app().frames > frames {
            last_frame = turn;
        }
        if model.app().arrivals > arrivals {
            if cause == Cause::Arrived {
                waited += 1;
            } else {
                swallowed += 1;
            }
        }
        if !assert_invariant {
            continue;
        }
        let owed = model
            .app()
            .due
            .is_some_and(|deadline| deadline <= model.now());
        assert!(
            !(owed && model.parked() == Parked::Indefinitely && !model.anything_pending()),
            "turn {turn}: the loop parked on nothing with a moment owed, and nothing will ever \
             ask for the frame it wanted"
        );
    }

    Run {
        frames: model.app().frames,
        swallowed,
        waited,
        last_frame,
    }
}

#[test]
fn no_turn_of_a_long_run_ever_parks_on_nothing_with_a_frame_owed() {
    let run = soak(Reading::Shipped, true);
    assert!(
        run.last_frame > TURNS - 16,
        "the last frame ran on turn {} of {TURNS}: the loop stopped drawing and then sat in a \
         state the invariant does not name",
        run.last_frame
    );
    assert!(
        run.frames > u64::from(TURNS) / 8,
        "{} frames over {TURNS} turns: the loop stopped drawing without ever entering the state \
         the invariant names, which means the invariant has stopped describing the failure",
        run.frames
    );
    assert!(
        run.swallowed > 10_000,
        "only {} moments passed before they could be installed; the run is not reaching the \
         boundary it exists to hammer",
        run.swallowed
    );
    assert!(
        run.waited > 10_000,
        "only {} moments were waited for; the run has collapsed into the overdue path and is no \
         longer exercising an ordinary park",
        run.waited
    );
}

#[test]
fn the_same_run_freezes_at_once_when_the_moment_is_dropped() {
    // The soak's positive control. Without it the run above is a long test nobody has seen fail,
    // and the number of turns in it is decoration.
    let run = soak(Reading::Dropping, false);
    assert!(
        run.last_frame < TURNS / 1_000,
        "the reading that drops the moment was still drawing on turn {} of {TURNS}; if it does \
         not freeze almost at once the model has stopped reproducing the loop that froze",
        run.last_frame
    );
    assert_eq!(
        run.swallowed, 0,
        "a moment that was dropped was handed over anyway"
    );
    assert!(
        run.frames < 100,
        "{} frames from a loop that is supposed to have stopped",
        run.frames
    );
}
