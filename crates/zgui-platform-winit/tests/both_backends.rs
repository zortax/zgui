//! The same runtime, over both backends, with no conditional code anywhere in it.
//!
//! A seam with one implementation behind it is not a seam; it is whatever shape the first
//! implementation happened to have. There are two now — a real event loop and a set of buffers — and
//! this is where that is checked rather than asserted in prose.
//!
//! Two things are checked, and they are different in kind:
//!
//! * **At compile time**, that the two backends are values of one function type. [`DRIVERS`] holds
//!   both in one array and the program picks between them by index. An application that can choose
//!   its backend by index at run time cannot contain a compile-time branch on which one it has,
//!   because there is nothing for such a branch to be written against.
//! * **At run time**, that the same application, built once, actually runs over both — and that the
//!   one decision that genuinely differs between them, whether there are native handles a graphics
//!   API can draw into, is answered by *asking the contract* rather than by knowing which backend
//!   is underneath.

#[path = "support/mod.rs"]
mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use zgui_platform::{AppHandler, PlatformCx, PlatformError, Surface, SurfaceEvent, SurfaceId};
use zgui_platform_headless::Harness;
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::Builder;
use zgui_runtime::{App, AppError};
use zgui_view::{BuildCx, IntoView, View};

/// What a platform backend's entry point looks like from the runtime's side.
///
/// One type, two implementations. Nothing above this signature is written twice.
type Driver = fn(Box<dyn AppHandler>) -> Result<(), PlatformError>;

/// The two backends, as values of one type.
const DRIVERS: [(&str, Driver); 2] = [("headless", headless), ("winit", zgui_platform_winit::run)];

/// The sheet the window is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 100px; height: 50px }";

/// How many frames the application has presented, wherever it is running.
static FRAMES: AtomicU64 = AtomicU64::new(0);

/// Whether the machine turned out to have no graphics device the window could be drawn through.
static NO_DEVICE: AtomicBool = AtomicBool::new(false);

/// Drives the runtime against buffers, one turn at a time.
///
/// A windowing backend blocks until the loop finishes; this one is allowed to return at once, which
/// is the whole reason the contract says a driver *may*.
fn headless(handler: Box<dyn AppHandler>) -> Result<(), PlatformError> {
    let mut harness = Harness::new(handler);
    harness.settle(8);
    harness.shut_down();
    Ok(())
}

/// The runtime, plus the one decision a test has to make that an application does not: when to stop.
///
/// It forwards everything and changes nothing. The point of wrapping rather than editing is that
/// the thing being driven over both backends is the ordinary runtime, unmodified.
struct StopWhenDrawn<A: AppHandler> {
    /// The application.
    inner: A,
}

impl<A: AppHandler> AppHandler for StopWhenDrawn<A> {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        let drawn = matches!(event, SurfaceEvent::RedrawRequested);
        self.inner.surface_event(cx, surface, event);
        if drawn && FRAMES.fetch_add(1, Ordering::Relaxed) >= 1 {
            cx.request_exit();
        }
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: zgui_platform::WakeReason) {
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> zgui_platform::IdlePolicy {
        self.inner.idle(cx)
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.inner.deadline_reached(cx);
    }

    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        self.inner.shutting_down(cx);
    }
}

/// Builds whatever this surface can actually be drawn through.
///
/// This is the one place the two backends differ, and the difference is asked for rather than known:
/// a surface either offers native handles a graphics API can draw into or it does not. A window
/// does, and gets the real renderer, with the surface created from the same instance the device
/// will come from. A buffer does not, and gets one that records. Nothing here names a backend.
fn renderer(
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let Some(handles) = Arc::clone(surface).gpu_shared() else {
        let mut recording = zgui_testkit_scene::CaptureRenderer::new();
        recording.configure(target);
        return Ok(Box::new(recording));
    };

    let builder = Builder::new();
    // The surface has to be created from the instance the device is opened from, which is why the
    // window's handles are handed to the renderer rather than the other way round. The shared
    // handle keeps the window alive for as long as anything draws through it.
    let drawable = builder
        .instance()
        .create_surface(handles)
        .map_err(|error| PlatformError::Backend(error.to_string()))?;
    builder.for_surface(target, drawable).map_or_else(
        |unavailable| {
            // A machine with no usable device is a fact about the machine, not a defect. It is
            // recorded so the program can say so out loud rather than report a silent zero.
            NO_DEVICE.store(true, Ordering::Relaxed);
            Err(AppError::from(unavailable))
        },
        |renderer| Ok(Box::new(renderer) as Box<dyn Renderer>),
    )
}

/// The application, built the same way for both backends.
fn application() -> Result<zgui_runtime::Runtime, AppError> {
    App::new()
        .with_title("both backends")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_renderer(Box::new(renderer))
        .into_handler(|cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(zgui_elements::column())
                    .into_view()
                    .build(cx),
            )
        })
}

fn main() {
    const PROPERTY: &str = "the runtime runs over both backends with no conditional code";

    support::watchdog(PROPERTY);

    // The buffers first, because they need nothing from the machine and prove the application
    // itself works before a window is ever opened. It also leaves the process's one event loop
    // uncreated, so the windowed half below can create it.
    let (buffers, drive) = DRIVERS[0];
    FRAMES.store(0, Ordering::Relaxed);
    drive(Box::new(StopWhenDrawn {
        inner: application().expect("the reactive runtime installs"),
    }))
    .expect("the buffers accepted the application");
    let over_buffers = FRAMES.load(Ordering::Relaxed);
    assert!(
        over_buffers >= 1,
        "the application drew nothing at all over the {buffers} backend"
    );

    // The same application, built by the same function, handed to the other value in the array.
    let (windows, drive) = DRIVERS[1];
    FRAMES.store(0, Ordering::Relaxed);
    match drive(Box::new(StopWhenDrawn {
        inner: application().expect("the reactive runtime installs"),
    })) {
        Ok(()) => {}
        Err(PlatformError::Backend(reason)) => {
            eprintln!("SKIPPED: no windowing system to run the {windows} backend on: {reason}");
            println!("ok (over {buffers} only): {PROPERTY} - {over_buffers} frames");
            return;
        }
        Err(other) => panic!("the {windows} backend refused the application: {other}"),
    }

    if NO_DEVICE.load(Ordering::Relaxed) {
        eprintln!(
            "SKIPPED: this machine has no graphics device, so the window could not be drawn \
             through; the application was still built and driven over the {windows} backend"
        );
        println!("ok (over {buffers} only): {PROPERTY} - {over_buffers} frames");
        return;
    }

    let over_windows = FRAMES.load(Ordering::Relaxed);
    assert!(
        over_windows >= 1,
        "the application drew nothing at all over the {windows} backend"
    );
    println!(
        "ok: {PROPERTY} - {over_buffers} frames over {buffers}, {over_windows} over {windows}"
    );
}
