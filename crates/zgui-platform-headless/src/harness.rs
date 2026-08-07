//! Driving an application against the headless platform, one turn at a time.
//!
//! This is the loop, with everything a real one does about parking and nothing it does about
//! blocking. A turn delivers whatever is queued, gives a frame to every surface that asked for
//! one, and then asks the application how to park. Moving the clock past an installed deadline
//! takes the same edge a windowing backend takes when the deadline arrives: the loop reports it
//! reached, the application turns that into a request to draw, and the expired deadline is never
//! re-installed.
//!
//! Both halves of that sentence are load-bearing, and leaving either out looks like nothing
//! happening. Without the report, a timer fires no frame. Without the clearing, the deadline is
//! reported reached again on the next turn and on every turn after it, and the loop spins while
//! running no frames at all.

use std::time::{Duration, Instant};

use zgui_platform::{AppHandler, Clock, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId};

use crate::platform::Headless;

/// An application driven against the headless platform.
///
/// ```
/// use std::time::Duration;
/// use zgui_platform::{AppHandler, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId,
///     WakeReason};
/// use zgui_platform_headless::Harness;
///
/// /// An application that never asks for anything.
/// struct Idle;
///
/// impl AppHandler for Idle {
///     fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
///         cx.create_surface(&SurfaceAttributes::new("idle")).expect("headless");
///     }
///     fn surface_event(&mut self, _: &dyn PlatformCx, _: SurfaceId, _: SurfaceEvent) {}
///     fn wake(&mut self, _: &dyn PlatformCx, _: WakeReason) {}
/// }
///
/// let mut harness = Harness::new(Idle);
/// harness.run_for(Duration::from_secs(10), Duration::from_millis(16));
/// assert_eq!(harness.frames_requested(), 0, "nothing changed, so nothing was drawn");
/// assert!(harness.parked_deadline().is_none());
/// ```
pub struct Harness<A: AppHandler> {
    /// The platform the application is driven against.
    platform: Headless,
    /// The application.
    app: A,
    /// How the loop is parked right now.
    policy: IdlePolicy,
    /// How many deadlines have been reported reached.
    resumes: u64,
    /// What the surfaces had asked for in total when the counts were last reset.
    ///
    /// The redraw count is a difference rather than a tally, so that it counts every call that
    /// reached [`Surface::request_redraw`](zgui_platform::Surface::request_redraw) — including the
    /// one a frame's last phase makes and the one the park makes for a deadline that has already
    /// passed. A tally kept by hand counts only the places someone remembered to add to it, which
    /// is how a request that is never made and a request nobody counted come to look alike.
    redraw_baseline: u64,
    /// How many of those have been turned into frames.
    frames: u64,
    /// Whether delivering a resize is forbidden from moving the clock on its own.
    clock_held: bool,
    /// Whether a configure also marks the surface as needing a redraw, on the backend's account.
    redraw_on_configure: bool,
}

impl<A: AppHandler> Harness<A> {
    /// Starts `app` against a fresh platform, telling it that surfaces may now be created.
    pub fn new(app: A) -> Self {
        Self::over(Headless::new(), app)
    }

    /// The same, against a platform that has already been configured.
    pub fn over(platform: Headless, mut app: A) -> Self {
        app.surfaces_available(&platform);
        let mut harness = Self {
            platform,
            app,
            policy: IdlePolicy::Block,
            resumes: 0,
            redraw_baseline: 0,
            frames: 0,
            clock_held: false,
            redraw_on_configure: false,
        };
        harness.park();
        harness
    }

    /// Forbids [`Harness::deliver`] from moving the clock to a deadline a resize installed.
    ///
    /// Delivering a resize crosses that deadline by default, because otherwise a virtual clock
    /// never reaches it and a test that resizes and then asserts on the result asserts on the size
    /// before the resize. A test that is measuring *how many* frames a sequence of configures costs
    /// has to own the clock itself, and this is how it says so.
    pub const fn hold_clock(&mut self, held: bool) {
        self.clock_held = held;
    }

    /// Makes every configure also mark the surface as needing a redraw, as a real backend does.
    ///
    /// A windowing backend does not wait to be asked. A compositor that resizes a window sets that
    /// window's redraw flag itself — winit's Wayland loop does it on the same turn it reports the
    /// new size — so an application is handed a frame for every configure whether or not it wanted
    /// one, and declining the ones it did not want is the only thing that keeps the work down.
    ///
    /// Off by default, because a test that scripts events without one is scripting the application
    /// and not the compositor. Turning it on is how a test asks the question a drag actually poses:
    /// not "did the window ask for fewer frames" but "did it *run* fewer".
    pub const fn redraw_on_configure(&mut self, redraw: bool) {
        self.redraw_on_configure = redraw;
    }

    /// The platform, for creating surfaces, reading the clipboard, or asserting on a surface.
    pub fn platform(&self) -> &Headless {
        &self.platform
    }

    /// The application.
    pub fn app(&self) -> &A {
        &self.app
    }

    /// The application, mutably, for a script that pokes it directly.
    pub fn app_mut(&mut self) -> &mut A {
        &mut self.app
    }

    /// The present moment.
    pub fn now(&self) -> Instant {
        self.platform.virtual_clock().now()
    }

    /// The moment the loop is parked until, if it is parked on a deadline at all.
    pub fn parked_deadline(&self) -> Option<Instant> {
        self.policy.deadline()
    }

    /// How the loop is parked.
    pub fn idle_policy(&self) -> IdlePolicy {
        self.policy
    }

    /// How many redraw requests have reached a surface since the counts were last reset.
    ///
    /// Every one of them, wherever it came from: a wake delivered from another thread, a deadline
    /// reported reached, an event, or the frame's own last phase.
    pub fn redraws_requested(&self) -> u64 {
        self.total_redraws() - self.redraw_baseline
    }

    /// How many of those have been turned into frames.
    ///
    /// The two differ by whatever is still pending, and the difference is where a loop that asks
    /// for four frames per wake shows up: coalescing makes the first number four and the second
    /// one, and only the second is what the machine actually did.
    pub fn frames_requested(&self) -> u64 {
        self.frames
    }

    /// How many times an installed deadline has been reported reached.
    pub fn resumes(&self) -> u64 {
        self.resumes
    }

    /// Delivers one event to one surface, exactly as the platform would.
    ///
    /// This is the scripted-input half of the backend: a pointer move, a key, a resize or an
    /// occlusion arrives here rather than from a person.
    ///
    /// An event that says the surface moved is applied to the surface before it is delivered,
    /// because that is the order a window system does it in: the extent a window reports has
    /// already changed by the time the notification arrives. Delivering the notification alone
    /// would leave the surface saying one thing and the event saying another, and a test written
    /// against that pair proves nothing about either.
    pub fn deliver(&mut self, surface: SurfaceId, event: SurfaceEvent) {
        let resized = self.apply_to_surface(surface, &event);
        self.app.surface_event(&self.platform, surface, event);
        self.drain_wakes();
        self.park();
        if resized {
            self.wait_out_the_resize();
        }
    }

    /// Moves the surface to where `event` says it already is, and reports whether it was a configure.
    ///
    /// The order is the order a window system does it in: the extent a window reports has already
    /// changed by the time the notification arrives. Delivering the notification alone would leave
    /// the surface saying one thing and the event saying another, and a test written against that
    /// pair proves nothing about either.
    fn apply_to_surface(&self, surface: SurfaceId, event: &SurfaceEvent) -> bool {
        let configure = matches!(
            event,
            SurfaceEvent::Resized(_) | SurfaceEvent::ScaleFactorChanged { .. }
        );
        let Some(offscreen) = self.platform.offscreen(surface) else {
            return configure;
        };
        match event {
            SurfaceEvent::Resized(size) => offscreen.resize(*size),
            SurfaceEvent::ScaleFactorChanged { scale_factor, size } => {
                offscreen.set_scale_factor(*scale_factor);
                offscreen.resize(*size);
            }
            _ => {}
        }
        if configure && self.redraw_on_configure {
            zgui_platform::Surface::request_redraw(offscreen.as_ref());
        }
        configure
    }

    /// Crosses the deadline a configure that was answered with no frame installed.
    ///
    /// A size is a level rather than an event, so an application is entitled to answer a configure
    /// with a deadline instead of a frame — once per frame of the output rather than once per
    /// configure, because the ones in between cannot be seen. On a real loop that is a block of a
    /// few milliseconds. On a virtual clock it is for ever, because nothing moves the clock but a
    /// test, and a test that asserts on the layout after a resize would be asserting on the size
    /// before it.
    ///
    /// Scoped to a delivered resize and to a deadline inside one frame of the output, so nothing
    /// that does not resize can reach it: a document animating or waiting on a timer parks on its
    /// own deadlines, and those are still the test's to cross.
    fn wait_out_the_resize(&mut self) {
        if self.clock_held || self.has_pending_work() {
            return;
        }
        let Some(deadline) = self.policy.deadline() else {
            return;
        };
        let wait = deadline.saturating_duration_since(self.now());
        if wait <= zgui_platform::refresh_interval(None) {
            self.advance(wait);
        }
    }

    /// Delivers several events to one surface inside a single turn.
    ///
    /// A window system does not deliver one event per turn. A drag hands over every configure that
    /// arrived while the last frame was being drawn, all of them before the loop is asked how to
    /// park and long before any of them is drawn — so a test that delivers them one at a time is
    /// testing a stream nothing produces. This is what a burst actually looks like: several events,
    /// one park, one chance to draw.
    pub fn deliver_all(
        &mut self,
        surface: SurfaceId,
        events: impl IntoIterator<Item = SurfaceEvent>,
    ) {
        let mut resized = false;
        for event in events {
            resized |= self.apply_to_surface(surface, &event);
            self.app.surface_event(&self.platform, surface, event);
        }
        self.drain_wakes();
        self.park();
        if resized {
            self.wait_out_the_resize();
        }
    }

    /// Delivers one event to the first surface the application created.
    ///
    /// # Panics
    ///
    /// Panics when no surface exists, because an event delivered to nothing is a test that is
    /// asserting on an application that was never started.
    pub fn deliver_to_first(&mut self, event: SurfaceEvent) {
        let surface = self
            .platform
            .offscreens()
            .first()
            .map(|surface| zgui_platform::Surface::id(surface.as_ref()))
            .expect("the application has created a surface");
        self.deliver(surface, event);
    }

    /// Runs one turn of the loop: deliver what is queued, draw what asked to be drawn, then park.
    ///
    /// Returns how many frames this turn ran, which is zero for an application with nothing to do.
    pub fn pump(&mut self) -> u64 {
        self.drain_wakes();
        let mut frames = 0;
        for surface in self.platform.offscreens() {
            if surface.take_pending_redraw() {
                frames += 1;
                self.frames += 1;
                self.app.surface_event(
                    &self.platform,
                    zgui_platform::Surface::id(surface.as_ref()),
                    SurfaceEvent::RedrawRequested,
                );
                // A frame may itself have queued a wake, and the wake has to be delivered before
                // the park is computed or the loop parks over work it already has.
                self.drain_wakes();
            }
        }
        self.park();
        frames
    }

    /// Pumps until nothing more is pending, up to `turns`.
    ///
    /// Returns how many frames ran in total.
    ///
    /// # Panics
    ///
    /// Panics when `turns` is exhausted with work still pending, because a loop that never settles
    /// is the failure this backend exists to make visible rather than a slow test.
    pub fn settle(&mut self, turns: u32) -> u64 {
        let mut frames = 0;
        for _ in 0..turns {
            let ran = self.pump();
            frames += ran;
            if ran == 0 && !self.has_pending_work() {
                return frames;
            }
        }
        panic!(
            "the loop still had work pending after {turns} turns; it ran {frames} frames and is \
             parked on {:?}",
            self.policy
        );
    }

    /// Moves the clock, taking the deadline-expiry edge if that crosses the parked deadline.
    ///
    /// The edge is the whole point. It counts the resume, tells the application its deadline was
    /// reached — which is what asks the surfaces concerned to draw — and parks with no deadline,
    /// so the expired one is never reported twice.
    pub fn advance(&mut self, by: Duration) {
        self.platform.virtual_clock().advance(by);
        self.expire();
        self.assert_park_invariant();
    }

    /// Advances in `step`-sized increments until `total` has elapsed, pumping between each.
    ///
    /// This is how an idle application is checked to be idle: a real loop wakes on nothing, and an
    /// application that produces frames anyway produces them here too.
    pub fn run_for(&mut self, total: Duration, step: Duration) -> u64 {
        assert!(!step.is_zero(), "a zero step would never reach the total");
        let mut frames = self.pump();
        let mut elapsed = Duration::ZERO;
        while elapsed < total {
            self.advance(step);
            frames += self.pump();
            elapsed += step;
        }
        frames
    }

    /// Asks the application to finish, and reports that it is shutting down.
    pub fn shut_down(&mut self) {
        self.platform.request_exit();
        self.app.shutting_down(&self.platform);
    }

    /// Takes every surface away, as a platform that suspends an application does.
    ///
    /// Not a close: what the application asked for is untouched, and [`Harness::resume`] is what
    /// gives it surfaces again. The pair exists here because a test cannot make these calls itself
    /// — the platform and the application are both held by the harness, and only it can hand one
    /// to the other.
    pub fn suspend(&mut self) {
        self.app.surfaces_lost(&self.platform);
    }

    /// Gives the application surfaces again, as a platform that resumes one does.
    pub fn resume(&mut self) {
        self.app.surfaces_available(&self.platform);
        self.park();
    }

    /// Sets the loop's own counts back to zero, leaving the park itself alone.
    pub fn reset_counts(&mut self) {
        self.resumes = 0;
        self.redraw_baseline = self.total_redraws();
        self.frames = 0;
    }

    /// Asserts that the loop is parking rather than spinning.
    ///
    /// One resume per frame, plus the one that has been reported but whose frame has not run yet,
    /// is the whole budget. Anything above it means deadlines are being reported reached without
    /// producing the frames they exist to produce — a loop that looks idle, ignores its own
    /// timers, and burns a core.
    ///
    /// # Panics
    ///
    /// Panics when more deadlines have been reported reached than frames have run, plus one.
    pub fn assert_park_invariant(&self) {
        assert!(
            self.resumes <= self.frames + 1,
            "the loop reported {} expired deadlines against {} frames. A deadline reported reached \
             that produces no frame is not a stall: it is a busy loop running no frames at all, \
             and this ratio is the only thing that separates it from a correct park.",
            self.resumes,
            self.frames
        );
    }

    /// Whether anything is waiting: a queued wake, or a surface that asked to be drawn.
    fn has_pending_work(&self) -> bool {
        self.platform.has_pending_wakes()
            || self
                .platform
                .offscreens()
                .iter()
                .any(|surface| surface.has_pending_redraw())
    }

    /// Delivers every queued wake, and counts what the deliveries asked for.
    fn drain_wakes(&mut self) {
        loop {
            let wakes = self.platform.drain_wakes();
            if wakes.is_empty() {
                return;
            }
            for reason in wakes {
                self.app.wake(&self.platform, reason);
            }
        }
    }

    /// Reports the parked deadline reached, if the clock has passed it.
    fn expire(&mut self) {
        let Some(deadline) = self.policy.deadline() else {
            return;
        };
        if self.now() < deadline {
            return;
        }
        self.resumes += 1;
        // Cleared *before* the application is told, so a handler that installs a fresh deadline
        // from inside the callback is not overwritten by the clearing that follows it.
        self.policy = IdlePolicy::Block;
        self.app.deadline_reached(&self.platform);
        self.park();
    }

    /// Asks the application how to park, and installs the answer.
    ///
    /// A deadline that is not strictly in the future is never installed. The platform recomputes
    /// the time remaining on every turn, finds none, and reports the deadline reached again — for
    /// ever. [`IdlePolicy::until`] is where that rule lives; this only has to route through it.
    fn park(&mut self) {
        let policy = self.app.idle(&self.platform);
        let now = self.now();
        self.policy = match policy {
            IdlePolicy::BlockUntil(deadline) => IdlePolicy::until(deadline, now),
            other => other,
        };
    }

    /// How many redraws every surface has asked for in total.
    fn total_redraws(&self) -> u64 {
        self.platform
            .offscreens()
            .iter()
            .map(|surface| surface.redraws_requested())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::Harness;
    use std::time::Duration;
    use zgui_platform::{
        AppHandler, IdlePolicy, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
    };

    /// An application that asks to be woken once and draws when it is.
    #[derive(Default)]
    struct Delayed {
        /// When it wants to be woken, if it does.
        deadline: Option<std::time::Instant>,
        /// How many frames it has drawn.
        frames: u32,
        /// How many times it was told a deadline was reached.
        reached: u32,
    }

    impl AppHandler for Delayed {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            cx.create_surface(&SurfaceAttributes::new("delayed"))
                .expect("headless surfaces are always creatable");
        }

        fn surface_event(&mut self, _cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
            if matches!(event, SurfaceEvent::RedrawRequested) {
                self.frames += 1;
                self.deadline = None;
            }
        }

        fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {}

        fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
            self.deadline.map_or(IdlePolicy::Block, |at| {
                IdlePolicy::until(at, cx.clock().now())
            })
        }

        fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
            self.reached += 1;
            for surface in cx.surfaces() {
                surface.request_redraw();
            }
        }
    }

    /// A harness whose application wants a frame `after` from now.
    fn delayed(after: Duration) -> Harness<Delayed> {
        let mut harness = Harness::new(Delayed::default());
        let at = harness.now() + after;
        harness.app_mut().deadline = Some(at);
        harness.pump();
        harness
    }

    #[test]
    fn a_seven_hundred_millisecond_delay_costs_one_wake_and_one_frame() {
        let mut harness = delayed(Duration::from_millis(700));
        assert!(harness.parked_deadline().is_some());
        harness.reset_counts();

        harness.advance(Duration::from_millis(700));
        assert_eq!(
            harness.redraws_requested(),
            1,
            "the reached deadline itself is what asked for the frame"
        );
        assert_eq!(harness.pump(), 1);
        assert_eq!(harness.frames_requested(), 1);
        assert!(
            harness.parked_deadline().is_none(),
            "an expired deadline is never re-installed"
        );
        assert_eq!(harness.app().frames, 1);
    }

    #[test]
    fn an_expired_deadline_is_reported_once_however_long_the_clock_runs_on() {
        let mut harness = delayed(Duration::from_millis(700));
        harness.reset_counts();
        harness.advance(Duration::from_millis(700));
        assert_eq!(harness.resumes(), 1);
        for _ in 0..100 {
            harness.advance(Duration::from_secs(1));
        }
        assert_eq!(
            harness.resumes(),
            1,
            "the deadline was reported reached again after it had already been serviced"
        );
    }

    #[test]
    fn a_deadline_that_has_already_passed_is_never_installed() {
        let mut harness = Harness::new(Delayed::default());
        let past = harness.now() - Duration::from_millis(1);
        harness.app_mut().deadline = Some(past);
        harness.pump();
        assert_eq!(harness.idle_policy(), IdlePolicy::Block);
        assert!(harness.parked_deadline().is_none());
    }

    #[test]
    fn nothing_to_do_means_no_frames_at_all() {
        let mut harness = Harness::new(Delayed::default());
        let frames = harness.run_for(Duration::from_secs(10), Duration::from_millis(16));
        assert_eq!(frames, 0);
        assert_eq!(harness.resumes(), 0);
        harness.assert_park_invariant();
    }

    #[test]
    #[should_panic(expected = "expired deadlines against")]
    fn the_park_invariant_fails_on_a_loop_that_never_draws() {
        // The positive control. Without it, `resumes <= frames + 1` is an assertion nobody has ever
        // seen fail — and an assertion nobody has seen fail is an assertion nobody has checked.
        struct NeverDraws {
            deadline: Option<std::time::Instant>,
        }

        impl AppHandler for NeverDraws {
            fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
                cx.create_surface(&SurfaceAttributes::new("stuck"))
                    .expect("creatable");
            }
            fn surface_event(&mut self, _: &dyn PlatformCx, _: SurfaceId, _: SurfaceEvent) {}
            fn wake(&mut self, _: &dyn PlatformCx, _: WakeReason) {}
            fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
                // Re-installs the same deadline for ever and never asks for a frame: the shape of
                // the loop with the wake edge missing.
                self.deadline.map_or(IdlePolicy::Block, |at| {
                    IdlePolicy::until(at, cx.clock().now())
                })
            }
            fn deadline_reached(&mut self, _: &dyn PlatformCx) {}
        }

        let mut harness = Harness::new(NeverDraws { deadline: None });
        let at = harness.now() + Duration::from_millis(1);
        harness.app_mut().deadline = Some(at);
        harness.pump();
        for _ in 0..4 {
            harness.app_mut().deadline = Some(harness.now() + Duration::from_millis(1));
            harness.pump();
            harness.advance(Duration::from_millis(1));
        }
    }
}
