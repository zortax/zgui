//! The event loop, and the four things that can wake it.
//!
//! This is the adapter and nothing more. It receives the loop's callbacks, translates each into the
//! contract's vocabulary, and hands the result to the application. It decides one thing on its own
//! account — when to close the deadline edge — and it decides it by asking [`Park`], which is where
//! all the parking arithmetic lives.
//!
//! # The four ways a frame gets asked for
//!
//! The list is exhaustive on purpose. A missing entry is not a crash but a window that quietly
//! stops responding to one whole class of event, and an exhaustive list is what makes a missing one
//! auditable.
//!
//! 1. **The application changed something**, and asked for the frame that shows it.
//! 2. **Input arrived**, and was dispatched into the frame it asked for.
//! 3. **Work finished on another thread.** It reaches a parked loop through the waker, which
//!    interrupts the block, and arrives here as a wake with the surfaces it belongs to — never as a
//!    reason to redraw every window, because an image decoding for one is not a reason to redraw
//!    the other.
//! 4. **A deadline arrived.** This is the one that has to be closed by hand, because a reached
//!    deadline is reported as the *cause of the next turn* and never as a request to draw. Turning
//!    it into a request is what makes a timer fire and an animation advance on an otherwise idle
//!    loop, and it happens in exactly two places.
//!
//!    [`ApplicationHandler::new_events`] is where the loop reports a moment it waited for and
//!    reached. [`ApplicationHandler::about_to_wait`] is where it reports a moment it never got as
//!    far as waiting for: the application names a moment against one reading of the clock and the
//!    loop installs it against a later one, so a moment a few microseconds ahead can pass in
//!    between. Refusing to install such a moment is right, and forgetting it is a loop blocked for
//!    ever with nothing left that will ask for the frame. So it is handed over on the spot, and
//!    the type the install hands back is what makes handing it over the only thing that can be
//!    done with it.

mod drag;
mod events;
mod window;

use std::collections::HashMap;

use winit::application::ApplicationHandler;
use winit::event::StartCause;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;
use zgui_platform::{AppHandler, PlatformError, Surface, SurfaceEvent, WakeReason};

use crate::app::window::WindowState;
use crate::cx::{Shared, WinitCx};
use crate::park::{Park, Parked};
use crate::surface::a11y;
use crate::waker::UserEvent;

/// Opens an event loop this backend can drive.
///
/// Separate from [`run`] so that a caller who has to own the loop — a test that wants to inspect
/// what happened after it finished, a program embedding this in a larger process — can have it.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when there is no windowing system to connect to.
pub fn event_loop() -> Result<EventLoop<UserEvent>, PlatformError> {
    EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|error| PlatformError::Backend(error.to_string()))
}

/// Runs `handler` on a real event loop until the last window closes.
///
/// This is what an application hands to its runtime as the production driver. It blocks until the
/// loop finishes, which is the whole of its contract: a windowing backend does not return early.
///
/// # Errors
///
/// Returns [`PlatformError::Backend`] when there is no windowing system to connect to, or when the
/// loop stopped for a reason of the platform's own.
pub fn run(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let event_loop = event_loop()?;
    let mut app = WinitApp::new(&event_loop, handler);
    zgui_profile::latency::start_epoch();
    event_loop
        .run_app(&mut app)
        .map_err(|error| PlatformError::Backend(error.to_string()))
}

/// An application, as the event loop sees it.
///
/// Generic over the handler rather than boxing it, so that a caller who owns both can still read
/// its application's own state once the loop has finished — which is what a test asserting on how
/// many frames ran needs and what a boxed trait object would take away.
pub struct WinitApp<A: AppHandler> {
    /// Everything that outlives a callback.
    shared: Shared,
    /// The application.
    handler: A,
    /// How the loop parks, and what it owes when a moment passes before it can.
    park: Park,
    /// What each window remembers between events.
    windows: HashMap<WindowId, WindowState>,
    /// Whether the one-time attachment has happened.
    attached: bool,
    /// How many turns of the loop have started.
    turns: u64,
}

impl<A: AppHandler> WinitApp<A> {
    /// An application that will run on `event_loop`.
    pub fn new(event_loop: &EventLoop<UserEvent>, handler: A) -> Self {
        Self {
            shared: Shared::new(event_loop.create_proxy()),
            handler,
            park: Park::new(),
            windows: HashMap::new(),
            attached: false,
            turns: 0,
        }
    }

    /// The application.
    pub const fn handler(&self) -> &A {
        &self.handler
    }

    /// How the loop is parked, and how many deadline arrivals it has reported.
    pub const fn park(&self) -> &Park {
        &self.park
    }

    /// How many turns of the loop have started.
    ///
    /// A loop that is waiting properly takes one turn per thing that happens. One that is spinning
    /// takes as many as the processor allows, so this is what separates the two without measuring
    /// time — which no test can do the same way twice.
    pub const fn turns(&self) -> u64 {
        self.turns
    }

    /// What a park means to the loop.
    ///
    /// An indefinite park is a block, a deadline is a block with a limit, and not parking at all is
    /// a poll. Nothing else in this crate names a control flow, so there is one place where a park
    /// becomes a decision the platform acts on.
    ///
    /// A park this backend has not been taught blocks. Waiting too long shows up as one late frame;
    /// polling by mistake burns a core for ever, which is the failure the whole module exists to
    /// prevent.
    const fn control_flow(parked: Parked) -> ControlFlow {
        match parked {
            Parked::Until(deadline) => ControlFlow::WaitUntil(deadline),
            Parked::Never => ControlFlow::Poll,
            _ => ControlFlow::Wait,
        }
    }

    /// Hands every drag that finished arriving to the window it arrived over.
    ///
    /// This runs at the end of a turn because that is the earliest moment the set of dragged files
    /// is known to be complete: the platform reports them one at a time, within one turn, and says
    /// nothing when it has finished.
    fn flush_drags(&mut self, event_loop: &ActiveEventLoop) {
        let pending: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, state)| state.drag.is_pending())
            .map(|(id, _)| *id)
            .collect();
        for id in pending {
            let Some(surface) = self.shared.by_window(id) else {
                continue;
            };
            let Some(state) = self.windows.get_mut(&id) else {
                continue;
            };
            let events = state.drag.take(state.pointer);
            let cx = WinitCx::new(&self.shared, event_loop);
            for event in events {
                self.handler.surface_event(
                    &cx,
                    Surface::id(surface.as_ref()),
                    SurfaceEvent::Drag(event),
                );
            }
        }
    }
}

impl<A: AppHandler> ApplicationHandler<UserEvent> for WinitApp<A> {
    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.turns += 1;
        zgui_profile::latency::note(
            "loop.wake",
            match cause {
                StartCause::ResumeTimeReached { .. } => "deadline",
                StartCause::WaitCancelled { .. } => "cancelled",
                StartCause::Poll => "poll",
                StartCause::Init => "init",
            },
        );
        if !self.attached {
            self.shared.attach(event_loop);
            self.attached = true;
        }
        match cause {
            StartCause::ResumeTimeReached { .. } => {
                // **The deadline edge.** A reached deadline draws nothing by itself: it is
                // reported here and nowhere else, and a request to draw comes only from something
                // asking for one. Without this, a parked deadline produces no frame at all — the
                // timer never fires, the animation never advances, and the application looks like
                // it has stopped rather than like it has a bug.
                //
                // The deadline is cleared inside `resumed`, before the application is told, so a
                // handler installing a fresh one from within its own callback is not undone by the
                // clearing that would otherwise follow it.
                self.park.resumed();
                let cx = WinitCx::new(&self.shared, event_loop);
                self.handler.deadline_reached(&cx);
            }
            StartCause::WaitCancelled { .. } => {
                // The wait ended before its moment. Whatever was installed is no longer what the
                // loop is waiting on, and the end of this turn computes the park again from
                // scratch.
                self.park.cancel();
            }
            _ => {}
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let cx = WinitCx::new(&self.shared, event_loop);
        self.handler.surfaces_available(&cx);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        let cx = WinitCx::new(&self.shared, event_loop);
        self.handler.surfaces_lost(&cx);
        self.shared.forget_all();
        self.windows.clear();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        let reason = match event {
            UserEvent::Wake(reason) => Some(reason),
            UserEvent::A11y(event) => a11y_wake(&self.shared, event),
        };
        let Some(reason) = reason else { return };
        let cx = WinitCx::new(&self.shared, event_loop);
        self.handler.wake(&cx, reason);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: winit::event::WindowEvent,
    ) {
        zgui_profile::latency::note("evt.in", describe(&event));
        let Some(surface) = self.shared.by_window(window_id) else {
            return;
        };
        // Shown to the accessibility adapter before anything else touches it: the adapter learns
        // where the window is and whether it has focus only from the events it is shown, and one
        // withheld leaves a screen reader's highlight in the place the window used to be.
        a11y::observe(Surface::id(surface.as_ref()), surface.window(), &event);

        let destroyed = matches!(event, winit::event::WindowEvent::Destroyed);
        let timestamp = self.shared.clock().timestamp();
        let state = self.windows.entry(window_id).or_default();
        let translated = events::translate(&surface, state, timestamp, event);

        if let Some(translated) = translated {
            if let SurfaceEvent::ColorSchemeChanged(scheme) = translated {
                self.shared.set_scheme(scheme);
            }
            let cx = WinitCx::new(&self.shared, event_loop);
            self.handler
                .surface_event(&cx, Surface::id(surface.as_ref()), translated);
        }

        if destroyed {
            self.shared.forget(window_id);
            self.windows.remove(&window_id);
        }
        zgui_profile::latency::mark("evt.out");
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        zgui_profile::latency::mark("wait.in");
        self.flush_drags(event_loop);
        // Between turns, with no callback for any of them running: this is where a window the
        // application closed is actually destroyed, and where the input state kept for it goes.
        //
        // Never while the loop is stopping. A window closed on the way out is left to the ordinary
        // teardown, because destroying one and then dropping the display connection in the same
        // breath races the clipboard's own thread — which shares that connection and is still
        // taking its objects down. The user sees no difference: the process is exiting, and every
        // window goes with it either way.
        if !event_loop.exiting() {
            for window in self.shared.retire() {
                self.windows.remove(&window);
            }
        }

        let cx = WinitCx::new(&self.shared, event_loop);
        let policy = self.handler.idle(&cx);
        // Every deadline the loop ever parks on is installed here and nowhere else, and the loop
        // never parks on a decision the application made before a deadline was serviced.
        //
        // The clock is read after the application has decided, and therefore later than the reading
        // it decided against: a moment picked a few microseconds ahead can already be behind this
        // one. Such a moment is paid immediately rather than waited for — but paying it can change
        // what the application wants, and it need not ask for a frame while doing so, so the answer
        // it gave beforehand is stale and cannot be parked on. It is asked again, and the loop
        // settles only once it names something genuinely ahead, or nothing at all.
        let now = self.shared.clock().now();
        let install = self.park.install(policy, now);
        if let Some(deadline) = install.overdue() {
            zgui_profile::latency::note(
                "wait.overdue",
                format!("-{}us", now.saturating_duration_since(deadline).as_micros()),
            );
        }
        let answered = install.overdue().is_some();
        let parked = install.park(|_| {
            // The moment passed between the application naming it and the loop reaching here, so
            // its edge is taken now instead of being waited for. This is the same call the loop
            // would have made on the next turn had the moment been a microsecond further off.
            self.handler.deadline_reached(&cx);
        });
        // A turn that answered such a moment does not park on the answer the application gave
        // before it was told. It hands the turn back: whatever the answer asked for runs, and the
        // next turn asks again on a clock that has moved. Parking instead would settle on a
        // decision already out of date, and blocking on it is how a loop stops for good.
        let parked = if answered { Parked::Never } else { parked };
        event_loop.set_control_flow(Self::control_flow(parked));
        zgui_profile::latency::note(
            "wait.out",
            match parked {
                Parked::Until(deadline) => format!(
                    "until+{}us",
                    deadline
                        .saturating_duration_since(self.shared.clock().now())
                        .as_micros()
                ),
                Parked::Never => "poll".to_owned(),
                _ => "block".to_owned(),
            },
        );
        zgui_profile::latency::flush();
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        let cx = WinitCx::new(&self.shared, event_loop);
        self.handler.shutting_down(&cx);
    }
}

/// A window event's name, short enough to sit in a trace line.
fn describe(event: &winit::event::WindowEvent) -> String {
    use winit::event::WindowEvent as E;
    match event {
        E::RedrawRequested => "redraw".to_owned(),
        E::CursorMoved { position, .. } => format!("moved {},{}", position.x, position.y),
        E::MouseInput { state, .. } => format!("mouse {state:?}"),
        E::Resized(size) => format!("resized {}x{}", size.width, size.height),
        E::ScaleFactorChanged { scale_factor, .. } => format!("scale {scale_factor}"),
        E::CursorEntered { .. } => "entered".to_owned(),
        E::CursorLeft { .. } => "left".to_owned(),
        E::KeyboardInput { .. } => "key".to_owned(),
        other => format!("{:.24}", format!("{other:?}").replace('"', "'")),
    }
}

/// What an accessibility event from the adapter's own connection means to the application.
///
/// A request for the initial tree has to force a build even when nothing is dirty: the tree has
/// never been produced, so there is nothing for a dirty check to notice. A deactivation means
/// nothing has to be built any more, which is not something to be woken for.
fn a11y_wake(shared: &Shared, event: accesskit_winit::Event) -> Option<WakeReason> {
    match event.window_event {
        accesskit_winit::WindowEvent::InitialTreeRequested => shared
            .by_window(event.window_id)
            .map(|surface| WakeReason::A11yTreeRequested(Surface::id(surface.as_ref()))),
        accesskit_winit::WindowEvent::ActionRequested(request) => {
            Some(WakeReason::A11yAction(request))
        }
        accesskit_winit::WindowEvent::AccessibilityDeactivated => None,
    }
}

#[cfg(test)]
mod tests {
    use super::WinitApp;
    use crate::park::Parked;
    use std::time::{Duration, Instant};
    use winit::event_loop::ControlFlow;
    use zgui_platform::{AppHandler, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};

    /// A handler that does nothing, for the parts of the adapter that need one and no more.
    struct Nothing;

    impl AppHandler for Nothing {
        fn surfaces_available(&mut self, _: &dyn PlatformCx) {}
        fn surface_event(&mut self, _: &dyn PlatformCx, _: SurfaceId, _: SurfaceEvent) {}
        fn wake(&mut self, _: &dyn PlatformCx, _: WakeReason) {}
    }

    #[test]
    fn an_indefinite_park_is_a_block_and_never_a_poll() {
        // A poll where a block was meant is a loop that never sleeps, which is the whole failure
        // this backend is written to avoid. The two must never be confusable.
        assert_eq!(
            WinitApp::<Nothing>::control_flow(Parked::Indefinitely),
            ControlFlow::Wait
        );
        assert_eq!(
            WinitApp::<Nothing>::control_flow(Parked::Never),
            ControlFlow::Poll
        );
    }

    #[test]
    fn a_park_with_a_deadline_asks_the_platform_to_wait_until_exactly_that_moment() {
        let moment = Instant::now() + Duration::from_millis(700);
        assert_eq!(
            WinitApp::<Nothing>::control_flow(Parked::Until(moment)),
            ControlFlow::WaitUntil(moment)
        );
    }
}
