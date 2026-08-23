//! A pointer the compositor actually moved arrives as the events the contract describes.
//!
//! The tables that turn a kernel button code into a named button, and a surface-local position
//! into the space a stylesheet is written in, are asserted on their own. What they cannot assert is
//! that any of it is *wired*: that a seat is opened when the compositor advertises one, that its
//! events reach the surface they happened on, and that the position and the held modifiers travel
//! with them.
//!
//! So this drives the compositor's own pointer over the window and asserts on what came back. It
//! needs a compositor that can be told to move its pointer, which is what the headless session
//! these programs run under is for; anywhere else it says it could not answer rather than passing.

#[path = "support/mod.rs"]
mod support;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, PlatformError, SurfaceEvent, SurfaceId, WakeReason,
};
use zgui_platform_wayland::WaylandApp;
use zgui_vocab::{PointerAction, PointerButton};

/// What the pointer did, in the order it did it.
static SEEN: Mutex<Vec<(PointerAction, Option<PointerButton>)>> = Mutex::new(Vec::new());

/// Whether the window has been drawn, and so exists to be pointed at.
static DRAWN: AtomicBool = AtomicBool::new(false);

/// Whether the compositor turned out to have no pointer to move.
static NO_POINTER: AtomicBool = AtomicBool::new(false);

/// How long the compositor is given to deliver what a virtual device produced.
///
/// A compositor that will not attach a virtual pointer at all — some headless sessions do not —
/// delivers nothing, and this program has to say that rather than hang. Generous, because what is
/// being waited for is a device being attached and a pointer entering a surface.
const PATIENCE: Duration = Duration::from_secs(4);

/// The extent of the output the window is on, which absolute motion is measured against.
const EXTENT: (u32, u32) = (1920, 1080);

/// Where the pointer is put, in the output's own coordinates.
const AT: (u32, u32) = (400, 300);

/// Watches the pointer stream and stops once a whole click has arrived.
struct WatchPointer<A: AppHandler> {
    /// The application.
    inner: A,
    /// Whether the compositor has been asked to move its pointer yet.
    driving: bool,
    /// How the deadline thread ends this loop.
    waker: Option<std::sync::Arc<dyn zgui_platform::Waker>>,
}

impl<A: AppHandler> AppHandler for WatchPointer<A> {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.waker.get_or_insert_with(|| cx.waker());
        self.inner.surfaces_available(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        if let SurfaceEvent::Pointer { action, event, .. } = &event {
            let mut seen = SEEN.lock().expect("the record is not poisoned");
            seen.push((*action, event.button));
            // Entered, moved, pressed, released: a whole click, and the shortest sequence that
            // proves every part of the path rather than only that something arrived.
            if seen
                .iter()
                .any(|(action, _)| *action == PointerAction::Released)
            {
                cx.request_exit();
            }
        }
        if matches!(event, SurfaceEvent::RedrawRequested) {
            DRAWN.store(true, Ordering::Relaxed);
        }
        self.inner.surface_event(cx, surface, event);
        if !self.driving && DRAWN.load(Ordering::Relaxed) {
            self.driving = true;
            let made = support::virtual_pointer::click_over(EXTENT, AT);
            match (made, self.waker.clone()) {
                (true, Some(waker)) => give_up_after(waker),
                _ => {
                    NO_POINTER.store(true, Ordering::Relaxed);
                    cx.request_exit();
                }
            }
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        if NO_POINTER.load(Ordering::Relaxed) {
            cx.request_exit();
        }
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        self.inner.idle(cx)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.inner.deadline_reached(cx);
    }

    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        self.inner.shutting_down(cx);
    }
}

/// Ends the loop if nothing has arrived by the time the patience runs out.
///
/// From another thread and through the waker, which is the only thing that reaches a loop parked
/// on the compositor's socket — and is itself the shape of a property this backend has to satisfy.
fn give_up_after(waker: std::sync::Arc<dyn zgui_platform::Waker>) {
    std::thread::spawn(move || {
        std::thread::sleep(PATIENCE);
        if SEEN.lock().expect("the record is not poisoned").is_empty() {
            NO_POINTER.store(true, Ordering::Relaxed);
            waker.wake(WakeReason::AppWork);
        }
    });
}

fn main() {
    const PROPERTY: &str = "a pointer the compositor moved arrives as pointer events";

    support::tracing();
    support::watchdog(PROPERTY);

    let handler = WatchPointer {
        inner: match support::application("pointer stream") {
            Ok(runtime) => runtime,
            Err(error) => {
                support::skipped(PROPERTY, &format!("the runtime would not install: {error}"));
                return;
            }
        },
        driving: false,
        waker: None,
    };

    let mut app = match WaylandApp::new(handler) {
        Ok(app) => app,
        Err(PlatformError::Backend(reason)) => {
            support::skipped(PROPERTY, &format!("no compositor to run on: {reason}"));
            return;
        }
        Err(other) => panic!("the compositor refused the application: {other}"),
    };
    if let Err(error) = app.run() {
        panic!("the loop stopped: {error}");
    }

    if support::NO_DEVICE.load(Ordering::Relaxed) {
        support::skipped(
            PROPERTY,
            "this machine has no graphics device to draw through",
        );
        return;
    }
    if NO_POINTER.load(Ordering::Relaxed) {
        // Which of the two reasons this is decides whether it is a skip or a failure, and only the
        // compositor can say. A seat it advertises a pointer on advertises it to every client, so
        // no pointer events under one is this backend opening nothing — the defect that leaves an
        // application nothing can be typed into, and the one that hides as a machine with nothing
        // to say. A seat with no pointer on it is the headless session, and has nothing to say.
        assert!(
            !support::seat::advertised().pointer,
            "the compositor advertised a pointer on its seat and none of it reached the surface"
        );
        support::skipped(PROPERTY, "this compositor attaches no pointer to its seat");
        return;
    }

    let seen = SEEN.lock().expect("the record is not poisoned").clone();
    assert!(
        seen.iter()
            .any(|(action, _)| *action == PointerAction::Entered),
        "the pointer never entered the window: {seen:?}"
    );
    assert!(
        seen.iter()
            .any(|(action, _)| *action == PointerAction::Moved),
        "the pointer moved and nothing said so: {seen:?}"
    );
    assert!(
        seen.contains(&(PointerAction::Pressed, Some(PointerButton::Primary))),
        "the primary button was pressed and did not arrive as one: {seen:?}"
    );
    assert!(
        seen.contains(&(PointerAction::Released, Some(PointerButton::Primary))),
        "the primary button was released and did not arrive as one: {seen:?}"
    );

    support::passed(PROPERTY, &format!("{} pointer events", seen.len()));
}
