//! The handler that sits between the desktop and the application.
//!
//! It forwards everything, and once the window has drawn its first frames it takes one turn of the
//! loop for itself and runs *one part* of the script in it. Running a part from inside a single
//! turn is what makes that part deterministic: no other event can arrive between two of its steps,
//! because the loop that would deliver one is the loop the part is running on.
//!
//! Handing the loop back between the parts is not tidiness. A window that stays inside one
//! callback stops answering the compositor's liveness ping; the desktop then marks it as not
//! responding and keeps showing the last frame it managed to composite — so the process goes on
//! measuring a live document while every picture taken of it is of a window frozen minutes ago.
//!
//! The window is real throughout. The frames each part asks for are drawn by the machine's
//! graphics device and presented to the compositor, which is why a capture taken between two steps
//! shows what a person would have seen.

use std::time::Instant;

use zgui::platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};

use crate::report::Report;
use crate::stage::{Stage, handles};

/// How many frames to let the window run before the script starts, so that the first layout, the
/// first cascade and the real scale factor have all landed.
const WARMUP: usize = 24;

/// The environment variable that lengthens the warm-up, in turns of the loop.
///
/// A desktop that decides a window's size *after* mapping it — a tiling compositor placing it, a
/// rule resizing it — delivers that decision as a configure the application answers with a fresh
/// layout. A script that starts before it arrives aims every step at a page that is about to
/// reflow, so the run has to be able to wait for the size to be the one it will keep.
const WARM: &str = "ZGUI_PROBE_WARMUP";

/// How many turns to warm up for.
fn warmup() -> usize {
    std::env::var(WARM)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(WARMUP)
}

/// The environment variable that keeps the window open once the script has finished.
///
/// Set it when the question needs the *desktop's own* input in the loop — a wheel detent from a
/// real mouse, a touchpad gesture, a compositor's idea of where the pointer is. None of those can
/// be answered by a synthesised event, and none of them can be asked of a window that has already
/// closed itself.
const HOLD: &str = "ZGUI_PROBE_HOLD";

/// The handler that drives the application, and then forwards for the rest of the run.
pub(crate) struct Probe {
    /// The application.
    inner: Box<dyn AppHandler>,
    /// The surface the window is on, once one has been seen.
    surface: Option<SurfaceId>,
    /// How many frames have gone by before the script.
    warmed: usize,
    /// Which part of the script runs next.
    next: usize,
    /// The parts, in order.
    sections: Vec<crate::script::Section>,
    /// Whether every part has been run.
    ran: bool,
    /// When the run started.
    started: Instant,
    /// Where the findings go.
    report: Report,
}

impl Probe {
    /// Wraps `inner`, reporting into `report`.
    pub(crate) fn new(inner: Box<dyn AppHandler>, report: Report) -> Self {
        Self {
            inner,
            surface: None,
            warmed: 0,
            next: 0,
            sections: crate::script::sections(),
            ran: false,
            started: Instant::now(),
            report,
        }
    }

    /// Runs the script, once, if everything it needs is there.
    fn drive(&mut self, cx: &dyn PlatformCx) {
        if self.ran {
            return;
        }
        let Some(surface) = self.surface else {
            return;
        };
        let Some(handles) = handles::taken() else {
            return;
        };
        self.warmed += 1;
        if self.warmed < warmup() {
            // Real time, not just turns: the window has to be mapped, the output has to report its
            // scale, and the faces have to be found, and none of those are things this loop can
            // hurry along.
            std::thread::sleep(std::time::Duration::from_millis(20));
            return;
        }
        let Some((name, section)) = self.sections.get(self.next).copied() else {
            self.ran = true;
            return;
        };
        self.next += 1;

        let began = Instant::now();
        let mut stage = Stage::new(
            &mut *self.inner,
            cx,
            surface,
            handles,
            self.started,
            &mut self.report,
        );
        section(&mut stage);
        println!("-- {name} took {:.1}s", began.elapsed().as_secs_f32());

        // Written after every part, so a run that is cut short still leaves everything it found.
        if let Err(error) = self.report.write() {
            eprintln!("the report could not be written: {error}");
        }
        if self.next < self.sections.len() {
            return;
        }
        self.ran = true;
        println!("probe: {} broken", self.report.broken());
        if std::env::var_os(HOLD).is_some() {
            println!("probe: holding the window open");
            return;
        }
        // The window has done its job, and it is asked to close the way a person would ask — so
        // the loop unwinds, the views unmount and their cleanups run. Ending the process here
        // instead would leave every one of those undone, and a run that cannot shut down cleanly
        // is a run that cannot say whether shutting down works.
        self.inner
            .surface_event(cx, surface, SurfaceEvent::CloseRequested);
    }
}

impl AppHandler for Probe {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
        self.surface = cx.surfaces().first().map(|surface| surface.id());
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        if self.surface.is_none() {
            self.surface = Some(surface);
        }
        self.inner.surface_event(cx, surface, event);
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        let policy = self.inner.idle(cx);
        self.drive(cx);
        if self.ran {
            policy
        } else {
            // Until the script has run, the loop must keep coming back here rather than parking on
            // a desktop that has nothing more to say.
            IdlePolicy::Spin
        }
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.inner.deadline_reached(cx);
    }

    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        self.inner.shutting_down(cx);
    }
}
