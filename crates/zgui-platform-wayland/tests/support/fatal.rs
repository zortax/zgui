//! A run whose connection fails while the window draws, and what the run must return.
//!
//! The compositor ends a connection that broke a rule, and a compositor that goes away ends every
//! connection. Every later request and every later read on it fails the same way. A loop that goes
//! on turning spins on the dead socket and logs the same failure for ever. The run returns the
//! failure instead, promptly, so a supervisor sees the process end and starts it again.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use wayland_client::Connection;
use zgui_platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};
use zgui_runtime::Runtime;

/// How many frames the window drew.
static DRAWN: AtomicU64 = AtomicU64::new(0);

/// How long the window draws before the connection fails.
const DRAWING: Duration = Duration::from_millis(600);

/// How long the run may take from the failure to its end.
const PROMPT: Duration = Duration::from_secs(3);

/// Keeps the window drawing, so the loop is busy when the connection fails.
struct Drive {
    /// The application.
    inner: Runtime,
}

impl AppHandler for Drive {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        let drawn = matches!(event, SurfaceEvent::RedrawRequested);
        self.inner.surface_event(cx, surface, event);
        if drawn {
            DRAWN.fetch_add(1, Ordering::Relaxed);
            if let Some(window) = cx.surface(surface) {
                window.request_redraw();
            }
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
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

/// Runs a drawing window, breaks its connection with `break_it`, and asserts on the end.
///
/// The run must return an error that contains `expected`, within [`PROMPT`] of the break.
pub(crate) fn assert_run_ends(
    property: &'static str,
    break_it: fn(&Connection),
    expected: &'static str,
) {
    if !super::require_nested(property) {
        return;
    }
    super::tracing();
    super::watchdog(property);

    let inner = match super::animated_application("fatal") {
        Ok(runtime) => runtime,
        Err(error) => {
            super::skipped(property, &format!("the runtime would not install: {error}"));
            return;
        }
    };
    let Some(mut app) = super::loop_for(property, Drive { inner }) else {
        return;
    };
    let conn = app.connection().clone();
    let broken = Arc::new(Mutex::new(None::<Instant>));
    let stamp = Arc::clone(&broken);
    std::thread::spawn(move || {
        std::thread::sleep(DRAWING);
        *stamp.lock().unwrap() = Some(Instant::now());
        break_it(&conn);
    });

    let ran = app.run();
    let ended = Instant::now();
    if super::NO_DEVICE.load(Ordering::Relaxed) {
        super::skipped(
            property,
            "this machine has no graphics device to draw through",
        );
        return;
    }
    let Err(error) = ran else {
        panic!("FAILED: {property}: the run ended without an error");
    };
    let broken = broken
        .lock()
        .unwrap()
        .unwrap_or_else(|| panic!("FAILED: {property}: the run ended before the break: {error}"));
    let message = error.to_string();
    assert!(
        message.contains(expected),
        "FAILED: {property}: the error does not say `{expected}`: {message}"
    );
    let waited = ended.duration_since(broken);
    assert!(
        waited < PROMPT,
        "FAILED: {property}: the run took {waited:?} to end after the break"
    );
    let drawn = DRAWN.load(Ordering::Relaxed);
    assert!(
        drawn > 0,
        "FAILED: {property}: the window drew no frame before the break"
    );
    super::passed(
        property,
        &format!("ended {waited:?} after the break, {drawn} frames drawn: {message}"),
    );
}
