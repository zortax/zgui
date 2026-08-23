//! A surface the compositor stops showing says so, and the loop keeps running.
//!
//! This is the defect the backend exists to remove, and it has two halves.
//!
//! **A hidden surface must say it is hidden.** A portable backend reports nothing here, so an
//! animation behind a window on another workspace runs the whole pipeline for ever against pixels
//! nobody can see. Everything above the contract already knows what to do with the report; what it
//! has never had is the report.
//!
//! **A hidden surface must not stall the loop.** Presentation that waits for the display waits on
//! the compositor's frame callbacks, and a surface it has stopped drawing receives none — so the
//! acquisition blocks the thread that also reads input until the driver gives up, a second at a
//! time. Here presentation never waits, so the loop stays responsive: this program proves that by
//! continuing to be answered while the window is hidden, and by coming back when it is shown.

#[path = "support/mod.rs"]
mod support;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, PlatformError, SurfaceEvent, SurfaceId, WakeReason,
};
use zgui_platform_wayland::WaylandApp;

/// Every visibility edge the compositor produced, in order.
static EDGES: Mutex<Vec<bool>> = Mutex::new(Vec::new());

/// Whether the window has been drawn, and so exists to be hidden.
static DRAWN: AtomicBool = AtomicBool::new(false);

/// Whether the compositor turned out not to hide the window at all.
static NEVER_HID: AtomicBool = AtomicBool::new(false);

/// How long the compositor is given to hide the window and show it again.
const PATIENCE: Duration = Duration::from_secs(6);

/// Watches the visibility edges and stops once the window has been hidden and shown again.
struct WatchVisibility<A: AppHandler> {
    /// The application.
    inner: A,
    /// Whether the compositor has been asked to hide the window yet.
    driving: bool,
    /// How the deadline thread ends this loop.
    waker: Option<std::sync::Arc<dyn zgui_platform::Waker>>,
}

impl<A: AppHandler> AppHandler for WatchVisibility<A> {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.waker.get_or_insert_with(|| cx.waker());
        self.inner.surfaces_available(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        let drawn = matches!(event, SurfaceEvent::RedrawRequested);
        match &event {
            SurfaceEvent::RedrawRequested => DRAWN.store(true, Ordering::Relaxed),
            SurfaceEvent::Occluded(hidden) => {
                let mut edges = EDGES.lock().expect("the record is not poisoned");
                edges.push(*hidden);
                // Shown, hidden, shown again: the first is the surface being accepted, and the
                // pair after it is the round trip this asserts on.
                if edges.len() >= 3 {
                    cx.request_exit();
                }
            }
            _ => {}
        }
        self.inner.surface_event(cx, surface, event);
        // The document is static, so each frame asks for the next: a loop that stalled while the
        // window was hidden would stop being answered, which is what the patience below catches.
        if drawn && let Some(surface) = cx.surface(surface) {
            surface.request_redraw();
        }
        if !self.driving && DRAWN.load(Ordering::Relaxed) {
            self.driving = true;
            match self.waker.clone() {
                Some(waker) => {
                    hide_and_show();
                    give_up_after(waker);
                }
                None => cx.request_exit(),
            }
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        if NEVER_HID.load(Ordering::Relaxed) {
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

/// Puts the window away and takes it out again.
///
/// Two compositor commands and nothing this backend can do for itself: what is being asserted is
/// what the compositor says when it stops showing a surface, so the compositor has to be the one
/// that stops showing it.
///
/// Put away rather than moved to another workspace, because a workspace with nothing left on it is
/// destroyed and the view follows the window there — so the window is never actually hidden.
fn hide_and_show() {
    std::thread::spawn(|| {
        let selector = format!("[app_id=\"{}\"]", support::APP_ID);
        std::thread::sleep(Duration::from_millis(400));
        support::ask_compositor(
            &format!("{selector} move container to scratchpad"),
            &support::hyprland_move(support::APP_ID, "98"),
        );
        std::thread::sleep(Duration::from_millis(1200));
        support::ask_compositor(
            &format!("{selector} scratchpad show"),
            &support::hyprland_move(support::APP_ID, "10"),
        );
    });
}

/// Ends the loop if the window was never hidden by the time the patience runs out.
fn give_up_after(waker: std::sync::Arc<dyn zgui_platform::Waker>) {
    std::thread::spawn(move || {
        let began = Instant::now();
        while began.elapsed() < PATIENCE {
            std::thread::sleep(Duration::from_millis(200));
            if EDGES.lock().expect("the record is not poisoned").len() >= 3 {
                return;
            }
        }
        NEVER_HID.store(true, Ordering::Relaxed);
        waker.wake(WakeReason::AppWork);
    });
}

fn main() {
    const PROPERTY: &str = "a surface the compositor stops showing says so, and the loop runs on";

    support::tracing();
    support::watchdog(PROPERTY);

    let handler = WatchVisibility {
        inner: match support::animated_application("occlusion") {
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

    let edges = EDGES.lock().expect("the record is not poisoned").clone();
    if NEVER_HID.load(Ordering::Relaxed) {
        support::skipped(
            PROPERTY,
            &format!("this compositor never stopped showing the window: {edges:?}"),
        );
        return;
    }

    assert_eq!(
        edges,
        [false, true, false],
        "the window was accepted, hidden and shown again; the reports do not say so"
    );
    support::passed(
        PROPERTY,
        "hidden and shown again, with the loop still answering",
    );
}
