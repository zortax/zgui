//! What every property asserted against a real compositor needs.
//!
//! Each of these targets is its own program, and that is forced rather than chosen: a process
//! opens one connection and runs one loop on it. So each property gets a binary, each binary runs
//! one loop, and the assertions are made after the loop has finished and the application can be
//! read back.
//!
//! Not every program here needs every piece, so what one of them does not use is not dead code but
//! a piece another one needs.

#![allow(dead_code)]

#[path = "paste.rs"]
pub(crate) mod paste;
#[path = "seat.rs"]
pub(crate) mod seat;
#[path = "virtual_pointer.rs"]
pub(crate) mod virtual_pointer;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use zgui_platform::{
    AppHandler, IdlePolicy, PlatformCx, PlatformError, Surface, SurfaceEvent, SurfaceId, WakeReason,
};
use zgui_render::{RenderTarget, Renderer};
use zgui_render_wgpu::{Builder, PresentPacing};
use zgui_runtime::{App, AppError};
use zgui_view::{BuildCx, IntoView, View};

/// How long a scripted property is allowed to take before something is declared stuck.
///
/// A loop that parks when it should not is not slow — it never finishes at all — so a program
/// asserting on parking has to be able to say that out loud rather than hang a machine.
const PATIENCE: Duration = Duration::from_secs(20);

/// The sheet a window is styled by when the property is about a document that is not changing.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 100px; height: 50px }";

/// The same, for a property that needs frames to actually reach the screen.
///
/// A document that is not changing produces no damage, so the frame loop correctly declines to
/// present anything after the first frame — which leaves a property about what a *presented* frame
/// does with nothing to observe. It is also the case that matters: a window nobody can see, running
/// the whole pipeline for ever, is the cost those properties exist to prevent.
const ANIMATED_CSS: &str = "@keyframes pulse { from { opacity: 1 } to { opacity: 0.4 } }
                            root { display: block; width: 400px; height: 300px }
                            column { display: block; width: 100px; height: 50px;
                                     animation: pulse 400ms linear infinite }";

/// How many frames the application has been given, wherever it is running.
pub(crate) static FRAMES: AtomicU64 = AtomicU64::new(0);

/// Whether the machine turned out to have no graphics device the window could be drawn through.
pub(crate) static NO_DEVICE: AtomicBool = AtomicBool::new(false);

/// Turns on the backend's own tracing when `ZGUI_TRACE` asks for it.
///
/// Off by default, because a property that passes says so in one line and a property that fails
/// says why. It is here because the two questions these programs cannot answer from their own
/// assertions — which presentation mode the swap chain took, and whether any acquisition blocked —
/// are both answered by one log line each.
pub(crate) fn tracing() {
    if std::env::var_os("ZGUI_TRACE").is_none() {
        return;
    }
    let level = match std::env::var("ZGUI_TRACE").as_deref() {
        Ok("debug") => tracing::Level::DEBUG,
        Ok("trace") => tracing::Level::TRACE,
        _ => tracing::Level::INFO,
    };
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .try_init();
}

/// Kills the program if the loop has not finished within the patience.
///
/// A pacing failure stalls rather than fails: the compositor stops answering and the loop waits
/// for ever. A suite that never returns tells nobody anything, so this turns it into a message.
pub(crate) fn watchdog(property: &'static str) {
    thread::spawn(move || {
        thread::sleep(PATIENCE);
        eprintln!(
            "FAILED: {property}: the loop never finished, which is the stall this asserts on"
        );
        std::process::exit(1);
    });
}

/// Announces that a property held, or that the machine could not answer it.
pub(crate) fn passed(property: &str, detail: &str) {
    println!("ok: {property} - {detail}");
}

/// Delivers `reason` to the loop after `delay`, from a thread of its own.
///
/// This is what a property about waking needs and what nothing inside the loop can provide: a
/// signal that arrives while the loop is blocked on the compositor's socket, from outside every
/// event stream it is watching.
pub(crate) fn wake_after(
    waker: Arc<dyn zgui_platform::Waker>,
    delay: Duration,
    reason: impl FnOnce() -> WakeReason + Send + 'static,
) {
    thread::spawn(move || {
        thread::sleep(delay);
        waker.wake(reason());
    });
}

/// Asks whichever compositor this is to do something, in its own words.
///
/// Two compositors and one request, because the property being asserted is what a *compositor*
/// says when it stops showing a surface — so the compositor has to be the one that stops showing
/// it, and which one is running is not something the backend under test gets to choose.
pub(crate) fn ask_compositor(sway: &str, hyprland: &str) -> bool {
    if std::env::var_os("SWAYSOCK").is_some() {
        return run("swaymsg", &[sway]);
    }
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        return run("hyprctl", &["--instance", "0", "eval", hyprland]);
    }
    false
}

/// The request that moves this application's own window to `workspace`, and nothing else.
///
/// By window rather than by whatever happens to be focused: these windows deliberately never take
/// focus — they are opened on an output nobody is looking at — so a request aimed at the focused
/// window would move somebody else's.
pub(crate) fn hyprland_move(class: &str, workspace: &str) -> String {
    format!(
        "for _, w in ipairs(hl.get_windows()) do            if w.class == \"{class}\" then              hl.dispatch(hl.dsp.window.move({{ workspace = \"{workspace}\", silent = true, window = w }}))            end          end"
    )
}

/// Runs one command, saying so if it would not.
fn run(program: &str, arguments: &[&str]) -> bool {
    match std::process::Command::new(program).args(arguments).output() {
        Ok(done) if done.status.success() => {
            // Hyprland answers a refused request with a body rather than a status, so the answer is
            // read: a rule that did not apply must not look like one that did.
            !String::from_utf8_lossy(&done.stdout).starts_with("error")
        }
        Ok(done) => {
            eprintln!(
                "the compositor refused `{program} {}`: {}",
                arguments.join(" "),
                String::from_utf8_lossy(&done.stderr).trim()
            );
            false
        }
        Err(error) => {
            eprintln!("the compositor could not be driven: {error}");
            false
        }
    }
}

/// Opens a loop, or says why this machine has none to open.
pub(crate) fn loop_for<A: AppHandler>(
    property: &str,
    handler: A,
) -> Option<zgui_platform_wayland::WaylandApp<A>> {
    match zgui_platform_wayland::WaylandApp::new(handler) {
        Ok(app) => Some(app),
        Err(PlatformError::Backend(reason)) => {
            skipped(property, &format!("no compositor to run on: {reason}"));
            None
        }
        Err(other) => panic!("the compositor refused the application: {other}"),
    }
}

/// Announces that this machine has nothing to say about the property.
pub(crate) fn skipped(property: &str, reason: &str) {
    eprintln!("SKIPPED: {property}: {reason}");
}

/// The runtime, plus the two decisions a test has to make that an application does not: how to keep
/// the frames coming, and when to stop.
///
/// A document that is not changing asks for one frame and then nothing, which is the whole point of
/// the frame loop and is useless for measuring a chain. So each delivered redraw asks for the next
/// one — the same thing an animation does, and the shortest path to a sustained chain.
///
/// Everything else is forwarded unchanged. What is being driven is the ordinary runtime.
pub(crate) struct StopAfter<A: AppHandler> {
    /// The application.
    pub(crate) inner: A,
    /// How many frames to give it before asking the loop to finish.
    pub(crate) frames: u64,
    /// Whether each frame asks for the next.
    pub(crate) sustained: bool,
}

impl<A: AppHandler> AppHandler for StopAfter<A> {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        let drawn = matches!(event, SurfaceEvent::RedrawRequested);
        self.inner.surface_event(cx, surface, event);
        if !drawn {
            return;
        }
        if FRAMES.fetch_add(1, Ordering::Relaxed) + 1 >= self.frames {
            cx.request_exit();
        } else if self.sustained
            && let Some(surface) = cx.surface(surface)
        {
            surface.request_redraw();
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

/// Builds whatever this surface can actually be drawn through.
///
/// The pacing is asked for rather than assumed, exactly as the application does it: a backend that
/// paces frames itself gets presentation that never blocks, and is told when a buffer is about to
/// be committed so that it can ask the compositor for the next frame.
pub(crate) fn renderer(
    surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    let Some(handles) = Arc::clone(surface).gpu_shared() else {
        return Err(AppError::Platform(PlatformError::Backend(
            "this surface offers no handles a graphics API can draw into".to_owned(),
        )));
    };
    let pacing = match surface.present_pacing() {
        zgui_platform::PresentPacing::Platform => PresentPacing::Platform,
        _ => PresentPacing::Display,
    };
    let notified = Arc::clone(surface);
    let builder = Builder::new()
        .with_present_pacing(pacing)
        .with_pre_present(Box::new(move || notified.pre_present_notify()));
    let drawable = builder
        .instance()
        .create_surface(handles)
        .map_err(|error| PlatformError::Backend(error.to_string()))?;
    builder.for_surface(target, drawable).map_or_else(
        |unavailable| {
            NO_DEVICE.store(true, Ordering::Relaxed);
            Err(AppError::from(unavailable))
        },
        |renderer| Ok(Box::new(renderer) as Box<dyn Renderer>),
    )
}

/// The identifier every window these properties open is grouped under.
///
/// Named rather than defaulted so that a desktop can be told to keep these windows out of the way
/// while they run: a property asserted against a real compositor opens a real window, and a
/// window-rule needs something to match on.
pub(crate) const APP_ID: &str = "dev.zgui.platform-wayland-test";

/// The application a property about a still document drives.
pub(crate) fn application(title: &'static str) -> Result<zgui_runtime::Runtime, AppError> {
    styled(title, CSS)
}

/// The application a property that needs presented frames drives.
pub(crate) fn animated_application(title: &'static str) -> Result<zgui_runtime::Runtime, AppError> {
    styled(title, ANIMATED_CSS)
}

/// The application both are, differing only in what it is styled by.
fn styled(title: &'static str, css: &'static str) -> Result<zgui_runtime::Runtime, AppError> {
    App::new()
        .with_title(title)
        .with_application_id(APP_ID)
        .with_size(400.0, 300.0)
        .with_stylesheet(css)
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
