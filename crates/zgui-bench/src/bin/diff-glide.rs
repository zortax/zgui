//! A diagnostic probe: what one glide frame costs over a diff-shaped virtual list.
//!
//! **This is not a CI reference workload.** It has no criterion and gates nothing. It exists to
//! answer one report — "scrolling through larger diffs doesn't feel smooth" — with numbers: the
//! rows here are the shape a diff viewer draws (two fixed line-number cells, a mark, and a line
//! of text cut into styled spans), the list is [`zgui_ui::virtualize::VirtualList`] exactly as
//! the viewer mounts it, and the fonts are the machine's own.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin diff-glide
//! ```
//!
//! # What it measures
//!
//! Two gestures, the frames of each split by what the frame did:
//!
//! * a **reading glide** — a three-line notch, carried to rest, repeated — where most frames
//!   cross at most one row boundary; and
//! * a **fling** — notches stacked faster than the glide decays — where every frame crosses
//!   several, and each crossing is a row built, styled, laid out and shaped from nothing.
//!
//! A frame that crossed no boundary is a *translation*; one that did is a *recycle*, bucketed by
//! how many rows arrived in it. Beside each bucket's time distribution the counters say what the
//! frames actually did: paragraphs shaped, glyphs placed, nodes relaid out, chunks re-encoded
//! against replayed.

#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

use zgui::app::Fonts;
use zgui::geom::{Css, CssPx, DevicePx, Point, Size};
use zgui::platform::SurfaceEvent;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx, IntoView, View};
use zgui::vocab::{
    Modifiers, PointerAction, PointerEvent, PointerId, PointerKind, ScrollDelta, ScrollPhase,
    Timestamp, WheelEvent,
};
use zgui::{component, view};
use zgui_platform_headless::Harness;
use zgui_ui::prelude::*;

/// How wide the window opens, in CSS pixels.
const WIDTH: f32 = 1000.0;

/// How tall it opens, which is also the port.
const HEIGHT: f32 = 800.0;

/// How many rows the diff holds. Far more than the port shows, which is the point of the list.
const ROWS: usize = 50_000;

/// How tall one row is — the diff viewer's own figure.
const ROW: f32 = 17.0;

/// One tick of a 120 Hz refresh.
const TICK: Duration = Duration::from_micros(8_333);

/// The diff viewer's row markup, near enough: tight rows, fixed number cells, styled spans.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: monospace; font-size: 12px }
     .diff-list { height: 800px }
     .diff-row { flex-direction: row; height: 17px; align-items: center; line-height: 17px }
     .diff-number { width: 44px; color: #5b6272 }
     .diff-mark { width: 14px }
     .diff-text { flex: 1 1 auto }
     .tone-a { color: #7fd1a7 }
     .tone-b { color: #d19a66 }
     .tone-c { color: #61afef }
     .tone-d { color: #c678dd }"
);

/// The words the lines are assembled from.
const WORDS: &[&str] = &[
    "let",
    "mutably",
    "borrowed",
    "renderer",
    "configure",
    "target",
    "size",
    "self",
    "frame",
    "damage",
    "outcome",
    "expect",
    "pixels",
    "stats",
    "bytes",
    "uploaded",
    "scene",
    "chunk",
];

/// A few words of deterministic code-like text.
fn piece(seed: usize, count: usize) -> String {
    let mut text = String::new();
    for step in 0..count {
        if step > 0 {
            text.push(' ');
        }
        text.push_str(
            WORDS[seed.wrapping_mul(31).wrapping_add(step.wrapping_mul(7)) % WORDS.len()],
        );
    }
    text
}

/// One row of the probe's diff: numbers, a mark, and a line in six styled spans.
#[component]
fn ProbeRow(
    /// Where the row is in the diff.
    index: usize,
) -> impl IntoView {
    let mark = match index % 7 {
        0 | 1 => "+",
        2 => "-",
        _ => " ",
    };
    view! {
        row(class = "diff-row") {
            label(class = "diff-number") {{(index / 2).to_string()}}
            label(class = "diff-number") {{index.to_string()}}
            label(class = "diff-mark") {{mark}}
            box(class = "diff-text") {
                text(class = "tone-c") {{piece(index, 2)}}
                text {{format!(" {} ", piece(index + 3, 3))}}
                text(class = "tone-a") {{piece(index + 5, 1)}}
                text {{format!(" {} ", piece(index + 7, 2))}}
                text(class = "tone-d") {{piece(index + 11, 1)}}
                text(class = "tone-b") {{format!(" {}", piece(index + 13, 2))}}
            }
        }
    }
}

/// The list itself, exactly as the diff viewer mounts one.
#[component]
fn ProbeList() -> impl IntoView {
    let count = zgui::reactive::RwSignal::new_local(ROWS);
    view! {
        VirtualList(
            class = "diff-list",
            count = count,
            row_size = ROW,
            label = "Diff",
            row = move |index: usize| view! { ProbeRow(index = index) },
        )
    }
}

/// The recorder, answering yes to moving composed pixels it does not hold.
///
/// The same stance as the harness renderer the scenarios draw through: this probe takes CPU
/// numbers, and the decision to shift narrows the frame's damage — the emit walk, replays and
/// order inserts that follow from the narrowing are exactly what is being measured. Answering no
/// would measure a frame the framework does not draw where a device exists.
struct Shifting(zgui_testkit_scene::CaptureRenderer);

impl Renderer for Shifting {
    fn capabilities(&self) -> zgui::render::RenderCapabilities {
        self.0.capabilities()
    }
    fn configure(&mut self, target: RenderTarget) {
        self.0.configure(target);
    }
    fn target(&self) -> Option<RenderTarget> {
        self.0.target()
    }
    fn draw(
        &mut self,
        scene: &zgui::scene::Scene,
        damage: &zgui::bits::DamageSet,
    ) -> zgui::render::FrameOutcome {
        self.0.draw(scene, damage)
    }
    fn shifts_composed_pixels(&self) -> bool {
        true
    }
    fn shift_composed(&mut self, _shift: zgui::render::ScrollShift) {}
    fn register_external(
        &mut self,
        texture: zgui::render::ExternalTexture,
    ) -> zgui::render::TextureHandle {
        self.0.register_external(texture)
    }
    fn release_external(&mut self, handle: zgui::render::TextureHandle) {
        self.0.release_external(handle)
    }
    fn memory(&self) -> zgui::render::MemoryReport {
        self.0.memory()
    }
    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        self.0.texture_sink()
    }
}

/// Opens the probe's window over the machine's own fonts.
fn opened() -> Harness<Runtime> {
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("diff-glide")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(
            |_surface: &std::sync::Arc<dyn zgui::platform::Surface>,
             target: RenderTarget|
             -> Result<Box<dyn Renderer>, AppError> {
                let mut inner = Shifting(zgui_testkit_scene::CaptureRenderer::new());
                inner.configure(target);
                Ok(Box::new(inner))
            },
        ))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { ProbeList() }.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(WIDTH),
        DevicePx(HEIGHT),
    )));
    harness.settle(512);
    for _ in 0..8 {
        harness.advance(Duration::from_micros(16_667));
        harness.pump();
    }
    harness.settle(512);
    harness
}

/// The middle of the port.
fn middle() -> Point<CssPx, Css> {
    Point::new(CssPx(WIDTH / 2.0), CssPx(HEIGHT / 2.0))
}

/// One wheel notch of `lines`.
fn notch(lines: f32) -> SurfaceEvent {
    SurfaceEvent::Wheel {
        event: WheelEvent {
            delta: ScrollDelta::Lines { x: 0.0, y: lines },
            phase: ScrollPhase::Discrete,
            position: middle(),
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

/// One frame class's population.
#[derive(Default)]
struct Bucket {
    /// Frame costs, in microseconds.
    costs: Vec<f64>,
    /// Counter totals across the bucket's frames.
    shaped: u64,
    rebroken: u64,
    glyphs: u64,
    relaid: u64,
    reencoded: u64,
    translated: u64,
    emitted: u64,
}

impl Bucket {
    fn take(&mut self, cost: f64, moved: &zgui_profile::Counters) {
        self.costs.push(cost);
        self.shaped += moved.text_shaped;
        self.rebroken += moved.text_rebroken;
        self.glyphs += moved.glyphs_placed;
        self.relaid += moved.nodes_relaid_out;
        self.reencoded += moved.chunks_reencoded;
        self.translated += moved.chunks_translated;
        self.emitted += moved.primitives_emitted;
    }

    fn report(&mut self, name: &str) {
        if self.costs.is_empty() {
            println!("  {name:<26} (no frames)");
            return;
        }
        self.costs.sort_by(f64::total_cmp);
        let at = |q: f64| self.costs[((self.costs.len() - 1) as f64 * q) as usize];
        let n = self.costs.len() as u64;
        println!(
            "  {name:<26} n={n:<5} p50={:8.1}us p95={:8.1}us max={:8.1}us | per frame: shaped={:.1} rebroken={:.1} glyphs={:.0} relaid={:.1} reencoded={:.1} translated={:.1} emitted={:.0}",
            at(0.5),
            at(0.95),
            self.costs[self.costs.len() - 1],
            self.shaped as f64 / n as f64,
            self.rebroken as f64 / n as f64,
            self.glyphs as f64 / n as f64,
            self.relaid as f64 / n as f64,
            self.reencoded as f64 / n as f64,
            self.translated as f64 / n as f64,
            self.emitted as f64 / n as f64,
        );
    }
}

/// How many rows one frame's rebuilt boxes amount to.
///
/// A probe row is a fixed shape, so the quotient is exact enough to bucket by.
fn rows_arrived(boxes_rebuilt: u64) -> u64 {
    // row + 2 numbers + mark + text box: the flattened text spans generate no boxes of their own.
    boxes_rebuilt / 5
}

/// Drives `ticks` glide ticks, delivering `notch_lines` every `notch_every` ticks, into buckets.
fn drive(
    harness: &mut Harness<Runtime>,
    ticks: usize,
    notch_every: usize,
    notch_lines: f32,
    translation: &mut Bucket,
    arrivals: &mut [Bucket; 3],
) {
    for tick in 0..ticks {
        if tick % notch_every == 0 {
            harness.deliver_to_first(notch(notch_lines));
        }
        let mark = zgui_profile::counter::snapshot();
        let started = Instant::now();
        harness.advance(TICK);
        let ran = harness.pump();
        let cost = started.elapsed().as_secs_f64() * 1e6;
        if ran == 0 {
            continue;
        }
        let moved = mark.delta(&zgui_profile::counter::snapshot());
        if moved.boxes_rebuilt == 0 {
            translation.take(cost, &moved);
        } else {
            let bucket = match rows_arrived(moved.boxes_rebuilt) {
                0..=1 => 0,
                2..=4 => 1,
                _ => 2,
            };
            arrivals[bucket].take(cost, &moved);
        }
    }
}

fn main() {
    zgui_profile::latency::start_epoch();
    println!(
        "This is a diagnostic probe and not a reference workload: it answers what one glide \
         frame costs over a diff-shaped virtual list, on this machine, with its own fonts."
    );

    let mut harness = opened();
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action: PointerAction::Moved,
        event: PointerEvent::mouse(middle()),
        modifiers: Modifiers::default(),
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(64);
    zgui_profile::counter::reset();

    // The reading glide: a notch every fortieth tick, so the list is genuinely moving throughout.
    let mut translation = Bucket::default();
    let mut arrivals: [Bucket; 3] = Default::default();
    drive(
        &mut harness,
        1_200,
        40,
        3.0,
        &mut translation,
        &mut arrivals,
    );
    println!("\nreading glide (3-line notches, 120 Hz):");
    translation.report("translation (0 rows)");
    arrivals[0].report("recycle, 1 row");
    arrivals[1].report("recycle, 2-4 rows");
    arrivals[2].report("recycle, 5+ rows");

    // The fling: notches stacked every fourth tick, the glide never allowed to decay.
    let mut translation = Bucket::default();
    let mut arrivals: [Bucket; 3] = Default::default();
    drive(
        &mut harness,
        1_200,
        4,
        20.0,
        &mut translation,
        &mut arrivals,
    );
    println!("\nfling (20-line notches every 4 ticks):");
    translation.report("translation (0 rows)");
    arrivals[0].report("recycle, 1 row");
    arrivals[1].report("recycle, 2-4 rows");
    arrivals[2].report("recycle, 5+ rows");

    harness.shut_down();
    zgui_profile::latency::flush();
}
