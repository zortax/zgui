//! A model of the loop's turn, so the park can be exercised without a display server.
//!
//! The event loop this backend runs on decides one thing by itself that nothing else can be
//! substituted for: what *caused* the turn that is about to happen. Three of its answers matter
//! here, and one of them is the reason this model exists at all — an installed deadline whose
//! moment has passed is converted to a remaining time on **every** iteration, finds none, and is
//! re-derived as an arrival every time. A loop that installs such a deadline therefore does not
//! wake late; it never sleeps again.
//!
//! So the model reproduces exactly that: the turn's cause is derived from the installed instant
//! and the present moment, on every turn, with no memory of having reported it before. Everything
//! else — the order of the callbacks, the drain of wakes, the frame per surface that asked for
//! one, the recomputation of the park at the end of the turn — mirrors the adapter one for one,
//! and the park it drives is the shipped [`Park`], not a copy of it.
//!
//! The model also reproduces the *gap* the adapter has between asking the application what it
//! wants and installing the answer. The application answers against one reading of the clock and
//! the install happens against a later one, so a moment picked a few microseconds ahead can be in
//! the future for the first reading and in the past for the second. A model without that gap can
//! never express the case, which is why a suite built on one passed while the loop froze.
//!
//! The clock is one a test moves, and the surfaces are buffers, so every assertion is exact and
//! takes a microsecond. What the model cannot prove is that the adapter routes through it; that is
//! what the suite against the real loop is for.

use std::time::{Duration, Instant};

use zgui_platform::{AppHandler, Clock, IdlePolicy, SurfaceEvent};
use zgui_platform_headless::Headless;

use zgui_platform::{Park, Parked};

/// Which reading of the park a model is driving.
///
/// The two defective readings are not decoration. Each is a repair that closes one failure and
/// opens another, and they are here so that every assertion below is one that has been seen to
/// fail: an invariant no defect has ever tripped is an invariant nobody has checked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Reading {
    /// The shipped one.
    #[default]
    Shipped,
    /// Whatever the application asked for is installed, expired or not.
    ///
    /// Closes the stall and opens the spin.
    Unclamped,
    /// A moment that has passed is refused *and forgotten*, and the loop blocks.
    ///
    /// Closes the spin and opens the dropped moment: the loop parks indefinitely while a frame is
    /// owed, and nothing is left that will ever ask for it. This is what shipped.
    Dropping,
}

/// Why a turn of the loop happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Cause {
    /// An installed deadline's moment has arrived.
    Arrived,
    /// Something was waiting: a wake from another thread, or a surface asking to be drawn.
    Awake,
    /// Nothing was waiting and no deadline had arrived, so the loop would have stayed asleep.
    Idle,
}

/// An application driven over a model of the loop's turn.
pub(super) struct Model<A: AppHandler> {
    /// The platform the application sees: a virtual clock, buffers for surfaces, a queue of wakes.
    platform: Headless,
    /// The application.
    app: A,
    /// The shipped park, driven exactly as the adapter drives it.
    park: Park,
    /// What the loop is parked on right now.
    parked: Parked,
    /// Which reading of the park is being driven.
    reading: Reading,
    /// How far the clock moves between the application deciding and the loop installing.
    ///
    /// A real loop reads the clock once to hand to the application and again to install against,
    /// and work happens in between. Without that gap the model cannot express the case where a
    /// moment is in the future for the first reading and in the past for the second — which is the
    /// only case in which the shipped park ever went wrong, and the reason the suite that came
    /// before this passed while the loop froze.
    skew: Duration,
    /// How many frames have run.
    frames: u64,
    /// What caused each turn, in order.
    causes: Vec<Cause>,
}

impl<A: AppHandler> Model<A> {
    /// Starts `app` over a fresh model, telling it that surfaces may now be created.
    pub(super) fn new(app: A) -> Self {
        Self::over(app, Reading::Shipped)
    }

    /// Starts `app` over one of the defective readings, as a positive control.
    pub(super) fn misreading(app: A, reading: Reading) -> Self {
        Self::over(app, reading)
    }

    /// Starts `app` over `reading`.
    fn over(mut app: A, reading: Reading) -> Self {
        let platform = Headless::new();
        app.surfaces_available(&platform);
        let mut model = Self {
            platform,
            app,
            park: Park::new(),
            parked: Parked::Indefinitely,
            reading,
            skew: Duration::ZERO,
            frames: 0,
            causes: Vec::new(),
        };
        model.about_to_wait();
        model
    }

    /// Sets how far the clock moves between the application deciding and the loop installing.
    pub(super) const fn set_skew(&mut self, skew: Duration) {
        self.skew = skew;
    }

    /// The platform, for creating surfaces and asserting on what they recorded.
    pub(super) fn platform(&self) -> &Headless {
        &self.platform
    }

    /// The application.
    pub(super) fn app(&self) -> &A {
        &self.app
    }

    /// The application, mutably, for a script that pokes it directly.
    pub(super) fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }

    /// The present moment.
    pub(super) fn now(&self) -> Instant {
        self.platform.virtual_clock().now()
    }

    /// How many frames have run.
    pub(super) const fn frames(&self) -> u64 {
        self.frames
    }

    /// How many deadline arrivals have been reported.
    pub(super) const fn resumes(&self) -> u64 {
        self.park.resumes()
    }

    /// What the loop is parked on.
    pub(super) const fn parked(&self) -> Parked {
        self.parked
    }

    /// What caused each turn so far.
    pub(super) fn causes(&self) -> &[Cause] {
        &self.causes
    }

    /// Whether anything at all would bring the loop back out of a block.
    ///
    /// A wake from another thread, or a surface that has asked to be drawn. This is the second
    /// half of what decides a turn's cause, exposed on its own because a soak has to ask it
    /// without taking a turn: an indefinite park with this false and work still owed is the stall,
    /// and it is the only state from which nothing further ever happens.
    pub(super) fn anything_pending(&self) -> bool {
        self.platform.has_pending_wakes()
            || self
                .platform
                .offscreens()
                .iter()
                .any(|surface| surface.has_pending_redraw())
    }

    /// Moves the clock, without running a turn.
    pub(super) fn advance(&mut self, by: Duration) {
        self.platform.virtual_clock().advance(by);
    }

    /// Runs one turn of the loop, and reports what caused it.
    ///
    /// The order is the adapter's order, because a difference in it is a difference in behaviour:
    /// the arrival is reported first and is what asks for a frame, the frames run next, and the
    /// park is recomputed last from whatever those left behind.
    pub(super) fn turn(&mut self) -> Cause {
        let cause = self.cause();
        self.causes.push(cause);
        if cause == Cause::Arrived {
            // Cleared inside `resumed`, before the application is told, so a handler installing a
            // fresh deadline from within its own callback is not undone by the clearing.
            self.park.resumed();
            self.app.deadline_reached(&self.platform);
        }
        for reason in self.platform.drain_wakes() {
            self.app.wake(&self.platform, reason);
        }
        for surface in self.platform.offscreens() {
            if surface.take_pending_redraw() {
                self.frames += 1;
                self.app.surface_event(
                    &self.platform,
                    zgui_platform::Surface::id(surface.as_ref()),
                    SurfaceEvent::RedrawRequested,
                );
            }
        }
        self.about_to_wait();
        cause
    }

    /// Runs `turns` turns, advancing the clock by `step` before each.
    pub(super) fn run(&mut self, turns: u32, step: Duration) {
        for _ in 0..turns {
            self.advance(step);
            self.turn();
        }
    }

    /// Asserts that the loop is parking rather than looping.
    ///
    /// One arrival per frame, plus the one that has been reported and whose frame has not run yet,
    /// is the whole budget. Above it, deadlines are being reported reached without producing the
    /// frames they exist to produce, which is a busy loop running no frames at all.
    pub(super) fn assert_parks(&self) {
        assert!(
            self.park.resumes() <= self.frames + 1,
            "{} deadline arrivals against {} frames: the loop is spinning, not waiting",
            self.park.resumes(),
            self.frames
        );
    }

    /// What the loop would report as the cause of the next turn.
    ///
    /// The arrival is derived from the installed instant every time it is asked, with no record of
    /// having answered before. That is the behaviour that turns an expired deadline into a busy
    /// loop, and reproducing it is the whole reason this type exists.
    fn cause(&self) -> Cause {
        if let Parked::Until(deadline) = self.parked
            && self.now() >= deadline
        {
            return Cause::Arrived;
        }
        if self.platform.has_pending_wakes()
            || self
                .platform
                .offscreens()
                .iter()
                .any(|surface| surface.has_pending_redraw())
        {
            return Cause::Awake;
        }
        match self.parked {
            Parked::Never => Cause::Awake,
            _ => Cause::Idle,
        }
    }

    /// Asks the application how to park, moves the clock by the skew, and installs the answer.
    ///
    /// The skew is applied between the two steps and not around them, because that is where the
    /// gap is in the loop: the application answers against one reading of the clock and the install
    /// happens against a later one.
    fn about_to_wait(&mut self) {
        let policy = self.app.idle(&self.platform);
        self.platform.virtual_clock().advance(self.skew);
        let now = self.now();
        self.parked = match self.reading {
            Reading::Shipped => {
                let install = self.park.install(policy, now);
                let Self { app, platform, .. } = self;
                install.park(|_| app.deadline_reached(&*platform))
            }
            // The repair that closes the stall and opens the spin: whatever was asked for is
            // installed, including a moment that has already passed.
            Reading::Unclamped => match policy {
                IdlePolicy::Spin => Parked::Never,
                IdlePolicy::BlockUntil(deadline) => Parked::Until(deadline),
                _ => Parked::Indefinitely,
            },
            // The repair that closes the spin and opens the dropped moment, written out rather
            // than routed through the park: a moment that has passed is refused, and nothing is
            // left holding the frame it asked for. This is the arithmetic that shipped.
            Reading::Dropping => match policy {
                IdlePolicy::Spin => Parked::Never,
                IdlePolicy::BlockUntil(deadline) if deadline > now => Parked::Until(deadline),
                _ => Parked::Indefinitely,
            },
        };
    }
}
