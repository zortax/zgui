//! Whether a window that asks to be woken very soon is ever woken.
//!
//! The loop parks on the earliest deadline anything asked for, and what it does with that moment is
//! the platform's business. This asks for one a stated distance away, over and over, and counts the
//! wakes against the asks. A window whose deadlines are honoured answers one for one; a window that
//! stops answering has stopped, and everything that depends on a timer in it — every animation,
//! every `set_timeout`, every deadline a component armed against something failing to arrive — has
//! stopped with it.
//!
//! ```text
//! park-probe <app-id> <micros> [seconds]
//! ```

#![forbid(unsafe_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use zgui::platform::{AppHandler, IdlePolicy, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};
use zgui::view;

/// The application, asked to park a fixed distance ahead every turn.
struct Probing {
    /// The real application, so the window is a real window.
    inner: Box<dyn AppHandler>,
    /// How far ahead each park is asked for.
    ahead: Duration,
    /// How many parks have been asked for.
    asked: Arc<AtomicU64>,
    /// How many arrivals have been reported.
    woken: Arc<AtomicU64>,
    /// When the last arrival was reported.
    last: Arc<std::sync::Mutex<Instant>>,
}

impl AppHandler for Probing {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        self.inner.surface_event(cx, surface, event);
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> IdlePolicy {
        self.asked.fetch_add(1, Ordering::Relaxed);
        self.inner
            .idle(cx)
            .merge(IdlePolicy::BlockUntil(Instant::now() + self.ahead))
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.woken.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut held) = self.last.lock() {
            *held = Instant::now();
        }
        self.inner.deadline_reached(cx);
    }
}

/// Opens a window and reports whether its parks come back.
fn main() -> Result<(), zgui::Error> {
    let mut args = std::env::args().skip(1);
    let id = args
        .next()
        .unwrap_or_else(|| "dev.zgui.park-probe".to_owned());
    let micros: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(200);
    let seconds: u64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(10);

    let asked = Arc::new(AtomicU64::new(0));
    let woken = Arc::new(AtomicU64::new(0));
    let last = Arc::new(std::sync::Mutex::new(Instant::now()));
    let (reading, counted, seen) = (Arc::clone(&asked), Arc::clone(&woken), Arc::clone(&last));

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(seconds));
        let quiet = seen
            .lock()
            .map(|held| held.elapsed())
            .unwrap_or(Duration::ZERO);
        println!(
            "ahead={micros}us asked={} woken={} quiet_for={:?}",
            reading.load(Ordering::Relaxed),
            counted.load(Ordering::Relaxed),
            quiet
        );
        std::process::exit(i32::from(quiet > Duration::from_secs(1)));
    });

    zgui::app()
        .with_application_id(id.clone())
        .with_title(id)
        .with_size(600.0, 400.0)
        .with_stylesheet(":root { background-color: #202020; color: #f0f0f0 }")
        .run_on(
            move |handler| {
                zgui_platform_winit::run(Box::new(Probing {
                    inner: handler,
                    ahead: Duration::from_micros(micros),
                    asked,
                    woken,
                    last,
                }))
            },
            || view! { box {"park probe"} },
        )
}
