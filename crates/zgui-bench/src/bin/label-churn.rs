//! A diagnostic probe: thirty axis labels whose strings change on every frame.
//!
//! A plot that zooms relabels its ticks on most frames. Every label is then a shaping miss, and
//! what decides the frame is the fixed cost of one short paragraph — and whether handing thirty
//! of them to the layout workers costs more than shaping them on the frame thread.
//!
//! ```text
//! cargo run --release -p zgui-bench --bin label-churn
//! ZGUI_LAYOUT_THREADS=0 cargo run --release -p zgui-bench --bin label-churn
//! cargo run --release -p zgui-bench --bin label-churn -- 120
//! ```
//!
//! It prints the median wall time of one relabelled frame. It gates nothing.

#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

use zgui::app::Fonts;
use zgui::geom::{DevicePx, Size};
use zgui::platform::SurfaceEvent;
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{App, AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui_bench::reference::sample;
use zgui_platform_headless::Harness;

/// How wide the window opens, in CSS pixels.
const WIDTH: f32 = 1000.0;

/// How tall it opens.
const HEIGHT: f32 = 700.0;

/// Labels on the two axes together, unless the command line names another count.
const LABELS: usize = 30;

/// How many labels this run relabels.
fn labels() -> usize {
    std::env::args()
        .nth(1)
        .and_then(|count| count.parse().ok())
        .unwrap_or(LABELS)
}

/// The sheet: a plot area with absolutely positioned tick labels along two edges.
const SHEET: &str = zgui::css!(
    ":root { background-color: #14161a; color: #e7ecf5; font-family: sans-serif; font-size: 11px }
     .plot { position: relative; width: 900px; height: 600px }
     .tick { position: absolute }"
);

/// Where each label sits: the first half along the bottom edge, the rest up the left edge.
fn placements() -> String {
    let labels = labels();
    (0..labels)
        .map(|index| {
            let (left, top) = if index < labels / 2 {
                (40 + index * 56 % 840, 580)
            } else {
                (4, (index - labels / 2) * 38 % 570)
            };
            format!(".tick-{index} {{ left: {left}px; top: {top}px }}\n")
        })
        .collect()
}

/// A texture sink that accepts every upload and holds nothing.
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

/// A renderer that accepts a frame and does nothing with it, so the number is the pipeline's.
struct Nowhere {
    /// The surface it was configured for.
    target: Option<RenderTarget>,
    /// Where tiles are uploaded.
    sink: NullSink,
}

impl Renderer for Nowhere {
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
        zgui::render::TextureHandle(0)
    }

    fn release_external(&mut self, _handle: zgui::render::TextureHandle) {}

    fn memory(&self) -> zgui::render::MemoryReport {
        zgui::render::MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui::atlas::TextureSink {
        &mut self.sink
    }
}

/// Builds the renderer a window draws through.
fn nowhere(
    _surface: &std::sync::Arc<dyn zgui::platform::Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    Ok(Box::new(Nowhere {
        target: Some(target),
        sink: NullSink,
    }))
}

/// The text of one label at one zoom step, different at every step and from every other label.
fn label(zoom: usize, index: usize) -> String {
    let value = ((zoom * 7_919 + index * 104_729) % 200_000) as f64 / 1_000.0 - 100.0;
    format!("{value:.2}")
}

/// Opens the plot, settled and warm.
fn opened(zoom: RwSignal<usize, LocalStorage>) -> Harness<Runtime> {
    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let handler = App::new()
        .with_title("label-churn")
        .with_size(WIDTH, HEIGHT)
        .with_stylesheet(format!("{SHEET}\n{}", placements()))
        .with_renderer(Box::new(nowhere))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            let mut plot = zgui::elements::column().class("plot");
            for index in 0..labels() {
                plot = plot.child(
                    zgui::elements::text()
                        .class("tick")
                        .class(format!("tick-{index}"))
                        .child(move || label(zoom.get(), index)),
                );
            }
            Box::new(plot.into_view().build(cx))
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

fn main() {
    let zoom = RwSignal::new_local(0_usize);
    let mut harness = opened(zoom);
    let mut step = 0;
    let mut frame = |_| {
        step += 1;
        let started = Instant::now();
        zoom.set(step);
        harness.settle(64);
        started.elapsed()
    };
    let rounds: Vec<f64> = (0..5).map(|_| sample::median_ns(&mut frame)).collect();
    let mut sorted = rounds.clone();
    sorted.sort_by(f64::total_cmp);
    println!(
        "label-churn: {} labels relabelled, {:.1} us/frame median of round medians ({})",
        labels(),
        sorted[sorted.len() / 2] / 1e3,
        rounds
            .iter()
            .map(|ns| format!("{:.1}", ns / 1e3))
            .collect::<Vec<_>>()
            .join(" "),
    );
}
