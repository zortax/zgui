//! Static vector documents drawn as CPU layers by a real window.
//!
//! The paint stage's tests prove the route and the records. These prove the window: that a page of
//! such documents never hands the renderer a general vector item, that a zoom settles into exact
//! rasters through the frames the paint report asks for, and that the window parks afterwards.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use zgui_bits::DamageSet;
use zgui_platform::Surface;
use zgui_profile::Counter;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle, VectorBackend, VectorStatus,
};
use zgui_runtime::{App, AppError};
use zgui_scene::Scene;
use zgui_testkit_scene::counters::Recording;
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// One frame, reduced to what these cases ask about.
#[derive(Clone, Copy, Debug)]
struct Frame {
    /// General vector items in the display list.
    vectors: usize,
    /// Colour sprites in the display list.
    sprites: usize,
}

/// The frames a run produced.
type Log = Rc<RefCell<Vec<Frame>>>;

/// A renderer that records every frame and draws nothing.
struct Recorder {
    log: Log,
    target: Option<RenderTarget>,
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for Recorder {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn draw(&mut self, scene: &Scene, _damage: &DamageSet) -> FrameOutcome {
        self.log.borrow_mut().push(Frame {
            vectors: scene.primitives.vectors.len(),
            sprites: scene.primitives.color_sprites.len(),
        });
        FrameOutcome::Presented(zgui_render::FrameStats {
            vector_passes: 0,
            draw_calls: 0,
            damage_px: 0,
            bytes_uploaded: 0,
            memory: MemoryReport::ZERO,
        })
    }

    fn register_external(&mut self, _texture: ExternalTexture) -> TextureHandle {
        TextureHandle(0)
    }

    fn release_external(&mut self, _handle: TextureHandle) {}

    fn memory(&self) -> MemoryReport {
        MemoryReport::ZERO
    }

    fn vector_status(&self) -> VectorStatus {
        VectorStatus {
            backend: Some(VectorBackend::Vello),
            initialized: false,
        }
    }

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }
}

/// Mounts a document styled by `css`, with every frame recorded into `log`.
fn mount(
    css: &'static str,
    log: &Log,
    view: impl FnMut(&mut BuildCx<'_>) -> Box<dyn Anchor> + 'static,
) -> zgui_platform_headless::Harness<zgui_runtime::Runtime> {
    let factory = Rc::clone(log);
    let handler = App::new()
        .with_title("layers")
        .with_size(800.0, 600.0)
        .with_stylesheet(css)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Recorder {
                    log: Rc::clone(&factory),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .into_handler(view)
        .expect("the reactive runtime installs");
    zgui_platform_headless::Harness::new(handler)
}

/// One refresh at 120 Hz.
const TICK: Duration = Duration::from_micros(8_333);

/// Four 96 unit documents, each with a ramp, a clip and solid shapes, as the static bench draws.
const SOURCES: [&str; 4] = [
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#4f8cff"/><stop offset="1" stop-color="#1b2a5c"/></linearGradient><clipPath id="c"><circle cx="48" cy="48" r="30"/></clipPath></defs><path d="M4 4 L92 4 L48 92 Z" fill="url(#a)"/><g clip-path="url(#c)"><rect x="20" y="40" width="56" height="16" fill="#06d6a0"/></g></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><radialGradient id="b" cx="0.3" cy="0.3" r="0.7"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#264653"/></radialGradient></defs><path d="M8 88 L48 20 L88 88 Z" fill="url(#b)"/><rect x="40" y="70" width="16" height="16" fill="#f4a261"/></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#8338ec"/><stop offset="1" stop-color="#3a86ff"/></linearGradient><clipPath id="c"><path d="M48 8 L88 48 L48 88 L8 48 Z"/></clipPath></defs><g clip-path="url(#c)"><rect x="0" y="0" width="96" height="96" fill="url(#a)"/></g></svg>"##,
    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 96 96"><defs><linearGradient id="a" x1="1" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ef233c"/><stop offset="1" stop-color="#2b2d42"/></linearGradient><clipPath id="c"><circle cx="48" cy="40" r="24"/></clipPath></defs><g clip-path="url(#c)"><path d="M0 40 L96 40 L96 96 L0 96 Z" fill="url(#a)"/></g><circle cx="20" cy="20" r="6" fill="#2b2d42"/></svg>"##,
];

const CSS: &str = "root { display: block; width: 800px; height: 600px }
     .port { display: flex; flex-direction: column; gap: 4px; width: 800px; height: 600px;
             transform-origin: 0 0 }
     .row { display: flex; flex-direction: row; gap: 4px; flex: none }
     .art { display: block; width: 96px; height: 96px; flex: none }";

/// Three rows of four documents, the port scaled by `zoom`.
fn grid(cx: &mut BuildCx<'_>, zoom: RwSignal<f64>) -> Box<dyn Anchor> {
    let mut port = zgui_elements::r#box()
        .class("port")
        .style_property("transform", move || {
            let zoom = zoom.get();
            (zoom != 1.0).then(|| format!("scale({zoom})"))
        });
    for line in 0..3 {
        let mut row = zgui_elements::r#box().class("row");
        for index in 0..4 {
            row = row.child(
                zgui_elements::vector()
                    .class("art")
                    .document(SOURCES[(line + index) % SOURCES.len()]),
            );
        }
        port = port.child(row);
    }
    Box::new(port.into_view().build(cx))
}

fn rasterised() -> u64 {
    zgui_profile::counter::get(Counter::VectorLayersRasterised)
}

#[test]
fn a_static_svg_grid_draws_layers_and_no_general_item() {
    let _recording = Recording::begin();
    let log: Log = Rc::default();
    let zoom = RwSignal::new(1.0);
    let mut harness = mount(CSS, &log, move |cx| grid(cx, zoom));
    harness.settle(8);
    let frames = log.borrow();
    assert!(!frames.is_empty(), "the grid drew no frame");
    assert!(
        frames.iter().all(|frame| frame.vectors == 0),
        "a general vector item reached the renderer: {frames:?}"
    );
    assert_eq!(frames.last().map(|frame| frame.sprites), Some(12));
    assert_eq!(rasterised(), 4, "one layer per source");
    assert_eq!(zgui_profile::counter::get(Counter::VectorRouteGeneral), 0);
}

#[test]
fn a_provisional_layer_settles_and_the_window_parks() {
    let _recording = Recording::begin();
    let log: Log = Rc::default();
    let zoom = RwSignal::new(1.0);
    let mut harness = mount(CSS, &log, move |cx| grid(cx, zoom));
    harness.settle(8);
    assert_eq!(rasterised(), 4);

    // A zoom over ten frames stretches the rasters it has.
    for step in 1..=10 {
        zoom.set(1.0 + 0.05 * f64::from(step));
        let before = rasterised();
        harness.advance(TICK);
        harness.pump();
        assert!(rasterised() - before <= 3, "frame {step} rasterised too much");
    }
    assert_eq!(rasterised(), 4, "nothing is rasterised while the zoom moves");

    // Once it stops, each key rasterises once, a few at a time, and then nothing is asked for.
    let stopped = log.borrow().len();
    let mut per_tick = Vec::new();
    for _ in 0..12 {
        let before = rasterised();
        harness.advance(TICK);
        harness.pump();
        per_tick.push(rasterised() - before);
    }
    assert!(per_tick.iter().all(|count| *count <= 3), "{per_tick:?}");
    assert_eq!(per_tick.iter().sum::<u64>(), 4, "{per_tick:?}");
    let drawn = log.borrow().len() - stopped;
    assert!(drawn <= 8, "{drawn} frames after the zoom stopped");
    let parked = log.borrow().len();
    for _ in 0..8 {
        harness.advance(TICK);
        harness.pump();
    }
    assert_eq!(log.borrow().len(), parked, "the window keeps drawing");
    assert!(log.borrow().iter().all(|frame| frame.vectors == 0));
}

#[test]
fn layer_bytes_stay_within_the_budget() {
    use zgui_geom::{Css, CssPx, Point};
    use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

    const PORT: &str = "root { display: block; width: 800px; height: 600px }
         .port { display: flex; flex-direction: column; gap: 4px; width: 800px; height: 600px;
                 overflow: auto }
         .art { display: block; width: 256px; height: 256px; flex: none }";
    let _recording = Recording::begin();
    let log: Log = Rc::default();
    let mut harness = mount(PORT, &log, |cx| {
        let mut port = zgui_elements::r#box().class("port");
        for index in 0..200_u32 {
            let source = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#{index:06x}"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><path d="M0 0 L32 0 L0 32 Z" fill="url(#a)"/></svg>"##
            );
            port = port.child(zgui_elements::vector().class("art").document(&source));
        }
        Box::new(port.into_view().build(cx))
    });
    harness.settle(8);
    // Four surfaces of 800 by 600 is less than the floor.
    let limit = 16 * 1024 * 1024;
    let mut highest = 0;
    for _ in 0..110 {
        harness.deliver_to_first(zgui_platform::SurfaceEvent::Wheel {
            event: WheelEvent {
                id: zgui_vocab::PointerId::MOUSE,
                kind: zgui_vocab::PointerKind::Mouse,
                position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(300.0)),
                delta: ScrollDelta::Pixels(zgui_geom::Size::new(CssPx(0.0), CssPx(520.0))),
                phase: ScrollPhase::Discrete,
            },
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        for _ in 0..4 {
            harness.advance(TICK);
            harness.pump();
            let live = zgui_profile::counter::get(Counter::VectorLayerBytesLive);
            highest = highest.max(live);
            assert!(live <= limit, "{live} layer bytes held against a level of {limit}");
        }
    }
    assert!(rasterised() >= 150, "the scroll reached {} drawings", rasterised());
    assert!(zgui_profile::counter::get(Counter::VectorLayersEvicted) > 0);
    assert!(highest > limit / 2, "the budget was never approached: {highest}");
}
