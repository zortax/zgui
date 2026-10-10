//! Static vector documents drawn as CPU layers by a real window.
//!
//! The paint stage's tests prove the route and the records. These prove the window: that a page of
//! such documents never hands the renderer a general vector item, that a zoom settles into exact
//! rasters through the frames the paint report asks for, that the window parks afterwards, and
//! that a drawing owed before a scroll shift is drawn where the shift moved it.

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
        assert!(
            rasterised() - before <= 3,
            "frame {step} rasterised too much"
        );
    }
    assert_eq!(
        rasterised(),
        4,
        "nothing is rasterised while the zoom moves"
    );

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
            assert!(
                live <= limit,
                "{live} layer bytes held against a level of {limit}"
            );
        }
    }
    assert!(
        rasterised() >= 150,
        "the scroll reached {} drawings",
        rasterised()
    );
    assert!(zgui_profile::counter::get(Counter::VectorLayersEvicted) > 0);
    assert!(
        highest > limit / 2,
        "the budget was never approached: {highest}"
    );
}

/// What a [`Composer`] found and did.
#[derive(Default)]
struct Composed {
    /// Which pixels of the composed target hold a drawing.
    inked: Vec<bool>,
    width: i32,
    height: i32,
    /// Pixels of a sprite in the display list that the target shows blank.
    blank: usize,
    /// When set, every pixel of every sprite is checked, damaged or not.
    audit: bool,
    shifts: usize,
}

impl Composed {
    fn at(&self, x: i32, y: i32) -> usize {
        (y * self.width + x) as usize
    }

    /// Every pixel of `rect` inside the target.
    fn each(&self, rect: [i32; 4]) -> impl Iterator<Item = (i32, i32)> + use<> {
        let [left, top, right, bottom] = [
            rect[0].max(0),
            rect[1].max(0),
            rect[2].min(self.width),
            rect[3].min(self.height),
        ];
        (top..bottom).flat_map(move |y| (left..right).map(move |x| (x, y)))
    }
}

/// A renderer that keeps a composed target of one bit per pixel: whether a drawing is on it.
struct Composer {
    state: Rc<RefCell<Composed>>,
    target: Option<RenderTarget>,
    atlas: zgui_atlas::MemorySink,
}

impl Renderer for Composer {
    fn capabilities(&self) -> RenderCapabilities {
        RenderCapabilities::MINIMAL
    }

    fn configure(&mut self, target: RenderTarget) {
        let mut state = self.state.borrow_mut();
        state.width = target.size.width;
        state.height = target.size.height;
        state.inked = vec![false; (target.size.width * target.size.height) as usize];
        self.target = Some(target);
    }

    fn target(&self) -> Option<RenderTarget> {
        self.target
    }

    fn shifts_composed_pixels(&self) -> bool {
        true
    }

    fn shift_composed(&mut self, shift: zgui_render::ScrollShift) {
        let mut state = self.state.borrow_mut();
        state.shifts += 1;
        let (Some(from), Some(to)) = (shift.source(), shift.destination()) else {
            return;
        };
        let before = state.inked.clone();
        for dy in 0..to.size.height {
            for dx in 0..to.size.width {
                let read = state.at(from.origin.x + dx, from.origin.y + dy);
                let write = state.at(to.origin.x + dx, to.origin.y + dy);
                state.inked[write] = before[read];
            }
        }
    }

    fn draw(&mut self, scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        let mut state = self.state.borrow_mut();
        let surface = [0, 0, state.width, state.height];
        let damaged: Vec<[i32; 4]> = if damage.is_full() {
            vec![surface]
        } else {
            damage
                .rects()
                .iter()
                .map(|rect| {
                    [
                        rect.origin.x,
                        rect.origin.y,
                        rect.origin.x + rect.size.width,
                        rect.origin.y + rect.size.height,
                    ]
                })
                .collect()
        };
        let inside = |x: i32, y: i32| {
            damaged
                .iter()
                .any(|rect| x >= rect[0] && y >= rect[1] && x < rect[2] && y < rect[3])
        };
        let sprites: Vec<[i32; 4]> = scene
            .primitives
            .color_sprites
            .iter()
            .map(|sprite| {
                let [x, y, width, height] = sprite.bounds;
                let (dx, dy) =
                    scene
                        .spatial
                        .resolve_at(sprite.transform)
                        .map_or((0.0, 0.0), |matrix| {
                            let [x, y, _] = matrix.transform_point(0.0, 0.0, 0.0);
                            (x, y)
                        });
                [
                    (x + dx).round() as i32,
                    (y + dy).round() as i32,
                    (x + dx + width).round() as i32,
                    (y + dy + height).round() as i32,
                ]
            })
            .collect();
        // What the target shows where the display list says a drawing is, before this frame
        // draws over the damage.
        for sprite in &sprites {
            for (x, y) in state.each(*sprite) {
                if (state.audit || !inside(x, y)) && !state.inked[state.at(x, y)] {
                    state.blank += 1;
                }
            }
        }
        for rect in &damaged {
            for (x, y) in state.each(*rect) {
                let at = state.at(x, y);
                state.inked[at] = false;
            }
        }
        for sprite in &sprites {
            for (x, y) in state.each(*sprite) {
                if inside(x, y) {
                    let at = state.at(x, y);
                    state.inked[at] = true;
                }
            }
        }
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

/// A zigzag of `teeth` teeth across a 32 unit document, filled with a ramp ending in `colour`.
///
/// The layer cache estimates two microseconds for each path element, so a few of these revealed at
/// once exceed the budget that a cold rasteriser gives first rasters.
fn zigzag(colour: u32, teeth: usize) -> String {
    use std::fmt::Write as _;
    let mut path = String::from("M0 32");
    for tooth in 0..teeth {
        let x = 32.0 * tooth as f64 / teeth as f64;
        let _ = write!(path, " L{x:.3} {}", if tooth % 2 == 0 { 0 } else { 4 });
    }
    path.push_str(" L32 32 Z");
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ffffff"/><stop offset="1" stop-color="#{colour:06x}"/></linearGradient></defs><path d="{path}" fill="url(#a)"/></svg>"##
    )
}

#[test]
fn a_drawing_owed_before_a_scroll_shift_is_drawn_where_the_shift_put_it() {
    use zgui_geom::{Css, CssPx, Point};
    use zgui_vocab::{Modifiers, ScrollDelta, ScrollPhase, Timestamp, WheelEvent};

    const PORT: &str = "root { display: block; width: 800px; height: 600px }
         .port { display: flex; flex-direction: column; gap: 4px; width: 800px; height: 600px;
                 overflow: auto; background-color: #101010 }
         .art { display: block; width: 60px; height: 60px; flex: none }";
    let _recording = Recording::begin();
    let state: Rc<RefCell<Composed>> = Rc::default();
    let factory = Rc::clone(&state);
    let lit = RwSignal::new(false);
    let handler = App::new()
        .with_title("layers")
        .with_size(800.0, 600.0)
        .with_stylesheet(PORT)
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = Composer {
                    state: Rc::clone(&factory),
                    target: None,
                    atlas: zgui_atlas::MemorySink::default(),
                };
                renderer.configure(target);
                Ok::<Box<dyn Renderer>, AppError>(Box::new(renderer))
            },
        ))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            let mut port = zgui_elements::r#box()
                .class("port")
                .style_property("background-color", move || {
                    lit.get().then(|| "#202020".to_owned())
                });
            for index in 0..120_u32 {
                port = port.child(
                    zgui_elements::vector()
                        .class("art")
                        .document(&zigzag(index * 0x02_03_07, 1_400)),
                );
            }
            Box::new(port.into_view().build(cx))
        })
        .expect("the reactive runtime installs");
    let mut harness = zgui_platform_headless::Harness::new(handler);
    harness.settle(16);

    let deferred = zgui_profile::counter::get(Counter::VectorLayersDeferred);
    for _ in 0..12 {
        harness.deliver_to_first(zgui_platform::SurfaceEvent::Wheel {
            event: WheelEvent {
                id: zgui_vocab::PointerId::MOUSE,
                kind: zgui_vocab::PointerKind::Mouse,
                position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(300.0)),
                delta: ScrollDelta::Pixels(zgui_geom::Size::new(CssPx(0.0), CssPx(256.0))),
                phase: ScrollPhase::Discrete,
            },
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        harness.advance(TICK);
        harness.pump();
    }
    // A second for the glide to finish.
    for _ in 0..120 {
        harness.advance(TICK);
        harness.pump();
    }
    assert!(
        zgui_profile::counter::get(Counter::VectorLayersDeferred) > deferred,
        "the scroll deferred no drawing, so nothing was owed across a shift"
    );
    assert!(
        state.borrow().shifts > 0,
        "no frame of the scroll was a shift"
    );
    assert_eq!(
        state.borrow().blank,
        0,
        "a drawn sprite showed blank pixels"
    );

    // Redraw the whole port and check every drawing on it against the target as it stood.
    state.borrow_mut().audit = true;
    lit.set(true);
    harness.advance(TICK);
    harness.pump();
    assert_eq!(
        state.borrow().blank,
        0,
        "a drawing on the screen was never drawn after the shift moved it"
    );
}
