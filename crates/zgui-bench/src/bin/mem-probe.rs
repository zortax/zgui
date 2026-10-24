//! A memory probe: where the bytes of one more table row go.
//!
//! **This is a diagnostic probe, not a reference workload.** It gates nothing and `cargo xtask` does
//! not run it.
//!
//! The document is a port of the comparison study's data explorer: one keyed row per record, each
//! row thirteen nodes, nine bindings, two memos and three selector subscriptions, under the study's
//! sheet. It runs on the headless harness, with the device's renderer offscreen or, with
//! `--no-gpu`, with a renderer that draws nothing.
//!
//! A global allocator charges every allocation to the thread's
//! [allocation tag](zgui_profile::alloc_tag), which the frame's stage marks and a few hot sites
//! set. The probe takes a snapshot after each step of a run:
//!
//! | snapshot  | after                                                          |
//! |-----------|----------------------------------------------------------------|
//! | `open`    | the window opened and drew its chrome                          |
//! | `model`   | the records and their signals were made                        |
//! | `view`    | the rows were built, before any frame                          |
//! | `frame`   | the first frames settled                                       |
//! | `edits`   | 200 one-cell edits, one frame each                             |
//! | `remount` | 900 rows were unmounted and mounted again                      |
//! | `trim`    | `malloc_trim(0)`                                               |
//!
//! Each snapshot holds the live, peak and block counts per tag, the resident set from `/proc`, the
//! C allocator's statistics, the frame's live counters, the window's object counts and what the
//! renderer and its device allocator hold. The sizes of the records a row is made of are printed
//! once.
//!
//! ```text
//! cargo build --release -p zgui-bench --features mem-probe --bin mem-probe
//! target/release/mem-probe --rows 1000 --mode unvirtualized --no-gpu
//! ZGUI_BENCH_GPU=1 target/release/mem-probe --rows 3000 --tsv 3k.tsv
//! target/release/mem-probe --compare 1k.tsv 3k.tsv
//! ```
//!
//! With `--pause`, the probe prints `MEMPROBE-PAUSE <snapshot> <pid>` at each snapshot and reads
//! one line from standard input before it continues. A line `vram_mib=<n>` records what a wrapper
//! read for the process, such as the used memory `nvidia-smi` reports.

#[path = "mem-probe/compare.rs"]
mod compare;
#[path = "mem-probe/libc.rs"]
mod libc;
#[path = "mem-probe/row.rs"]
mod row;
#[path = "mem-probe/snapshot.rs"]
mod snapshot;
#[path = "mem-probe/tagging.rs"]
mod tagging;

use std::cell::RefCell;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use zgui::geom::{DevicePx, Size};
use zgui::platform::{Surface, SurfaceEvent};
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui_platform_headless::Harness;
use zgui_profile::alloc_tag::{self, Tag};

use crate::row::{Ctx, RootProps, RowState};
use crate::snapshot::Snapshot;

#[global_allocator]
static ALLOCATOR: tagging::Tagging = tagging::Tagging;

/// The one font the study renders with.
static FONT: &[u8] = include_bytes!("../../../zgui-text-parley/tests/fonts/NotoSans-Regular.ttf");

/// The family the sheet names.
const FONT_FAMILY: &str = "Noto Sans";

/// The study's window size.
const WIDTH: f32 = 1280.0;

/// The study's window height.
const HEIGHT: f32 = 800.0;

/// One frame at 60 Hz.
const TICK: Duration = Duration::from_micros(16_667);

/// Which rows a run builds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// Every row is an element.
    Unvirtualized,
    /// The rows in front of the port, through `Virtualize`.
    Virtualized,
    /// The records and their signals, and no rows.
    ModelOnly,
}

impl Mode {
    /// The mode's name on the command line.
    fn name(self) -> &'static str {
        match self {
            Self::Unvirtualized => "unvirtualized",
            Self::Virtualized => "virtualized",
            Self::ModelOnly => "model-only",
        }
    }
}

/// What the command line asked for.
struct Args {
    /// How many records.
    rows: usize,
    /// Which rows are built.
    mode: Mode,
    /// Whether the renderer draws on the device.
    gpu: bool,
    /// Whether each snapshot waits for a line on standard input.
    pause: bool,
    /// How many one-cell edits.
    edits: usize,
    /// How many rows are unmounted and mounted again.
    remount: usize,
    /// Where the JSON goes.
    json: Option<PathBuf>,
    /// Where the figures go, one per line, for `--compare`.
    tsv: Option<PathBuf>,
    /// Two figure files to compare, instead of a run.
    compare: Option<(PathBuf, PathBuf)>,
}

/// The usage line.
const USAGE: &str = "usage: mem-probe [--rows N] [--mode unvirtualized|virtualized|model-only] \
                     [--no-gpu] [--pause] [--edits N] [--remount N] [--json PATH] [--tsv PATH]\n       \
                     mem-probe --compare SMALL.tsv LARGE.tsv";

impl Args {
    /// Reads the command line.
    fn parse() -> Result<Self, String> {
        let mut args = Self {
            rows: 1000,
            mode: Mode::Unvirtualized,
            gpu: std::env::var_os("ZGUI_BENCH_GPU").is_some(),
            pause: false,
            edits: 200,
            remount: 900,
            json: None,
            tsv: None,
            compare: None,
        };
        let mut words = std::env::args().skip(1);
        let number = |word: Option<String>| -> Result<usize, String> {
            word.ok_or_else(|| USAGE.to_owned())?
                .parse()
                .map_err(|error| format!("{error}\n{USAGE}"))
        };
        while let Some(word) = words.next() {
            match word.as_str() {
                "--rows" => args.rows = number(words.next())?,
                "--edits" => args.edits = number(words.next())?,
                "--remount" => args.remount = number(words.next())?,
                "--mode" => {
                    args.mode = match words.next().as_deref() {
                        Some("unvirtualized") => Mode::Unvirtualized,
                        Some("virtualized") => Mode::Virtualized,
                        Some("model-only") => Mode::ModelOnly,
                        _ => return Err(USAGE.to_owned()),
                    }
                }
                "--gpu" => args.gpu = true,
                "--no-gpu" => args.gpu = false,
                "--pause" => args.pause = true,
                "--json" => args.json = words.next().map(PathBuf::from),
                "--tsv" => args.tsv = words.next().map(PathBuf::from),
                "--compare" => {
                    let (Some(small), Some(large)) = (words.next(), words.next()) else {
                        return Err(USAGE.to_owned());
                    };
                    args.compare = Some((small.into(), large.into()));
                }
                _ => return Err(USAGE.to_owned()),
            }
        }
        Ok(args)
    }
}

/// A texture sink that accepts every upload and keeps nothing.
struct NullSink;

impl zgui::atlas::TextureSink for NullSink {
    fn create_texture(
        &mut self,
        _texture: zgui::atlas::TextureId,
        _size: Size<i32, zgui::geom::Device>,
        _format: zgui::atlas::TextureFormat,
    ) -> Result<(), zgui::atlas::SinkError> {
        Ok(())
    }

    fn write_texture(
        &mut self,
        _texture: zgui::atlas::TextureId,
        _bounds: zgui::geom::Rect<i32, zgui::geom::Device>,
        _format: zgui::atlas::TextureFormat,
        _bytes: &[u8],
    ) -> Result<(), zgui::atlas::SinkError> {
        Ok(())
    }

    fn destroy_texture(&mut self, _texture: zgui::atlas::TextureId) {}
}

/// A renderer that accepts each frame and keeps nothing of it.
struct NullRenderer {
    /// The surface it was configured for.
    target: Option<RenderTarget>,
    /// Where tiles are uploaded.
    sink: NullSink,
    /// The next handle an external texture is given.
    next: u64,
}

impl Renderer for NullRenderer {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        zgui::render::RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(
        &mut self,
        _scene: &zgui::scene::Scene,
        _damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        zgui::render::FrameOutcome::Presented(zgui::render::FrameStats::default())
    }

    fn register_external(
        &mut self,
        _texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        self.next += 1;
        zgui::render::TextureHandle(self.next)
    }

    fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.sink
    }
}

/// Builds the renderer the window draws through, charged to [`Tag::Renderer`].
fn renderer(gpu: bool) -> zgui::runtime::RendererFactory {
    Box::new(move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
        alloc_tag::scope(Tag::Renderer, || -> Result<Box<dyn Renderer>, AppError> {
            if !gpu {
                return Ok(Box::new(NullRenderer {
                    target: Some(target),
                    sink: NullSink,
                    next: 0,
                }));
            }
            let mut device = zgui_render_wgpu::Builder::new()
                .offscreen(
                    target,
                    zgui_render_wgpu::wgpu::TextureFormat::Bgra8Unorm,
                    false,
                )
                .map_err(AppError::GpuUnavailable)?;
            zgui_render_vector_vello::attach(&mut device, target.size);
            Ok(Box::new(device))
        })
    })
}

/// Opens the window over the study's document, and returns the harness and the context.
fn open(args: &Args) -> Result<(Harness<Runtime>, Ctx), String> {
    let fonts = zgui::app::Fonts::shipped_only();
    fonts
        .register(Arc::new(FONT), Some(FONT_FAMILY))
        .map_err(|error| format!("font: {error:?}"))?;
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts;
    let unvirtualized = args.mode != Mode::Virtualized;
    let ctx_cell: Rc<RefCell<Option<Ctx>>> = Rc::new(RefCell::new(None));
    let ctx_for_view = Rc::clone(&ctx_cell);
    let runtime = zgui::runtime::App::new()
        .with_title("mem-probe")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(row::SHEET)
        .with_style_threads(0)
        .with_layout_threads(0)
        .with_renderer(renderer(args.gpu))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            let ctx = ctx_for_view
                .borrow()
                .expect("the probe seeds the context before the window opens");
            Box::new(
                view! { Root(ctx = ctx, unvirtualized = unvirtualized) }
                    .into_view()
                    .build(cx),
            )
        })
        .map_err(|error| format!("zgui runtime: {error:?}"))?;
    let ctx = runtime.scope().with(Ctx::new);
    *ctx_cell.borrow_mut() = Some(ctx);
    let mut harness = alloc_tag::scope(Tag::ReactiveBuild, || Harness::new(runtime));
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(WIDTH),
        DevicePx(HEIGHT),
    )));
    harness.settle(256);
    if let Some(error) = harness.app().failure().take() {
        return Err(format!("zgui runtime: {error:?}"));
    }
    Ok((harness, ctx))
}

/// Prints the pause marker and waits for one line, and returns the used memory it reports.
fn pause(name: &str) -> Option<u64> {
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "MEMPROBE-PAUSE {name} {}", std::process::id());
    let _ = stdout.flush();
    drop(stdout);
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok()?;
    line.trim().strip_prefix("vram_mib=")?.parse().ok()
}

/// Settles the harness for a few frames of wall time, as a window that has just changed would.
fn settle(harness: &mut Harness<Runtime>) {
    harness.settle(256);
    for _ in 0..8 {
        harness.advance(TICK);
        harness.pump();
    }
    harness.settle(256);
}

/// The run itself.
fn run(args: &Args) -> Result<(), String> {
    let pid = std::process::id();
    println!(
        "MEMPROBE pid={pid} rows={} mode={} gpu={}",
        args.rows,
        args.mode.name(),
        args.gpu
    );
    let sizes = snapshot::sizes();
    let (mut harness, ctx) = open(args)?;
    let mut snapshots = Vec::with_capacity(8);
    let mut take = |name: &'static str, harness: &Harness<Runtime>| {
        let vram = if args.pause { pause(name) } else { None };
        let snapshot = Snapshot::take(name, &harness.app().windows()[0], vram);
        println!(
            "SNAP {name:<8} rss={:>7} KiB anon={:>7} KiB tagged={:>7} KiB nodes={} boxes={} \
             paragraphs={}",
            snapshot.process("rss_bytes") / 1024,
            snapshot.process("anon_bytes") / 1024,
            snapshot.process("tagged_live_bytes") / 1024,
            object(&snapshot, "dom_nodes"),
            object(&snapshot, "boxes"),
            object(&snapshot, "paragraphs_held"),
        );
        snapshots.push(snapshot);
    };
    take("open", &harness);

    let (model, states) = alloc_tag::scope(Tag::AppModel, || {
        let model = row::records(args.rows);
        let states: Vec<RowState> = harness
            .app()
            .scope()
            .with(|| model.iter().map(RowState::new).collect());
        (model, states)
    });
    ctx.summary_count.set(model.len());
    ctx.summary_total
        .set(model.iter().map(|record| record.price_cents).sum());
    take("model", &harness);

    let mounts = args.mode != Mode::ModelOnly;
    if mounts {
        alloc_tag::scope(Tag::ReactiveBuild, || {
            ctx.rows.set(states.clone());
            harness.app().scope().with(zgui::reactive::flush);
        });
    }
    take("view", &harness);

    settle(&mut harness);
    take("frame", &harness);

    if !states.is_empty() {
        for edit in 0..args.edits {
            let state = states[edit * 7_919 % states.len()];
            alloc_tag::scope(Tag::AppModel, || {
                state
                    .qty
                    .update(|qty| *qty = (*qty + 1 + edit as u32) % 1_000);
            });
            harness.settle(64);
        }
    }
    take("edits", &harness);

    let unmounted = args.remount.min(states.len());
    if mounts && unmounted > 0 {
        ctx.rows.set(states[unmounted..].to_vec());
        harness.settle(256);
        ctx.rows.set(states.clone());
        settle(&mut harness);
    }
    take("remount", &harness);

    let trimmed = libc::trim();
    println!("TRIM gave memory back: {trimmed}");
    take("trim", &harness);

    if let Some(error) = harness.app().failure().take() {
        return Err(format!("zgui runtime: {error:?}"));
    }
    let report = snapshot::Run {
        pid,
        rows: args.rows,
        mode: args.mode.name(),
        gpu: args.gpu,
        sizes: &sizes,
        snapshots: &snapshots,
    };
    println!("SIZES");
    for (name, size) in &sizes {
        println!("  {name:<32} {size:>6} B");
    }
    print!("{}", report.table());
    if let Some(path) = &args.json {
        std::fs::write(path, report.json())
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    if let Some(path) = &args.tsv {
        std::fs::write(path, report.tsv())
            .map_err(|error| format!("{}: {error}", path.display()))?;
    }
    drop(model);
    Ok(())
}

/// One object count of a snapshot, or zero.
fn object(snapshot: &Snapshot, key: &str) -> u64 {
    snapshot
        .objects
        .iter()
        .find(|(name, _)| *name == key)
        .map_or(0, |&(_, value)| value)
}

fn main() {
    let outcome = Args::parse().and_then(|args| match &args.compare {
        Some((small, large)) => compare::compare(small, large).map(|table| print!("{table}")),
        None => run(&args),
    });
    if let Err(error) = outcome {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
