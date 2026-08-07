//! One animation in a real window, with every frame it presents timed.
//!
//! The question is the simplest one an animation can be asked and the hardest to answer from
//! inside: *how often does it actually get drawn?* Every counter in the process agrees with an
//! animation that ticks at half the rate of the output, because the values it interpolates are read
//! against the clock and are therefore right on every frame that does run. What is wrong is how
//! many run, and only the loop and the compositor together can say.
//!
//! So this measures where the pixels are: the window's real renderer is wrapped, every call to
//! [`Renderer::draw`] is stamped, and the intervals between consecutive stamps are what is
//! reported. One frame per refresh is the whole of the answer, and the ratio it is stated as —
//! frames presented over refreshes elapsed — is the number that has to be one on a two hundred and
//! forty hertz output as much as on a sixty hertz one.
//!
//! ```text
//! anim-cadence <app-id> <seconds> [idle] > report.json
//! ```
//!
//! `idle` mounts the same window with nothing animating in it, which is the other half of the same
//! question: a window that owes no frames must present none at all, and a park that produces one
//! frame per refresh over an animation is worth nothing if it also produces them over a still
//! window.
//!
//! The report goes to a file named by `ZGUI_CADENCE_OUT`, written by a thread of its own so that
//! nothing in the measurement is paid for on the loop. That thread also ends the process, which is
//! what makes a run a fixed length rather than something to be killed from outside.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zgui::atlas::TextureSink;
use zgui::bits::DamageSet;
use zgui::prelude::*;
use zgui::render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui::scene::Scene;
use zgui::view;
use zgui_ui::prelude::*;
use zgui_ui_tokens::prelude::*;

/// When each presented frame reached the device.
type Stamps = Arc<Mutex<Vec<Instant>>>;

/// A window holding the indeterminate progress bar, which is what the defect was seen on.
#[component]
fn Animating() -> impl IntoView {
    let scheme = RwSignal::new_local(ColorScheme::Dark);
    view! {
        ThemeProvider(scheme = scheme) {
            column(class = "page") {
                Progress(label = "Connecting")
            }
        }
    }
}

/// The same window with the bar reporting a number, so that nothing in it moves.
#[component]
fn Still() -> impl IntoView {
    let scheme = RwSignal::new_local(ColorScheme::Dark);
    let value = RwSignal::new_local(Some(40.0));
    view! {
        ThemeProvider(scheme = scheme) {
            column(class = "page") {
                Progress(value = value, max = 100.0, label = "Connecting")
            }
        }
    }
}

/// How the window itself is laid out, which is deliberately almost nothing.
///
/// A frame of this window costs a fraction of a refresh interval on any output, which is what makes
/// the measurement about the *park* rather than about how long a frame takes: a document expensive
/// enough to miss its deadline would report a cadence that is true of the document instead.
const SHEET: &str = "root { background-color: #101014 }
                     .page { display: block; padding: 24px; width: 100% }";

/// Opens the window and measures it.
fn main() -> Result<(), zgui::Error> {
    let mut args = std::env::args().skip(1);
    let id = args
        .next()
        .unwrap_or_else(|| "dev.zgui.anim-cadence".to_owned());
    let seconds: f64 = args.next().and_then(|arg| arg.parse().ok()).unwrap_or(10.0);
    let idle = args.next().is_some_and(|arg| arg == "idle");

    let stamps: Stamps = Arc::new(Mutex::new(Vec::with_capacity(1 << 14)));
    reporting(Arc::clone(&stamps), seconds, idle);

    let counted = Arc::clone(&stamps);
    let app = zgui::app()
        .with_application_id(id.clone())
        .with_title(id)
        .with_size(560.0, 140.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(move |surface, target| {
            Ok(Box::new(Stamping::new(
                open_device(surface, target)?,
                Arc::clone(&counted),
            )))
        }));
    if idle {
        app.run(|| view! { Still() })
    } else {
        app.run(|| view! { Animating() })
    }
}

/// Starts the thread that writes the report and ends the run.
///
/// A thread rather than a timer in the window, because a timer is a deadline and a deadline is
/// exactly what is being measured: a window woken twice a second to write a file is a window whose
/// park has been changed by the act of observing it. This one touches nothing the loop owns beyond
/// a mutex it holds for the length of a `Vec` copy.
fn reporting(stamps: Stamps, seconds: f64, idle: bool) {
    let out = std::env::var("ZGUI_CADENCE_OUT").unwrap_or_else(|_| "/dev/stdout".to_owned());
    std::thread::spawn(move || {
        settled(&stamps, idle);
        stamps.lock().map(|mut held| held.clear()).ok();
        let wall = Instant::now();
        std::thread::sleep(Duration::from_secs_f64(seconds));
        let taken = stamps.lock().map(|held| held.clone()).unwrap_or_default();
        let _ = std::fs::write(&out, report(&taken, wall.elapsed(), idle));
        std::process::exit(0);
    });
}

/// Waits until the window is actually drawing, so that opening it is not part of the measurement.
///
/// A fixed sleep is not enough and the reason is the whole point of waiting: opening a window means
/// mapping a surface, opening a graphics device and compiling pipelines, and how long that takes is
/// a property of the machine and of what it has cached rather than of this program. A run whose span
/// began before the first frame reports a rate averaged over a stretch in which nothing could have
/// been drawn — which looks exactly like the stall this is here to detect.
///
/// An idle window draws nothing by design and would wait for ever, so it waits out a fixed span
/// instead. Nothing is being timed there: the count itself is the assertion.
fn settled(stamps: &Stamps, idle: bool) {
    if idle {
        std::thread::sleep(Duration::from_secs(4));
        return;
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        let drawn = stamps.lock().map(|held| held.len()).unwrap_or(0);
        if drawn >= 60 {
            // Past the first frames, which pay for a cold atlas, a cold pipeline cache and a full
            // first repaint, and would drag the median towards themselves.
            std::thread::sleep(Duration::from_millis(500));
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// What the run saw, as one JSON object.
///
/// The rate is taken between the first frame and the last rather than over the wall-clock span, so
/// that it is the cadence of the frames that were drawn. Both are reported: the two agreeing is what
/// says the window drew for the whole run rather than for part of it.
fn report(stamps: &[Instant], wall: Duration, idle: bool) -> String {
    let mut gaps: Vec<f64> = stamps
        .windows(2)
        .map(|pair| pair[1].duration_since(pair[0]).as_secs_f64() * 1_000.0)
        .collect();
    let drawn = match (stamps.first(), stamps.last()) {
        (Some(first), Some(last)) => last.duration_since(*first).as_secs_f64(),
        _ => 0.0,
    };
    gaps.sort_by(f64::total_cmp);
    let median = gaps.get(gaps.len() / 2).copied().unwrap_or(0.0);
    let fps = if drawn > 0.0 {
        gaps.len() as f64 / drawn
    } else {
        0.0
    };
    format!(
        "{{\"idle\":{idle},\"frames\":{},\"wall_s\":{:.4},\"drawn_s\":{drawn:.4},\
         \"fps\":{fps:.3},\"median_gap_ms\":{median:.4},\"gaps_ms\":{}}}",
        stamps.len(),
        wall.as_secs_f64(),
        serialise(&gaps)
    )
}

/// The gaps as a JSON array, rounded to microseconds.
fn serialise(gaps: &[f64]) -> String {
    let mut out = String::from("[");
    for (index, gap) in gaps.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!("{gap:.3}"));
    }
    out.push(']');
    out
}

/// Opens the graphics device for `surface`, exactly as a window of this framework opens one.
///
/// Repeated here rather than reached for, because what builds a window's renderer is the
/// application's own decision and the one this makes differs only by what it wraps the result in.
fn open_device(
    surface: &Arc<dyn zgui::platform::Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, zgui::Error> {
    let handles = Arc::clone(surface).gpu_shared().ok_or_else(|| {
        zgui::Error::Platform(zgui::platform::PlatformError::Backend(
            "this window offers no handles a graphics API can draw into".to_owned(),
        ))
    })?;
    let presented = Arc::clone(surface);
    let builder = zgui_render_wgpu::Builder::new().with_pre_present(Box::new(move || {
        presented.pre_present_notify();
    }));
    let drawable = builder
        .instance()
        .create_surface(handles)
        .map_err(|error| zgui::platform::PlatformError::Backend(error.to_string()))?;
    let mut renderer = builder.for_surface(target, drawable)?;
    zgui_render_vector_vello::attach(&mut renderer, target.size);
    Ok(Box::new(renderer))
}

/// A renderer that stamps every frame it is asked to draw and then draws it.
///
/// The stamp is taken before the call rather than after, so that what is measured is when the loop
/// reached the renderer and not how long the device took — the second is a property of the frame,
/// and the first is the cadence.
struct Stamping {
    /// What actually draws.
    inner: Box<dyn Renderer>,
    /// When each frame arrived here.
    stamps: Stamps,
    /// How many frames were skipped rather than presented, which the report must not count.
    skipped: AtomicU64,
}

impl Stamping {
    /// Wraps `inner`, recording into `stamps`.
    fn new(inner: Box<dyn Renderer>, stamps: Stamps) -> Self {
        Self {
            inner,
            stamps,
            skipped: AtomicU64::new(0),
        }
    }
}

impl Renderer for Stamping {
    fn capabilities(&self) -> RenderCapabilities {
        self.inner.capabilities()
    }

    fn configure(&mut self, target: RenderTarget) {
        self.inner.configure(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.inner.target()
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let at = Instant::now();
        let outcome = self.inner.draw(scene, damage);
        // Only a frame that reached the screen counts. A skipped one is the renderer declining, and
        // counting it would report a cadence the output never showed.
        if matches!(outcome, FrameOutcome::Presented(_)) {
            if let Ok(mut stamps) = self.stamps.lock() {
                stamps.push(at);
            }
        } else {
            self.skipped.fetch_add(1, Ordering::Relaxed);
        }
        outcome
    }

    fn register_external(&mut self, texture: ExternalTexture) -> TextureHandle {
        self.inner.register_external(texture)
    }

    fn release_external(&mut self, handle: TextureHandle) {
        self.inner.release_external(handle);
    }

    fn memory(&self) -> MemoryReport {
        self.inner.memory()
    }

    fn texture_sink(&mut self) -> &mut dyn TextureSink {
        self.inner.texture_sink()
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        // Forwarded so the wgpu companion crate reaches the real backend through this wrapper.
        self.inner.as_any_mut()
    }
}
