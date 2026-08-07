//! An overscroll spring in a real window, with every frame it presents timed.
//!
//! The counterpart of `anim-cadence`, asking the same question of the one motion that answered it
//! differently. A keyframe animation and an elastic edge are both driven by the frame clock and
//! both owe one frame per refresh, and a window can run one frame per refresh for both while
//! *presenting* half as many for the second: a frame whose picture is identical to the last one
//! damages nothing, and a renderer refuses an undamaged frame rather than spending a swap-chain
//! image on pixels the surface already holds. So the loop's own counters agree with a spring that
//! is drawn at half the rate of the output, and only the frames that reach the device can say.
//!
//! Which is what this counts. The window's real renderer is wrapped, every call to
//! [`Renderer::draw`] that ends in a presentation is stamped, and what is reported is the stamps
//! that fell inside each return.
//!
//! ```text
//! scroll-cadence <app-id> [top|bottom] > report.json
//! ```
//!
//! One return per run, at the end named. Two in one run would be two returns of a list that is
//! somewhere different for each, and the second would be a pull against an edge already held to the
//! end of its band — which moves it by a fraction of a pixel and is not a return at all.
//!
//! **Nothing is injected into the desktop.** The pulls are synthesised as platform events and
//! handed to the application from the driver that wraps it, so the whole of the pipeline under
//! measurement is the real one — real windowing backend, real graphics device, real presentation —
//! and no input reaches any other window.
//!
//! The report goes to a file named by `ZGUI_CADENCE_OUT`. It states how many wheel events actually
//! reached the list beside the frame counts, because a measurement of a motion that never started
//! reads exactly like a motion that was never drawn — and how many frames the renderer *refused*,
//! which is the defect this exists for stated directly.

#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zgui::atlas::TextureSink;
use zgui::bits::DamageSet;
use zgui::platform::{AppHandler, PlatformCx, SurfaceEvent, SurfaceId, WakeReason};
use zgui::prelude::*;
use zgui::render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui::scene::Scene;
use zgui::view;
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_vocab::{Modifiers, PointerId, PointerKind, ScrollDelta, ScrollPhase, WheelEvent};

/// When each presented frame reached the device.
type Stamps = Arc<Mutex<Vec<Instant>>>;

/// How many frames the renderer refused because they damaged nothing.
static REFUSED: AtomicU64 = AtomicU64::new(0);

/// How many wheel events reached the list.
///
/// The arrival check. Nothing else in the report can tell a spring that was never started from one
/// that was started and never drawn: both are zero frames.
static WHEELS: AtomicU64 = AtomicU64::new(0);

/// How many `scroll` events the list dispatched.
///
/// A pull past an end moves no clamped offset and so raises none of these, which is what separates
/// an overscroll from a scroll that merely happened to be short.
static SCROLLS: AtomicU64 = AtomicU64::new(0);

/// A long list in a scrollport, which is the document an elastic edge belongs to.
#[component]
fn Listing() -> impl IntoView {
    let rows = (0..400).collect::<Vec<_>>();
    view! {
        column(class = "page") {
            column(class = "port",
                on:wheel = move |_| { WHEELS.fetch_add(1, Ordering::Relaxed); },
                on:scroll = move |_| { SCROLLS.fetch_add(1, Ordering::Relaxed); }) {
                {rows.into_iter().map(|index| view! {
                    column(class = "row") {text {{format!("row {index}")}}}
                }).collect::<Vec<_>>()}
            }
        }
    }
}

/// How the window is laid out.
///
/// The root does not scroll, so the list is the outermost thing that does and the displacement is
/// its own. A root that scrolls takes the leftover instead, and then a second pull at the same end
/// is absorbed by the root rather than displacing anything.
const SHEET: &str = "root { background-color: #101014; overflow: hidden }
                     .page { display: block; padding: 8px; width: 100% }
                     .port { display: block; width: 100%; height: 380px; overflow: scroll;
                             background-color: #16161c }
                     .row { display: block; width: 100%; height: 24px; padding-left: 8px;
                            color: #d0d0d8 }";

/// Opens the window and measures it.
fn main() -> Result<(), zgui::Error> {
    let mut args = std::env::args().skip(1);
    let id = args
        .next()
        .unwrap_or_else(|| "dev.zgui.scroll-cadence".to_owned());
    let edge = match args.next().as_deref() {
        Some("bottom") => Edge::Bottom,
        _ => Edge::Top,
    };

    let stamps: Stamps = Arc::new(Mutex::new(Vec::with_capacity(1 << 14)));
    let counted = Arc::clone(&stamps);
    let driving = Arc::clone(&stamps);

    zgui::app()
        .with_application_id(id.clone())
        .with_title(id)
        .with_size(560.0, 420.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(move |surface, target| {
            Ok(Box::new(Stamping::new(
                open_device(surface, target)?,
                Arc::clone(&counted),
            )))
        }))
        .run_on(
            move |handler| zgui_platform_winit::run(Box::new(Pulling::new(handler, driving, edge))),
            || view! { Listing() },
        )
}

/// How long a return is given before the next thing happens to the list.
///
/// Longer than the return itself, which settles in about a third of a second, so that a cycle
/// never overlaps the next pull.
const RETURN: Duration = Duration::from_millis(1_500);

/// How long the window is left alone before the first pull.
///
/// Opening a window means mapping a surface, opening a graphics device and compiling pipelines,
/// and a run whose first return began inside that reports a rate averaged over a stretch in which
/// nothing could have been drawn.
const WARM_UP: Duration = Duration::from_secs(4);

/// The span of each return that is counted, measured from the pull that started it.
///
/// Very nearly the whole return, and that is the point. A spring decelerates, so a window covering
/// only its first third is a window over the part that moves more than a device pixel per frame on
/// any output — which is the part every version of it draws in full. What separates a return that
/// is drawn once per refresh from one that is not is its *tail*, and a measurement that stops
/// before the tail reports one for both.
const COUNTED: Duration = Duration::from_millis(450);

/// Which end of the list the run measures.
///
/// One end per run, and the run ends with the return. A script that pulled both ends in turn would
/// be measuring a list that is somewhere different every time, and what is wanted here is the same
/// return over and over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    /// The top, which is where the list already is.
    Top,
    /// The bottom, which the list has to be carried to first.
    Bottom,
}

impl Edge {
    /// What the script does, in order: how far each wheel asks to move, and which one is counted.
    const fn script(self) -> &'static [(f32, bool)] {
        match self {
            Self::Top => &[(-600.0, true)],
            // One scroll, far past the end of the list: what it cannot absorb is what displaces
            // the bottom edge, so the travel and the pull are the same event. Two events would be
            // a second pull against an edge that is already held out to the end of its band, which
            // moves it by a fraction of a pixel and is not a return at all.
            Self::Bottom => &[(40_000.0, true)],
        }
    }

    /// The name the report gives it.
    const fn name(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

/// One counted return: when it began, and at which end of the list.
struct Counted {
    /// Which end of the list was displaced.
    edge: Edge,
    /// When the pull that started it was delivered.
    at: Instant,
}

/// The application, with a script of pulls delivered to it between frames.
///
/// Everything is forwarded. The only thing added is that a redraw arriving after a step is due
/// hands the application one synthesised scroll first, exactly as a windowing backend would hand
/// it one that came from a device.
struct Pulling {
    /// The application.
    inner: Box<dyn AppHandler>,
    /// When each presented frame reached the device.
    stamps: Stamps,
    /// Which end of the list this run measures.
    edge: Edge,
    /// When the next step of the script is due.
    due: Option<Instant>,
    /// How many steps of the script have run.
    step: usize,
    /// The returns that have been counted so far.
    counted: Vec<Counted>,
    /// The surface everything is delivered to.
    surface: Option<SurfaceId>,
    /// How big the surface is, in physical pixels, and at what scale.
    ///
    /// Tracked rather than assumed, because a window manager decides how big a window is: a point
    /// measured from the size that was *asked* for lands outside a window that was tiled, and a
    /// scroll aimed outside a window scrolls nothing at all.
    extent: (f32, f32, f32),
}

impl Pulling {
    /// Wraps `inner`, reading the stamps `stamps` collects.
    fn new(inner: Box<dyn AppHandler>, stamps: Stamps, edge: Edge) -> Self {
        Self {
            inner,
            stamps,
            edge,
            due: None,
            step: 0,
            counted: Vec::new(),
            surface: None,
            extent: (560.0, 420.0, 1.0),
        }
    }

    /// Where in the window a scroll is aimed: the middle of the scrollport, whatever size it is.
    fn aim(&self) -> Point<CssPx, Css> {
        let (width, _, scale) = self.extent;
        Point::new(CssPx(width / scale / 2.0), CssPx(120.0))
    }

    /// Delivers `pixels` of scroll over the middle of the list.
    fn scroll(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, pixels: f32) {
        let event = SurfaceEvent::Wheel {
            event: WheelEvent {
                id: PointerId::MOUSE,
                kind: PointerKind::Touch,
                position: self.aim(),
                delta: ScrollDelta::Pixels(Size::new(CssPx(0.0), CssPx(pixels))),
                phase: ScrollPhase::Moved,
            },
            modifiers: Modifiers::NONE,
            timestamp: cx.clock().timestamp(),
        };
        self.inner.surface_event(cx, surface, event);
    }

    /// Runs whatever step of the script is due, if one is.
    fn script(&mut self, cx: &dyn PlatformCx, surface: SurfaceId) {
        let now = Instant::now();
        let due = *self.due.get_or_insert(now + WARM_UP);
        if now < due {
            return;
        }
        let script = self.edge.script();
        let Some(&(pixels, counted)) = script.get(self.step) else {
            self.finish();
            return;
        };
        self.step += 1;
        self.scroll(cx, surface, pixels);
        if counted {
            self.counted.push(Counted {
                edge: self.edge,
                at: Instant::now(),
            });
        }
        self.due = Some(now + RETURN);
    }

    /// Writes the report and ends the run.
    fn finish(&mut self) {
        // After the last return has had time to be drawn, which the script's own spacing gives it.
        std::thread::sleep(RETURN);
        let stamps = self
            .stamps
            .lock()
            .map(|held| held.clone())
            .unwrap_or_default();
        let out = std::env::var("ZGUI_CADENCE_OUT").unwrap_or_else(|_| "/dev/stdout".to_owned());
        let _ = std::fs::write(&out, report(&self.counted, &stamps));
        std::process::exit(0);
    }
}

impl AppHandler for Pulling {
    fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_available(cx);
    }

    fn surfaces_lost(&mut self, cx: &dyn PlatformCx) {
        self.inner.surfaces_lost(cx);
    }

    fn surface_event(&mut self, cx: &dyn PlatformCx, surface: SurfaceId, event: SurfaceEvent) {
        self.surface = Some(surface);
        match event {
            SurfaceEvent::Resized(size) => {
                self.extent = (size.width.0, size.height.0, self.extent.2);
            }
            SurfaceEvent::ScaleFactorChanged { scale_factor, size } => {
                self.extent = (size.width.0, size.height.0, scale_factor as f32);
            }
            _ => {}
        }
        if matches!(event, SurfaceEvent::RedrawRequested) {
            self.script(cx, surface);
        }
        self.inner.surface_event(cx, surface, event);
    }

    fn wake(&mut self, cx: &dyn PlatformCx, reason: WakeReason) {
        self.inner.wake(cx, reason);
    }

    fn idle(&mut self, cx: &dyn PlatformCx) -> zgui::platform::IdlePolicy {
        // The script's own deadline is merged into the park, so that a window with nothing else to
        // do still wakes for the next pull instead of sleeping through the run.
        let policy = self.inner.idle(cx);
        match self.due {
            Some(due) => policy.merge(zgui::platform::IdlePolicy::BlockUntil(due)),
            None => policy,
        }
    }

    fn deadline_reached(&mut self, cx: &dyn PlatformCx) {
        self.inner.deadline_reached(cx);
        if let Some(surface) = self.surface {
            self.script(cx, surface);
        }
    }

    fn shutting_down(&mut self, cx: &dyn PlatformCx) {
        self.inner.shutting_down(cx);
    }
}

/// What the run saw, as one JSON object.
///
/// A rate per return rather than one over the whole run: the list is still between them by design,
/// and a window that correctly draws nothing while nothing moves would drag the average down by
/// exactly as much as a spring that missed half its frames.
fn report(counted: &[Counted], stamps: &[Instant]) -> String {
    let mut returns = Vec::new();
    for run in counted {
        let end = run.at + COUNTED;
        let inside: Vec<Instant> = stamps
            .iter()
            .copied()
            .filter(|at| *at >= run.at && *at <= end)
            .collect();
        let mut gaps: Vec<f64> = inside
            .windows(2)
            .map(|pair| pair[1].duration_since(pair[0]).as_secs_f64() * 1_000.0)
            .collect();
        gaps.sort_by(f64::total_cmp);
        let median = gaps.get(gaps.len() / 2).copied().unwrap_or(0.0);
        returns.push(format!(
            "{{\"edge\":\"{}\",\"frames\":{},\"span_ms\":{:.1},\"median_gap_ms\":{median:.4}}}",
            run.edge.name(),
            inside.len(),
            COUNTED.as_secs_f64() * 1_000.0,
        ));
    }
    let first = counted.first().map(|run| run.at);
    let offsets: Vec<String> = stamps
        .iter()
        .filter_map(|at| first.map(|origin| at.saturating_duration_since(origin)))
        .map(|since| format!("{:.1}", since.as_secs_f64() * 1_000.0))
        .collect();
    format!(
        "{{\"returns\":[{}],\"total\":{},\"refused\":{},\"wheels\":{},\"scrolls\":{},\"since_first_pull_ms\":[{}]}}",
        returns.join(","),
        stamps.len(),
        REFUSED.load(Ordering::Relaxed),
        WHEELS.load(Ordering::Relaxed),
        SCROLLS.load(Ordering::Relaxed),
        offsets.join(",")
    )
}

/// Opens the graphics device for `surface`, exactly as a window of this framework opens one.
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

/// A renderer that stamps every frame it presents and then draws it.
struct Stamping {
    /// What actually draws.
    inner: Box<dyn Renderer>,
    /// When each frame arrived here.
    stamps: Stamps,
    /// How many frames were refused rather than presented.
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
        if matches!(outcome, FrameOutcome::Presented(_)) {
            if let Ok(mut stamps) = self.stamps.lock() {
                stamps.push(at);
            }
        } else {
            self.skipped.fetch_add(1, Ordering::Relaxed);
            REFUSED.fetch_add(1, Ordering::Relaxed);
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
