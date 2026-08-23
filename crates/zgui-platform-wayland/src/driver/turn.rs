//! One turn of the loop: dispatch, deliver, park.

use std::time::{Duration, Instant};

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use zgui_platform::{
    AppHandler, Clock, IdlePolicy, Install, Park, Parked, PlatformError, Surface as _, SurfaceEvent,
};

use crate::cx::WaylandCx;
use crate::driver::WaylandState;
use crate::waker::PingWaker;

/// Runs `handler` against this machine's compositor until the last surface closes.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when there is no compositor to connect to — which is how a
/// caller learns to fall back to a portable backend — or when the connection failed later.
pub fn run(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut app = WaylandApp::new(handler)?;
    zgui_profile::latency::start_epoch();
    app.run()
}

/// An application, as the loop sees it.
///
/// Generic over the handler rather than boxing it, so that a caller who owns both can read its own
/// state once the loop has finished — which is what a test asserting on how many frames ran needs.
pub struct WaylandApp<A: AppHandler> {
    /// Everything the loop holds.
    state: WaylandState,
    /// The application.
    handler: A,
    /// The loop itself.
    events: EventLoop<'static, WaylandState>,
    /// How the loop parks, and what it owes when a moment passes before it can.
    park: Park,
    /// How many turns have started.
    turns: u64,
}

impl<A: AppHandler> WaylandApp<A> {
    /// Connects to the compositor and prepares to drive `handler`.
    ///
    /// # Errors
    ///
    /// As [`run`].
    pub fn new(handler: A) -> Result<Self, PlatformError> {
        let (conn, globals, queue) = crate::conn::open()?;
        let events: EventLoop<'static, WaylandState> = EventLoop::try_new()
            .map_err(|error| PlatformError::Backend(format!("the event loop: {error}")))?;

        // The only thing that can interrupt a loop asleep on the compositor's socket. It carries
        // both the reasons another thread queues and the redraws a surface asks for off-thread,
        // because either has to end the same park and neither needs a channel of its own.
        let (ping, wakes) = calloop::ping::make_ping()
            .map_err(|error| PlatformError::Backend(format!("the wake pipe: {error}")))?;
        events
            .handle()
            .insert_source(wakes, |(), (), _| {})
            .map_err(|error| PlatformError::Backend(format!("the wake source: {error}")))?;

        let waker = PingWaker::new(ping);
        let state = WaylandState::new(
            conn.clone(),
            &globals,
            queue.handle(),
            events.handle(),
            waker,
        )?;
        // The adapter owns the read, dispatch and flush cycle for the socket. Doing it by hand is
        // how a client goes to sleep holding events it has read and not dispatched, which from
        // outside is a window that has stopped.
        WaylandSource::new(conn, queue)
            .insert(events.handle())
            .map_err(|error| PlatformError::Backend(format!("the wayland source: {error}")))?;

        Ok(Self {
            state,
            handler,
            events,
            park: Park::new(),
            turns: 0,
        })
    }

    /// The application.
    pub const fn handler(&self) -> &A {
        &self.handler
    }

    /// How the loop is parked, and how many deadline arrivals it has reported.
    pub const fn park(&self) -> &Park {
        &self.park
    }

    /// How many turns have started.
    ///
    /// A loop that waits properly takes one turn per thing that happens; one that spins takes as
    /// many as the processor allows. This is what separates the two without measuring time.
    pub const fn turns(&self) -> u64 {
        self.turns
    }

    /// Drives the application until it asks to finish.
    ///
    /// # Errors
    ///
    /// As [`run`].
    pub fn run(&mut self) -> Result<(), PlatformError> {
        {
            let cx = WaylandCx::new(&self.state);
            self.handler.surfaces_available(&cx);
        }
        while !self.state.live.exiting.get() {
            self.turn()?;
        }
        let cx = WaylandCx::new(&self.state);
        self.handler.shutting_down(&cx);
        Ok(())
    }

    /// One turn: everything that happened, then everything that should be drawn, then the park.
    fn turn(&mut self) -> Result<(), PlatformError> {
        self.turns += 1;
        self.state.carry_out_activations();
        self.deliver_wakes();
        self.deliver_events();
        self.retire();
        self.deliver_frames();
        self.park_and_dispatch()
    }

    /// Hands over everything another thread queued since the last turn.
    fn deliver_wakes(&mut self) {
        // A drag's paths are read on a thread and announced here, because the moment they arrive
        // is the first moment the drag can be reported at all.
        self.state.drag_read_finished();
        for reason in self.state.live.waker.drain() {
            let cx = WaylandCx::new(&self.state);
            self.handler.wake(&cx, reason);
        }
    }

    /// Hands over everything the compositor said during the last dispatch.
    fn deliver_events(&mut self) {
        // Taken as a whole rather than drained in place: delivering one may create or close a
        // surface, and both of those record events of their own.
        let events = std::mem::take(&mut self.state.out);
        for (id, event) in events {
            let closing = matches!(event, SurfaceEvent::Destroyed);
            let cx = WaylandCx::new(&self.state);
            self.handler.surface_event(&cx, id, event);
            if closing {
                self.state.live.close(id);
            }
        }
    }

    /// Lets go of the surfaces the application closed, between turns and never inside a callback.
    ///
    /// Never inside one, because the objects underneath are the ones the loop is dispatching for
    /// and taking them down there is a use after free. What actually destroys them is the last
    /// handle going, which is usually this one — the application drops its own on the way out of
    /// the callback that closed the window.
    fn retire(&mut self) {
        drop(self.state.live.retire());
    }

    /// Gives a frame to every surface that asked for one and is allowed one.
    ///
    /// The three steps per surface are one sequence and the third is not optional: ask the
    /// compositor for the next callback, let the application draw, then commit whether or not it
    /// did. A callback rides a commit, and a turn that ends without one ends the chain — after
    /// which the compositor never speaks about that surface again.
    fn deliver_frames(&mut self) {
        let now = self.state.live.clock.now();
        for surface in self.state.live.all() {
            // The pacer is handed the visibility to *write* as well as to read: a frame callback
            // given up on is the evidence that the compositor has stopped drawing this surface,
            // and a run of them is the only occlusion signal that needs no protocol version and no
            // cooperation. So the edge it produces is taken here, before anything is drawn.
            let (due, hidden) = {
                let mut shared = surface.shared();
                if surface.take_request() {
                    shared.pacer.request();
                }
                let mut visibility = shared.visibility;
                let due = shared.pacer.take(&mut visibility, now);
                shared.visibility = visibility;
                (due, shared.visibility_edge())
            };
            if let Some(event) = hidden {
                let cx = WaylandCx::new(&self.state);
                self.handler.surface_event(&cx, surface.id(), event);
            }
            if !due {
                continue;
            }
            let id = surface.id();
            {
                let cx = WaylandCx::new(&self.state);
                self.handler
                    .surface_event(&cx, id, SurfaceEvent::RedrawRequested);
            }
            surface.finish_redraw(self.state.live.clock.now());
        }
    }

    /// Asks the application how to park, installs it, and waits.
    fn park_and_dispatch(&mut self) -> Result<(), PlatformError> {
        let policy = {
            let cx = WaylandCx::new(&self.state);
            let asked = self.handler.idle(&cx);
            self.watchdogs().fold(asked, IdlePolicy::merge)
        };
        let now = self.state.live.clock.now();
        let install = self.park.install(policy, now);
        let answered = install.overdue().is_some();
        let parked = self.deliver_overdue(install);
        // A turn that answered a moment which had already passed does not park on the answer the
        // application gave before it was told. It hands the turn back and asks again on a clock
        // that has moved; parking would settle on a decision already out of date.
        let parked = if answered { Parked::Never } else { parked };
        self.dispatch(parked)
    }

    /// Hands over a deadline that passed between the application naming it and the loop parking.
    fn deliver_overdue(&mut self, install: Install) -> Parked {
        let cx = WaylandCx::new(&self.state);
        install.park(|_| self.handler.deadline_reached(&cx))
    }

    /// Every surface's limit on how long it will wait for a frame callback.
    fn watchdogs(&self) -> impl Iterator<Item = IdlePolicy> {
        let now = self.state.live.clock.now();
        self.state
            .live
            .all()
            .into_iter()
            .filter_map(move |surface| {
                let shared = surface.shared();
                shared.pacer.deadline(shared.visibility, now)
            })
            .map(move |deadline| IdlePolicy::until(deadline, now))
            .collect::<Vec<_>>()
            .into_iter()
    }

    /// Waits as `parked` says, then reports whether the moment it waited for arrived.
    fn dispatch(&mut self, parked: Parked) -> Result<(), PlatformError> {
        let before = self.state.live.clock.now();
        let timeout = match parked {
            Parked::Until(deadline) => Some(deadline.saturating_duration_since(before)),
            Parked::Never => Some(Duration::ZERO),
            _ => None,
        };
        if let Err(error) = self.events.dispatch(timeout, &mut self.state) {
            return Err(self.state.explain(&error));
        }

        // Calloop reports that it woke, never why. The deadline edge is therefore taken from the
        // clock: a moment the loop installed and that has now arrived is reported once, and the
        // install is cleared inside `resumed` before the application is told, so a handler naming
        // a fresh moment from its own callback is not undone by the clearing.
        let reached = self
            .park
            .deadline()
            .is_some_and(|deadline| self.state.live.clock.now() >= deadline);
        if reached {
            self.park.resumed();
            let cx = WaylandCx::new(&self.state);
            self.handler.deadline_reached(&cx);
        } else {
            self.park.cancel();
        }
        Ok(())
    }
}

/// The moment this turn started, for the parts that need one reading rather than several.
#[allow(dead_code)]
fn now() -> Instant {
    Instant::now()
}
