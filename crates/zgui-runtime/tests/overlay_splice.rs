//! What a frame rebuilds when an out-of-flow box is mounted beside a long list.
//!
//! A tooltip and an offscreen screen-reader region are absolutely positioned with no positioned
//! ancestor, so the root's box lays them out and the element they are written in paints them. Each
//! mount or unmount of one is a change to one child list. The frame must splice the box in or out,
//! keep every box of the list, and damage the pixels of the box alone. A whole box-tree build here
//! renames every box in the document, which makes every fragment, every hit entry and every pixel
//! of the surface count as changed.
//!
//! The counters are one process-wide block, so every case holds a [`Recording`].

mod support;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{Device, DevicePx, Rect};
use zgui_platform::Surface;
use zgui_profile::Counter;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_render::{
    ExternalTexture, FrameOutcome, MemoryReport, RenderCapabilities, RenderTarget, Renderer,
    TextureHandle,
};
use zgui_runtime::{App, AppError, Runtime};
use zgui_scene::Scene;
use zgui_testkit_scene::counters::Recording;
use zgui_view::{AnyView, BuildCx, IntoView, Show, View};

/// The application column is moved off the origin, so a box laid out against it lands somewhere
/// other than a box laid out against the root.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .app { display: block; margin-left: 30px }
                   .row { display: block; height: 12px }
                   .tip { position: absolute; left: 120px; top: 40px; width: 60px; height: 14px;
                          background-color: #202020 }
                   .offscreen { position: absolute; left: -10000px; top: 0 }";

/// How many rows the list holds: enough that a rebuild of the whole tree dwarfs the overlay.
const ROWS: usize = 300;

/// The window's own area in device pixels, which is what a full repaint costs.
const SURFACE_AREA: i64 = 400 * 300;

/// What one frame was drawn against: whether the set was the whole surface, and its area otherwise.
type Log = Rc<RefCell<Vec<(bool, Option<i64>, Option<Rect<i32, Device>>)>>>;

/// A renderer that records the damage of every frame and draws nothing.
struct Recorder {
    /// Where the frames go.
    log: Log,
    /// The surface it was pointed at.
    target: Option<RenderTarget>,
    /// Tiles go into plain memory, so the upload path still runs.
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

    fn draw(&mut self, _scene: &Scene, damage: &DamageSet) -> FrameOutcome {
        self.log
            .borrow_mut()
            .push((damage.is_full(), damage.area(), damage.bounds()));
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

    fn texture_sink(&mut self) -> &mut dyn zgui_atlas::TextureSink {
        &mut self.atlas
    }
}

/// A long list, a tooltip shown while `tip` holds text, and an offscreen region holding one row
/// while `request` names one.
fn window(
    tip: RwSignal<Option<String>>,
    request: RwSignal<Option<usize>>,
    log: &Log,
) -> zgui_platform_headless::Harness<Runtime> {
    let factory = Rc::clone(log);
    let handler = App::new()
        .with_title("overlay")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
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
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| Arc::new(zgui_testkit_scene::MonoRaster::new())))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let mut list = zgui_elements::column().class("list");
            for row in 0..ROWS {
                list = list.child(
                    zgui_elements::column()
                        .class("row")
                        .child(format!("row {row}")),
                );
            }
            let tooltip = Show::new(
                move || tip.get().is_some(),
                move || {
                    AnyView::new(
                        zgui_elements::column()
                            .class("tip")
                            .child(move || tip.get().unwrap_or_default()),
                    )
                },
            );
            let offscreen = Show::new(
                move || request.get().is_some(),
                move || {
                    AnyView::new(zgui_elements::column().class("offscreen").child(
                        zgui_view::Dynamic::new(move || {
                            let row = request.get().unwrap_or_default();
                            AnyView::new(
                                zgui_elements::column()
                                    .class("row")
                                    .child(format!("requested {row}")),
                            )
                        }),
                    ))
                },
            );
            Box::new(
                zgui_elements::column()
                    .class("app")
                    .child(list)
                    .child(tooltip)
                    .child(offscreen)
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    zgui_platform_headless::Harness::new(handler)
}

/// The border box of the first fragment of the element carrying `class`, if it has one.
fn box_of(
    harness: &zgui_platform_headless::Harness<Runtime>,
    class: &str,
) -> Option<Rect<DevicePx, Device>> {
    let window = harness.app().windows().first().expect("a window");
    let document = window.document().borrow();
    let layout = window.layout().borrow();
    layout.keys().into_iter().find_map(|key| {
        let index = document.store().index_of(layout.node(key).source?)?;
        if !document
            .store()
            .classes_of(index)
            .iter()
            .any(|held| held.0.as_ref() == class)
        {
            return None;
        }
        let frag = *layout.fragments_of_box(key).first()?;
        let fragment = layout.fragment(frag)?;
        Some(fragment.on_device(fragment.border_box, &window.placements()))
    })
}

/// The same box in whole device pixels, grown to cover every pixel it touches.
fn pixels(rect: Rect<DevicePx, Device>) -> Rect<i32, Device> {
    let (left, top) = (rect.origin.x.0.floor(), rect.origin.y.0.floor());
    let right = (rect.origin.x.0 + rect.size.width.0).ceil();
    let bottom = (rect.origin.y.0 + rect.size.height.0).ceil();
    Rect::new(
        zgui_geom::Point::new(left as i32, top as i32),
        zgui_geom::Size::new((right - left) as i32, (bottom - top) as i32),
    )
}

/// The union of the rectangles a burst of frames was drawn against, and the area they covered.
///
/// # Panics
///
/// Panics when a frame in the burst was drawn against the whole surface.
fn covered(log: &Log) -> (Option<Rect<i32, Device>>, i64) {
    let mut bounds: Option<Rect<i32, Device>> = None;
    let mut area = 0;
    for &(full, held, rect) in log.borrow().iter() {
        assert!(!full, "a frame was drawn against a full-surface damage set");
        area += held.expect("a set that is not full has an area");
        if let Some(rect) = rect {
            bounds = Some(bounds.map_or(rect, |held: Rect<i32, Device>| held.union(rect)));
        }
    }
    (bounds, area)
}

/// Whether `outer` covers every pixel of `inner`.
fn contains(outer: Rect<i32, Device>, inner: Rect<i32, Device>) -> bool {
    outer.origin.x <= inner.origin.x
        && outer.origin.y <= inner.origin.y
        && outer.origin.x + outer.size.width >= inner.origin.x + inner.size.width
        && outer.origin.y + outer.size.height >= inner.origin.y + inner.size.height
}

/// Fails unless a change spliced the box tree: no whole build, and a bounded number of new boxes.
fn assert_spliced(what: &str, moved: &zgui_testkit_scene::counters::Measurement, most: u64) {
    let builds = moved.get(Counter::BoxTreeBuilds);
    assert_eq!(
        builds, 0,
        "{what} built the whole box tree {builds} time(s) for one child list"
    );
    let boxes = moved.get(Counter::BoxesRebuilt);
    assert!(
        boxes <= most,
        "{what} built {boxes} boxes, more than the {most} the change owns"
    );
    let hits = moved.get(Counter::HitIndexRebuilds);
    assert_eq!(hits, 0, "{what} rebuilt the hit index {hits} time(s)");
}

#[test]
fn a_tooltip_shown_and_hidden_over_a_long_list_splices_its_own_box() {
    let mut recording = Recording::begin();
    let tip = RwSignal::new(None::<String>);
    let request = RwSignal::new(None::<usize>);
    let log = Log::default();
    let mut app = window(tip, request, &log);

    // The control: opening the window builds every box once.
    let opened = recording.measure(|| {
        app.settle(8);
    });
    assert_eq!(
        opened.get(Counter::BoxTreeBuilds),
        1,
        "the window built no box tree"
    );
    assert!(
        opened.get(Counter::BoxesRebuilt) > ROWS as u64,
        "the control built fewer boxes than the list has rows"
    );

    log.borrow_mut().clear();
    let shown = recording.measure(|| {
        tip.set(Some("hello".into()));
        app.settle(8);
    });
    assert_spliced("showing the tooltip", &shown, 4);
    let placed = box_of(&app, "tip").expect("the tooltip has a fragment");
    assert_eq!(
        (placed.origin.x.0, placed.origin.y.0),
        (120.0, 40.0),
        "the tooltip is positioned against the root, the initial containing block"
    );
    let (bounds, area) = covered(&log);
    let bounds = bounds.expect("showing the tooltip damaged nothing");
    assert!(
        contains(bounds, pixels(placed)),
        "the frame was drawn against {bounds:?}, which misses the tooltip at {placed:?}"
    );
    assert!(
        area <= SURFACE_AREA / 16,
        "showing a 60x14 tooltip damaged {area} px of a {SURFACE_AREA} px surface"
    );

    log.borrow_mut().clear();
    let edited = recording.measure(|| {
        tip.set(Some("world".into()));
        app.settle(8);
    });
    assert_spliced("changing the tooltip text", &edited, 0);
    let (_, area) = covered(&log);
    assert!(
        area <= SURFACE_AREA / 16,
        "changing the tooltip text damaged {area} px"
    );

    log.borrow_mut().clear();
    let hidden = recording.measure(|| {
        tip.set(None);
        app.settle(8);
    });
    assert_spliced("hiding the tooltip", &hidden, 0);
    assert!(
        box_of(&app, "tip").is_none(),
        "the tooltip is still laid out"
    );
    let (bounds, area) = covered(&log);
    let bounds = bounds.expect("hiding the tooltip damaged nothing");
    assert!(
        contains(bounds, pixels(placed)),
        "the frame was drawn against {bounds:?}, which leaves the tooltip at {placed:?} on screen"
    );
    assert!(
        area <= SURFACE_AREA / 16,
        "hiding a 60x14 tooltip damaged {area} px"
    );
}

#[test]
fn an_offscreen_request_over_a_long_list_splices_its_own_box() {
    let mut recording = Recording::begin();
    let tip = RwSignal::new(None::<String>);
    let request = RwSignal::new(None::<usize>);
    let log = Log::default();
    let mut app = window(tip, request, &log);
    app.settle(8);

    for (what, next, most) in [
        ("requesting a row offscreen", Some(7), 4),
        ("requesting another row", Some(9), 2),
        ("dropping the request", None, 0),
    ] {
        log.borrow_mut().clear();
        let moved = recording.measure(|| {
            request.set(next);
            app.settle(8);
        });
        assert_spliced(what, &moved, most);
        let (_, area) = covered(&log);
        assert!(
            area <= SURFACE_AREA / 16,
            "{what} damaged {area} px for a region drawn off the surface"
        );
    }
}
