//! What the animation and memory probes share.
//!
//! A probe opens one window on the headless harness over a document of its own and steps the
//! virtual clock one refresh at a time. For each step it records the frames the step drew, its wall
//! time and the counter deltas. With `ZGUI_BENCH_GPU=1` the window draws on this machine's device
//! offscreen, and with `ZGUI_GPU_TIMING=1` as well each draw's GPU time is recorded. A probe gates
//! nothing: it prints what it saw.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use zgui::bits::DamageSet;
use zgui::geom::{DevicePx, Size};
use zgui::platform::{Surface, SurfaceEvent};
use zgui::render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui::runtime::{AppError, Runtime};
use zgui::scene::Scene;
use zgui::view::{Anchor, BuildCx};
use zgui_platform_headless::Harness;
use zgui_profile::counter::{self, Counters};
use zgui_render_wgpu::WgpuRenderer;

/// One refresh at 60 Hz, which is what each step advances the clock by.
pub const TICK: Duration = Duration::from_micros(16_667);

/// The one font every probe renders with, so a run shapes the same glyphs on every machine.
static FONT: &[u8] = include_bytes!("../../zgui-text-parley/tests/fonts/NotoSans-Regular.ttf");

/// The family a probe's sheet names.
pub const FONT_FAMILY: &str = "Noto Sans";

/// The variant named by the first argument.
///
/// With no argument, runs this program once per entry of `all`, each in a process of its own, and
/// exits: a window opened in a process that already opened one starts from that window's caches.
/// The exit status is 1 when any run failed.
#[must_use]
pub fn variant_or_each(all: &[&str]) -> String {
    if let Some(named) = std::env::args().nth(1) {
        return named;
    }
    let exe = std::env::current_exe().unwrap_or_else(|error| {
        eprintln!("this program cannot find itself: {error}");
        std::process::exit(2);
    });
    let mut failed = false;
    for variant in all {
        let status = std::process::Command::new(&exe).arg(variant).status();
        failed |= !status.is_ok_and(|status| status.success());
    }
    std::process::exit(i32::from(failed));
}

/// What a probe's window draws through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    /// A renderer that records the scene and draws nothing, so every number is the CPU's.
    Capture,
    /// This machine's device, offscreen.
    Gpu,
}

impl Device {
    /// [`Device::Gpu`] when `ZGUI_BENCH_GPU` is set, else [`Device::Capture`].
    #[must_use]
    pub fn from_env() -> Self {
        if std::env::var_os("ZGUI_BENCH_GPU").is_some() {
            Self::Gpu
        } else {
            Self::Capture
        }
    }

    /// The name a report prints.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Capture => "capture",
            Self::Gpu => "gpu",
        }
    }
}

/// One draw a window made.
#[derive(Clone, Copy, Debug)]
struct Drawn {
    /// Whether the frame reached the target.
    presented: bool,
    /// The device pixels the frame damaged.
    damage_px: i64,
    /// The GPU milliseconds of the draw. `None` until its result arrives.
    gpu_ms: Option<f64>,
}

/// Every draw a window made, and the GPU time of each one that has a result.
#[derive(Debug, Default)]
pub struct Draws {
    /// The draws, in draw order.
    drawn: Vec<Drawn>,
    /// The draw each timed frame belongs to, for the results that have not arrived.
    pending: BTreeMap<u64, usize>,
    /// Whether the renderer times its passes.
    timed: bool,
}

impl Draws {
    /// How many draws the window made.
    #[must_use]
    pub fn count(&self) -> usize {
        self.drawn.len()
    }

    /// How many of the draws in `draws` reached the target.
    #[must_use]
    pub fn presented(&self, draws: Range<usize>) -> usize {
        self.drawn.get(draws).map_or(0, |drawn| {
            drawn.iter().filter(|drawn| drawn.presented).count()
        })
    }

    /// The device pixels the draws in `draws` damaged together.
    #[must_use]
    pub fn damage_px(&self, draws: Range<usize>) -> i64 {
        self.drawn
            .get(draws)
            .map_or(0, |drawn| drawn.iter().map(|drawn| drawn.damage_px).sum())
    }

    /// Whether the renderer times its passes on the GPU.
    #[must_use]
    pub fn timed(&self) -> bool {
        self.timed
    }

    /// The GPU milliseconds of the draws in `draws` together, or `None` when none has a result.
    #[must_use]
    pub fn gpu_ms(&self, draws: Range<usize>) -> Option<f64> {
        let mut total = None;
        for drawn in self.drawn.get(draws).unwrap_or_default() {
            if let Some(ms) = drawn.gpu_ms {
                *total.get_or_insert(0.0) += ms;
            }
        }
        total
    }
}

/// The record a window's renderer writes, shared with the probe that opened it.
pub type DrawLog = Rc<RefCell<Draws>>;

/// A renderer that records each draw and, when it can, the draw's GPU time.
struct Counted {
    /// The renderer that does the work.
    inner: Box<dyn Renderer>,
    /// Where each draw is recorded.
    log: DrawLog,
}

impl Counted {
    /// The device renderer underneath, if there is one.
    fn device(&mut self) -> Option<&mut WgpuRenderer> {
        self.inner
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<WgpuRenderer>())
    }

    /// Files every GPU result that has arrived under the draw that made it.
    fn collect(&mut self) {
        let log = Rc::clone(&self.log);
        let Some(device) = self.device() else {
            return;
        };
        let timings = device.take_gpu_timings();
        let mut log = log.borrow_mut();
        for timing in timings {
            if let Some(draw) = log.pending.remove(&timing.frame) {
                log.drawn[draw].gpu_ms = Some(timing.total_ms());
            }
            // Results arrive in frame order, so an older frame still pending got no query set.
            log.pending.retain(|frame, _| *frame > timing.frame);
        }
    }
}

impl Drop for Counted {
    fn drop(&mut self) {
        let waited = match self.device() {
            Some(device) if device.gpu_timing_enabled() => {
                device.gpu().wait();
                true
            }
            _ => false,
        };
        if waited {
            self.collect();
        }
    }
}

impl Renderer for Counted {
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
        let before = self.device().and_then(|device| device.gpu_timed_frames());
        let outcome = self.inner.draw(scene, damage);
        let after = self.device().and_then(|device| device.gpu_timed_frames());
        let surface = self.inner.target().map_or(0, |target| {
            i64::from(target.size.width) * i64::from(target.size.height)
        });
        {
            let mut log = self.log.borrow_mut();
            let draw = log.drawn.len();
            log.drawn.push(Drawn {
                presented: matches!(outcome, FrameOutcome::Presented(_)),
                damage_px: damage.area().unwrap_or(surface),
                gpu_ms: None,
            });
            if let (Some(before), Some(after)) = (before, after) {
                log.timed = true;
                for frame in before..after {
                    log.pending.insert(frame, draw);
                }
            }
        }
        self.collect();
        outcome
    }

    fn shifts_composed_pixels(&self) -> bool {
        self.inner.shifts_composed_pixels()
    }

    fn composed_survives_configure(&self) -> bool {
        self.inner.composed_survives_configure()
    }

    fn shift_composed(&mut self, shift: zgui::render::ScrollShift) {
        self.inner.shift_composed(shift);
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

    fn vector_status(&self) -> zgui::render::VectorStatus {
        self.inner.vector_status()
    }

    fn target_pool(&self) -> zgui::render::TargetPoolReport {
        self.inner.target_pool()
    }

    fn release_cached_targets(&mut self) -> u64 {
        self.inner.release_cached_targets()
    }

    fn release_idle_resources(&mut self) -> u64 {
        self.inner.release_idle_resources()
    }

    fn acquire_block(&self) -> Duration {
        self.inner.acquire_block()
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.inner.texture_sink()
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        self.inner.as_any_mut()
    }

    fn as_any(&self) -> Option<&dyn core::any::Any> {
        self.inner.as_any()
    }
}

/// Builds the renderer a probe's window draws through.
fn renderer(device: Device, log: DrawLog) -> zgui::runtime::RendererFactory {
    Box::new(
        move |_surface: &Arc<dyn Surface>,
              target: RenderTarget|
              -> Result<Box<dyn Renderer>, AppError> {
            let inner: Box<dyn Renderer> = match device {
                Device::Capture => {
                    let mut capture = zgui_testkit_scene::CaptureRenderer::new();
                    capture.configure(target);
                    Box::new(capture)
                }
                Device::Gpu => {
                    let mut gpu = zgui_render_wgpu::Builder::new()
                        .offscreen(
                            target,
                            zgui_render_wgpu::wgpu::TextureFormat::Bgra8Unorm,
                            false,
                        )
                        .map_err(AppError::GpuUnavailable)?;
                    zgui_render_vector_vello::attach(&mut gpu, target.size);
                    if zgui_render_wgpu::gpu::timing::requested() && !gpu.gpu_timing_enabled() {
                        eprintln!(
                            "GPU timing was asked for, but {} has no timestamp queries",
                            gpu.gpu().describe()
                        );
                    }
                    Box::new(gpu)
                }
            };
            Ok(Box::new(Counted {
                inner,
                log: Rc::clone(&log),
            }))
        },
    )
}

/// One open probe window.
pub struct Opened {
    /// The driven application.
    pub harness: Harness<Runtime>,
    /// What its renderer recorded.
    pub draws: DrawLog,
}

/// Opens a window `width` by `height` device pixels at scale one over `sheet` and `view`.
///
/// The window is settled and has drawn its first frame when this returns.
///
/// # Errors
///
/// Returns a message when the font does not register, the runtime does not install, or the
/// window failed while it opened, such as when [`Device::Gpu`] finds no usable device.
pub fn open<V>(
    title: &str,
    width: f32,
    height: f32,
    sheet: &str,
    device: Device,
    view: V,
) -> Result<Opened, String>
where
    V: FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
{
    let fonts = zgui::app::Fonts::shipped_only();
    fonts
        .register(Arc::new(FONT), Some(FONT_FAMILY))
        .map_err(|error| format!("font: {error:?}"))?;
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts;
    let draws = DrawLog::default();
    let runtime = zgui::runtime::App::new()
        .with_title(title)
        .with_size(width, height)
        .with_stylesheet(sheet)
        .with_renderer(renderer(device, Rc::clone(&draws)))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(view)
        .map_err(|error| format!("zgui runtime: {error:?}"))?;
    let mut harness = Harness::new(runtime);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(width),
        DevicePx(height),
    )));
    harness.settle(256);
    if let Some(error) = harness.app().failure().take() {
        return Err(format!("zgui runtime: {error:?}"));
    }
    Ok(Opened { harness, draws })
}

/// Prints one `CENSUS` line: what the window holds right now.
///
/// It is the check that the document was built: a probe over an empty window reports a frame
/// that costs nothing.
pub fn census(probe: &str, opened: &Opened) {
    use zgui_profile::Counter;
    let window = &opened.harness.app().windows()[0];
    let dom = window.dom();
    let mut nodes = 0_usize;
    let mut stack = vec![dom.root_node()];
    while let Some(node) = stack.pop() {
        nodes += 1;
        stack.extend(dom.children(node));
    }
    println!(
        "CENSUS\t{probe}\tdom_nodes={nodes}\tfragments_live={}\tpaint_chunk_bytes={}\t\
         paint_entries_live={}\tdraws={}",
        counter::get(Counter::FragmentsLive),
        counter::get(Counter::PaintChunkBytes),
        counter::get(Counter::PaintEntriesLive),
        opened.draws.borrow().count(),
    );
}

/// What one step of the clock did.
#[derive(Clone, Debug)]
pub struct Step {
    /// How many frames the step drew.
    pub frames: u64,
    /// The wall time of the step: the clock advance and the frames it caused.
    pub wall: Duration,
    /// What the counters moved by.
    pub counters: Counters,
    /// The draws the step made, as indices into the window's [`Draws`].
    pub draws: Range<usize>,
}

/// Advances the clock by [`TICK`] `steps` times, drawing what each advance owes.
pub fn step(opened: &mut Opened, steps: usize) -> Vec<Step> {
    let mut out = Vec::with_capacity(steps);
    for _ in 0..steps {
        let first = opened.draws.borrow().count();
        let before = counter::snapshot();
        let started = Instant::now();
        opened.harness.advance(TICK);
        let frames = opened.harness.pump();
        let wall = started.elapsed();
        let counters = before.delta(&counter::snapshot());
        let last = opened.draws.borrow().count();
        out.push(Step {
            frames,
            wall,
            counters,
            draws: first..last,
        });
    }
    out
}

/// The `q` quantile of `values` by nearest rank, or `None` when there are none.
#[must_use]
pub fn quantile(values: &mut [f64], q: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "a rank into a list of at most a few thousand samples"
    )]
    let rank = ((q * values.len() as f64).ceil() as usize).clamp(1, values.len());
    Some(values[rank - 1])
}

/// A value to print, or `-` when there is none.
fn or_dash(value: Option<f64>, digits: usize) -> String {
    value.map_or_else(|| "-".to_owned(), |value| format!("{value:.digits$}"))
}

/// Prints one `FRAME` line per step and a `SUMMARY` line for `probe`.
///
/// The summary's percentiles and means are over the steps that ran a frame. GPU time is printed
/// only when the window's renderer timed its passes; read `draws` after the window has dropped, so
/// the last results have arrived.
pub fn report(probe: &str, steps: &[Step], draws: &Draws) {
    for (index, step) in steps.iter().enumerate() {
        let counters = &step.counters;
        println!(
            "FRAME\t{probe}\t{index}\tframes={}\tdraws={}\tpresented={}\tdamage_px={}\twall_us={:.1}\tchunks_reencoded={}\t\
             chunks_translated={}\tprimitives_emitted={}\tnodes_visited={}\trepaints={}\t\
             group_targets={}\ttier_b_transitions={}\tlayout_passes={}\tgpu_ms={}",
            step.frames,
            step.draws.len(),
            draws.presented(step.draws.clone()),
            draws.damage_px(step.draws.clone()),
            step.wall.as_secs_f64() * 1e6,
            counters.chunks_reencoded,
            counters.chunks_translated,
            counters.primitives_emitted,
            counters.nodes_visited,
            counters.repaints,
            counters.group_targets,
            counters.tier_b_transitions,
            counters.layout_passes,
            or_dash(draws.gpu_ms(step.draws.clone()), 4),
        );
    }
    let drawn: Vec<&Step> = steps.iter().filter(|step| step.frames > 0).collect();
    let mut walls: Vec<f64> = drawn
        .iter()
        .map(|step| step.wall.as_secs_f64() * 1e6)
        .collect();
    let mut gpus: Vec<f64> = drawn
        .iter()
        .filter_map(|step| draws.gpu_ms(step.draws.clone()))
        .collect();
    #[expect(
        clippy::cast_precision_loss,
        reason = "counter totals and step counts stay far below 2^52"
    )]
    let mean = |field: fn(&Counters) -> u64| -> String {
        if drawn.is_empty() {
            return "-".to_owned();
        }
        let total: u64 = drawn.iter().map(|step| field(&step.counters)).sum();
        format!("{:.1}", total as f64 / drawn.len() as f64)
    };
    println!(
        "SUMMARY\t{probe}\tsteps={}\tdrawn={}\tgpu_timed={}\twall_us\tp50={};p95={}\tgpu_ms\tp50={};p95={};n={}\t\
         per_drawn_step\tchunks_reencoded={};chunks_translated={};primitives_emitted={};\
         nodes_visited={};repaints={};group_targets={};layout_passes={}",
        steps.len(),
        drawn.len(),
        draws.timed(),
        or_dash(quantile(&mut walls, 0.5), 1),
        or_dash(quantile(&mut walls, 0.95), 1),
        or_dash(quantile(&mut gpus, 0.5), 4),
        or_dash(quantile(&mut gpus, 0.95), 4),
        gpus.len(),
        mean(|counters| counters.chunks_reencoded),
        mean(|counters| counters.chunks_translated),
        mean(|counters| counters.primitives_emitted),
        mean(|counters| counters.nodes_visited),
        mean(|counters| counters.repaints),
        mean(|counters| counters.group_targets),
        mean(|counters| counters.layout_passes),
    );
}
