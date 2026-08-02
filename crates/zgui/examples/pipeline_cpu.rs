//! The frame pipeline's CPU cost, driven in process against the headless platform.
//!
//! The gallery of the `styled` example, its real font engine and its real glyph rasteriser, with a
//! renderer that accepts frames and draws nowhere. Everything from the platform event to the
//! finished display list therefore runs exactly as it does in a window, and the run is scriptable
//! and profilable without a display server.
//!
//! Run it as `cargo run -p zgui --release --example pipeline_cpu -- <phase> <repeats>`, where the
//! phase is one of `idle`, `hover`, `click` or `resize`.

use std::sync::Arc;

use zgui::prelude::*;
use zgui_geom::{CssPx, Device, DevicePx, Point, Size};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_platform_headless::Harness;
use zgui_render::{RenderTarget, Renderer};
use zgui_runtime::{AppError, Runtime};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Timestamp};

/// One panel of the gallery, with a heading above whatever it is showing.
#[component]
fn Panel(
    /// What the panel is called.
    #[prop(into)]
    title: String,
    /// One line about what it shows.
    #[prop(into)]
    note: String,
    /// What it shows.
    children: Children,
) -> impl IntoView {
    view! {
        column(class = "panel") {
            label(class = "panel__title") {{title}}
            label(class = "panel__note") {{note}}
            box(class = "panel__body") {{children.into_view_once()}}
        }
    }
}

/// The gallery.
#[component]
fn Gallery() -> impl IntoView {
    let picked = RwSignal::new(2_usize);

    view! {
        column(class = "page") {
            row(class = "masthead") {
                column(class = "masthead__text") {
                    label(class = "masthead__title") {"zgui"}
                    label(class = "masthead__subtitle") {
                        "components, signals and CSS, in a real window"
                    }
                }
                spacer()
                box(class = "badge") {"gallery"}
            }

            box(class = "grid") {
                Panel(title = "Grid", note = "three tracks, one gap") {
                    box(class = "tiles") {
                        box(class = "tile tile--a")
                        box(class = "tile tile--b")
                        box(class = "tile tile--c")
                        box(class = "tile tile--c")
                        box(class = "tile tile--a")
                        box(class = "tile tile--b")
                    }
                }

                Panel(title = "Gradients", note = "linear and radial, with stops") {
                    column(class = "ramps") {
                        box(class = "ramp ramp--linear")
                        box(class = "ramp ramp--radial")
                        box(class = "ramp ramp--angled")
                    }
                }

                Panel(title = "Shadows", note = "cast, inset and a lifted card") {
                    row(class = "shadows") {
                        box(class = "chip chip--cast")
                        box(class = "chip chip--inset")
                        box(class = "chip chip--lifted")
                    }
                }

                Panel(title = "Corners and borders", note = "per-corner radii and dashed edges") {
                    row(class = "corners") {
                        box(class = "corner corner--pill")
                        box(class = "corner corner--leaf")
                        box(class = "corner corner--dashed")
                    }
                }

                Panel(title = "Transforms", note = "rotate, scale and skew about a chosen origin") {
                    row(class = "transforms") {
                        box(class = "turn turn--rotate")
                        box(class = "turn turn--scale")
                        box(class = "turn turn--skew")
                    }
                }

                Panel(title = "Filters", note = "blur, saturate and opacity") {
                    row(class = "filters") {
                        box(class = "lens lens--blur")
                        box(class = "lens lens--saturate")
                        box(class = "lens lens--faded")
                    }
                }

                Panel(title = "Type", note = "size, weight, spacing and decoration") {
                    column(class = "type") {
                        text(class = "type__display") {"Aa"}
                        text(class = "type__body") {"The quick brown fox jumps over the lazy dog."}
                        row(class = "type__row") {
                            text(class = "type__thin") {"thin"}
                            text(class = "type__bold") {"bold"}
                            text(class = "type__wide") {"w i d e"}
                            text(class = "type__struck") {"struck"}
                        }
                    }
                }

                Panel(title = "State", note = "a class toggled by a signal, and :hover in the sheet") {
                    row(class = "swatches") {
                        for index in || [0_usize, 1, 2, 3], key = |index: &usize| *index {
                            control(
                                class = "swatch",
                                class:swatch-picked = move || picked.get() == index,
                                a11y:label = "Swatch",
                                on:click = move |_| picked.set(index)
                            )
                        }
                    }
                }
            }
        }
    }
}

/// How it looks. This is the example.
const SHEET: &str = css!(
    ":root {
        background-color: #0b0d12;
        color: #e9edf6;
        font-family: sans-serif;
        overflow: auto;
    }

    .page { gap: 20px; padding: 28px 32px; }

    .masthead { align-items: center; gap: 16px; }
    .masthead__text { gap: 4px; }
    .masthead__title {
        font-size: 30px;
        font-weight: 700;
        letter-spacing: -1px;
    }
    .masthead__subtitle { font-size: 14px; color: #79839a; }

    .badge {
        padding: 6px 14px;
        border-radius: 999px;
        font-size: 12px;
        letter-spacing: 1px;
        color: #0b0d12;
        background-image: linear-gradient(100deg, #7ee3ff, #7d8bff 55%, #d18bff);
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(4, 1fr);
        gap: 16px;
    }

    .panel {
        gap: 2px;
        padding: 16px;
        border-radius: 14px;
        border: 1px solid #1e2531;
        background-color: #12161e;
        box-shadow: 0 10px 24px rgba(0, 0, 0, 0.35);
    }

    .panel__title { font-size: 15px; font-weight: 700; }
    .panel__note { font-size: 11px; color: #79839a; }
    .panel__body { padding-top: 12px; }

    .tiles {
        display: grid;
        grid-template-columns: repeat(3, 1fr);
        gap: 6px;
    }
    .tile { height: 34px; border-radius: 6px; }
    .tile--a { background-color: #2f6bff; }
    .tile--b { background-color: #24d3a5; }
    .tile--c { background-color: #ff8a5b; }

    .ramps { gap: 8px; }
    .ramp { height: 24px; border-radius: 8px; }
    .ramp--linear { background-image: linear-gradient(90deg, #2f6bff, #d18bff); }
    .ramp--radial {
        background-image: radial-gradient(circle at 30% 50%, #ffd36e, #ff6b6b 70%);
    }
    .ramp--angled {
        background-image: linear-gradient(135deg, #24d3a5 0%, #2f6bff 50%, #12161e 100%);
    }

    .shadows, .corners, .transforms, .filters { gap: 14px; align-items: center; }

    .chip { width: 46px; height: 46px; border-radius: 12px; background-color: #2f6bff; }
    .chip--cast { box-shadow: 0 10px 18px rgba(47, 107, 255, 0.55); }
    .chip--inset {
        background-color: #1a2130;
        box-shadow: inset 0 6px 12px rgba(0, 0, 0, 0.8);
    }
    .chip--lifted {
        background-color: #24d3a5;
        box-shadow: 0 2px 0 #12161e, 0 16px 26px rgba(36, 211, 165, 0.4);
    }

    .corner { width: 52px; height: 46px; background-color: #232b3a; }
    .corner--pill { border-radius: 999px; }
    .corner--leaf { border-radius: 22px 4px 22px 4px; }
    .corner--dashed {
        border: 2px dashed #4a5670;
        border-radius: 10px;
        background-color: transparent;
    }

    .turn { width: 44px; height: 44px; border-radius: 8px; background-color: #d18bff; }
    .turn--rotate { transform: rotate(18deg); }
    .turn--scale { transform: scale(1.25); transform-origin: center; }
    .turn--skew { transform: skewX(-14deg); background-color: #7ee3ff; }

    .lens {
        width: 46px;
        height: 46px;
        border-radius: 10px;
        background-image: linear-gradient(45deg, #ff6b6b, #ffd36e);
    }
    .lens--blur { filter: blur(3px); }
    .lens--saturate { filter: saturate(0.25); }
    .lens--faded { opacity: 0.45; }

    .type { gap: 6px; }
    .type__display { font-size: 40px; font-weight: 700; line-height: 1; }
    .type__body { font-size: 13px; color: #b9c2d4; }
    .type__row { gap: 10px; }
    .type__thin { font-size: 12px; font-weight: 300; }
    .type__bold { font-size: 12px; font-weight: 800; }
    .type__wide { font-size: 12px; letter-spacing: 2px; }
    .type__struck { font-size: 12px; text-decoration: line-through; }

    .swatches { gap: 10px; }
    .swatch {
        width: 34px;
        height: 34px;
        border-radius: 10px;
        border: 2px solid transparent;
        background-color: #232b3a;
    }
    .swatch:hover { background-color: #2c3546; }
    .swatch-picked {
        border-color: #7ee3ff;
        background-color: #2f6bff;
    }"
);

/// A texture sink that accepts every upload and holds nothing.
struct NullSink;

impl zgui_atlas::TextureSink for NullSink {
    fn create_texture(
        &mut self,
        _texture: zgui_atlas::TextureId,
        _size: Size<i32, Device>,
        _format: zgui_atlas::TextureFormat,
    ) -> Result<(), zgui_atlas::SinkError> {
        Ok(())
    }

    fn write_texture(
        &mut self,
        _texture: zgui_atlas::TextureId,
        _bounds: zgui_geom::Rect<i32, Device>,
        _format: zgui_atlas::TextureFormat,
        _bytes: &[u8],
    ) -> Result<(), zgui_atlas::SinkError> {
        Ok(())
    }

    fn destroy_texture(&mut self, _texture: zgui_atlas::TextureId) {}
}

/// A renderer that accepts a frame and does nothing with it.
///
/// It exists so that a profile is the pipeline's own cost: a renderer that recorded the display
/// list would put its own serialisation into every measurement.
struct NullRenderer {
    /// The surface it was configured for.
    target: Option<RenderTarget>,
    /// Where tiles are uploaded.
    sink: NullSink,
    /// The next handle an external texture is given.
    next: u64,
}

impl Renderer for NullRenderer {
    fn capabilities(&self) -> zgui_render::RenderCapabilities {
        zgui_render::RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(
        &mut self,
        _scene: &zgui_scene::Scene,
        _damage: &zgui_bits::DamageSet,
    ) -> zgui_render::FrameOutcome {
        zgui_render::FrameOutcome::Presented(zgui_render::FrameStats::default())
    }

    fn register_external(
        &mut self,
        _texture: zgui_render::ExternalTexture,
    ) -> zgui_render::TextureHandle {
        self.next += 1;
        zgui_render::TextureHandle(self.next)
    }

    fn release_external(&mut self, _handle: zgui_render::TextureHandle) {}

    fn memory(&self) -> zgui_render::MemoryReport {
        zgui_render::MemoryReport::ZERO
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.sink
    }
}

/// Builds the renderer a window draws through.
fn renderer(
    _surface: &Arc<dyn Surface>,
    target: RenderTarget,
) -> Result<Box<dyn Renderer>, AppError> {
    Ok(Box::new(NullRenderer {
        target: Some(target),
        sink: NullSink,
        next: 0,
    }))
}

/// The centre of every 34x34 box in the window, which is what the swatches are.
fn swatch_centres(window: &zgui_runtime::Window) -> Vec<Point<CssPx, zgui_geom::Css>> {
    let layout = window.layout().borrow();
    let mut found = Vec::new();
    for key in layout.keys() {
        for fragment in layout.fragments_of_box(key) {
            let Some(fragment) = layout.fragment(*fragment) else {
                continue;
            };
            let border: zgui_geom::Rect<DevicePx, Device> = fragment.border_box;
            let width = border.size.width.0;
            let height = border.size.height.0;
            if (width - 34.0).abs() < 0.5 && (height - 34.0).abs() < 0.5 {
                found.push(Point::new(
                    CssPx(border.origin.x.0 + width / 2.0),
                    CssPx(border.origin.y.0 + height / 2.0),
                ));
            }
        }
    }
    found.sort_by(|a, b| a.x.0.total_cmp(&b.x.0));
    found
}

/// One pointer event at `at`.
fn pointer(action: PointerAction, at: Point<CssPx, zgui_geom::Css>) -> SurfaceEvent {
    SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(at),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    }
}

fn main() {
    let phase = std::env::args().nth(1).unwrap_or_else(|| "click".into());
    let repeats: usize = std::env::args()
        .nth(2)
        .and_then(|n| n.parse().ok())
        .unwrap_or(48);

    zgui_profile::latency::start_epoch();

    let fonts = Fonts::system();
    let metrics = fonts.clone();
    let shaping = fonts.clone();
    let raster = fonts.clone();
    let runtime: Runtime = zgui_runtime::App::new()
        .with_title("pipeline-cpu")
        .with_size(1080.0, 720.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(renderer))
        .with_metrics(Box::new(move || metrics.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .with_glyph_raster(Box::new(move || raster.raster()))
        .into_handler(|cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(view! { Gallery() }.into_view().build(cx))
        })
        .expect("the reactive runtime installs");

    let mut harness = Harness::new(runtime);
    harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
        DevicePx(1080.0),
        DevicePx(720.0),
    )));
    harness.settle(64);

    {
        let window = &harness.app().windows()[0];
        let layout = window.layout().borrow();
        let keys = layout.keys();
        let boxes = keys.len();
        let fragments: usize = keys.iter().map(|k| layout.fragments_of_box(*k).len()).sum();
        println!("document boxes={boxes} fragments={fragments}");
    }
    let centres = swatch_centres(&harness.app().windows()[0]);
    assert_eq!(
        centres.len(),
        4,
        "the four swatches were found: {centres:?}"
    );
    harness.reset_counts();

    let started = std::time::Instant::now();
    let frames = match phase.as_str() {
        "idle" => {
            let mut frames = 0;
            for _ in 0..repeats {
                harness.advance(std::time::Duration::from_millis(16));
                frames += harness.pump();
            }
            frames
        }
        "hover" => {
            let mut frames = 0;
            for index in 0..repeats {
                let at = centres[index % centres.len()];
                harness.deliver_to_first(pointer(PointerAction::Moved, at));
                frames += harness.settle(32);
                harness.deliver_to_first(pointer(
                    PointerAction::Moved,
                    Point::new(CssPx(4.0), CssPx(4.0)),
                ));
                frames += harness.settle(32);
            }
            frames
        }
        "click" => {
            let mut frames = 0;
            for index in 0..repeats {
                let at = centres[index % centres.len()];
                harness.deliver_to_first(pointer(PointerAction::Moved, at));
                frames += harness.settle(32);
                harness.deliver_to_first(pointer(PointerAction::Pressed, at));
                frames += harness.settle(32);
                harness.deliver_to_first(pointer(PointerAction::Released, at));
                frames += harness.settle(32);
            }
            frames
        }
        "resize" => {
            let mut frames = 0;
            for index in 0..repeats {
                let width = 1080.0 + (index % 24) as f32 * 8.0;
                harness.deliver_to_first(SurfaceEvent::Resized(Size::new(
                    DevicePx(width),
                    DevicePx(720.0),
                )));
                frames += harness.settle(32);
            }
            frames
        }
        other => panic!("unknown phase {other}"),
    };
    let elapsed = started.elapsed();

    zgui_profile::latency::flush();
    println!(
        "phase={phase} repeats={repeats} frames={frames} wall_ms={:.3} per_frame_ms={:.4}",
        elapsed.as_secs_f64() * 1000.0,
        elapsed.as_secs_f64() * 1000.0 / frames.max(1) as f64
    );
}
