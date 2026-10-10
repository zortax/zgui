//! Drawings that need the general rasteriser for a gradient or a clip, drawn as one CPU layer.
//!
//! Every fixture paints through a real content cache, so the layer route competes with the shape
//! routes as it does in a window.

mod support;

use zgui_atlas::AtlasLimits;
use zgui_canvas::{Brush, Marker, SceneHandle, Series, ShapeBuilder};
use zgui_color::Color;
use zgui_paint::{ContentCache, PaintReport, VectorCache, VectorRoute};
use zgui_profile::Counter;
use zgui_scene::kurbo::{self, Affine, Shape as _};
use zgui_testkit_scene::counters::Recording;

use support::{Element, Harness};

/// A 32 by 32 document of one triangle under a red to blue ramp, which no analytic route takes.
const RAMP: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><path d="M0 0 L32 0 L0 32 Z" fill="url(#a)"/></svg>"##;

/// The same ramp over a rectangle, which the analytic route draws as one quad.
const RAMP_RECT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><rect x="0" y="0" width="32" height="32" fill="url(#a)"/></svg>"##;

/// One drawing under a root, with nothing painted around it.
const CSS: &str = "root { display: block; width: 400px; height: 200px }
                   spacer { display: block; height: 10px }
                   spacer.tall { height: 30px }
                   mark { display: block; width: 64px; height: 64px; color: rgb(0, 128, 255) }";

/// A window's caches for one fixture.
struct Window {
    harness: Harness,
    vectors: VectorCache,
    content: ContentCache,
    raster: zgui_testkit_scene::MonoRaster,
}

impl Window {
    fn new(tree: Element, css: &str) -> Self {
        Self {
            harness: Harness::new(tree, css),
            vectors: VectorCache::new(),
            content: ContentCache::new(AtlasLimits::default()),
            raster: zgui_testkit_scene::MonoRaster::new(),
        }
    }

    /// A document drawn by `mark` below a spacer.
    fn document(source: &'static str) -> Self {
        Self::new(
            Element::new("root").children(vec![
                Element::new("spacer"),
                Element::new("mark").document(source),
            ]),
            CSS,
        )
    }

    /// Paints one frame with the general rasteriser cold, or built when `ready`.
    fn paint(&mut self, ready: bool) -> PaintReport {
        self.harness
            .paint_cached_vectors_ready(&self.vectors, &mut self.content, &self.raster, ready)
    }

    /// Gives the element `name` the class `class`, which moves the drawing without refragmenting
    /// it.
    fn restyle(&mut self, name: &str, class: &'static str) {
        let spacer = self.harness.element(name);
        self.harness
            .edit_and_restyle(|edit| edit.add_class(spacer, zgui_interned::ClassName::new(class)));
        zgui_layout::boxtree::patch::restyle(
            &mut self.harness.store,
            &self.harness.document,
            [spacer],
        );
        self.harness.compose_from_marks(400.0, 200.0);
    }

    /// The one colour sprite's rectangle.
    fn sprite(&self) -> [f32; 4] {
        let [sprite] = self.harness.scene().primitives.color_sprites.as_slice() else {
            panic!("one colour sprite");
        };
        sprite.bounds
    }
}

fn routes(report: &PaintReport) -> zgui_paint::VectorRoutes {
    let mut routes = zgui_paint::VectorRoutes::NONE;
    for entry in &report.vector_routes {
        routes.union_with(entry.routes);
    }
    routes
}

#[test]
fn a_gradient_document_draws_one_color_sprite_and_no_vector_item() {
    let mut window = Window::document(RAMP);
    let mut recording = Recording::begin();
    let mut report = None;
    let measured = recording.measure(|| report = Some(window.paint(false)));
    let report = report.expect("a frame");
    assert!(routes(&report).contains(VectorRoute::CpuLayer));
    assert!(!routes(&report).contains(VectorRoute::GeneralRaster));
    let primitives = &window.harness.scene().primitives;
    assert!(primitives.vectors.is_empty(), "no vector item");
    let [sprite] = primitives.color_sprites.as_slice() else {
        panic!("{} colour sprites", primitives.color_sprites.len());
    };
    assert_eq!(sprite.bounds, [0.0, 10.0, 64.0, 64.0], "the sprite covers the box");
    assert_eq!(measured.get(Counter::VectorLayersRasterised), 1);
    assert_eq!(measured.get(Counter::VectorRouteLayer), 1);
    assert_eq!(measured.get(Counter::VectorRouteGeneral), 0);
    assert_eq!(measured.get(Counter::VectorLayerBytesUploaded), 64 * 64 * 4);
}

#[test]
fn a_mono_inherited_icon_keeps_its_mask_route() {
    // Held so this test's rasters do not reach another test's counters.
    let _recording = Recording::begin();
    let mut window = Window::new(
        Element::new("root").children(vec![
            Element::new("mark").drawing("M0 0 L16 0 L16 16 Z", Some("0 0 16 16")),
        ]),
        CSS,
    );
    let report = window.paint(false);
    assert!(routes(&report).contains(VectorRoute::AtlasMask));
    assert!(!routes(&report).contains(VectorRoute::CpuLayer));
    assert!(window.harness.scene().primitives.color_sprites.is_empty());
}

/// A red to blue ramp over 40 units.
fn ramp() -> Brush {
    Brush::Linear {
        start: kurbo::Point::new(0.0, 0.0),
        end: kurbo::Point::new(40.0, 0.0),
        stops: vec![
            (0.0, Color::srgb_u8(255, 0, 0, 255)),
            (1.0, Color::srgb_u8(0, 0, 255, 255)),
        ],
        repeating: false,
    }
}

#[test]
fn a_series_canvas_is_no_candidate() {
    // Held so this test's rasters do not reach another test's counters.
    let _recording = Recording::begin();
    let handle = SceneHandle::new();
    handle.edit(|scene| {
        scene.replace(vec![
            ShapeBuilder::new(kurbo::Circle::new((20.0, 20.0), 12.0).to_path(0.1))
                .fill(ramp())
                .build(),
        ]);
        scene.push_series(Series::Points {
            data: std::sync::Arc::from(vec![[0.2_f32, 0.2], [0.5, 0.5], [0.8, 0.3]]),
            to_canvas: Affine::new([60.0, 0.0, 0.0, -60.0, 0.0, 60.0]),
            marker: Marker::Circle { radius: 3.0 },
            fill: Some(Brush::Solid(Color::srgb_u8(255, 0, 0, 255))),
            stroke: None,
        });
    });
    let mut window = Window::new(
        Element::new("root").children(vec![Element::new("mark").canvas(&handle)]),
        CSS,
    );
    let report = window.paint(false);
    assert!(!routes(&report).contains(VectorRoute::CpuLayer));
    assert!(window.harness.scene().primitives.color_sprites.is_empty());
}

#[test]
fn a_layer_sprite_carries_the_chain_clip_and_the_element_opacity() {
    // Held so this test's rasters do not reach another test's counters.
    let _recording = Recording::begin();
    let css = "root { display: block; width: 400px; height: 200px }
               port { display: block; width: 40px; height: 40px; overflow: hidden }
               mark { display: block; width: 64px; height: 64px; opacity: 0.5 }";
    let mut window = Window::new(
        Element::new("root").children(vec![
            Element::new("port").children(vec![Element::new("mark").document(RAMP)]),
        ]),
        css,
    );
    window.paint(false);
    let frag = window.harness.fragment_of("mark");
    let clip = window
        .harness
        .store()
        .fragment(frag)
        .expect("the drawing's fragment")
        .clip;
    let [sprite] = window.harness.scene().primitives.color_sprites.as_slice() else {
        panic!("one colour sprite");
    };
    assert_eq!(sprite.clip_id(), clip, "the chain clip applies at composite");
    assert_ne!(clip, zgui_scene::ClipId::ROOT);
    assert_eq!(sprite.opacity, 0.5, "the folded opacity rides the sprite");
    assert_eq!(sprite.bounds, [0.0, 0.0, 64.0, 64.0], "nothing is cut from the raster");
}

#[test]
fn a_ramp_an_analytic_route_takes_is_no_candidate() {
    let _recording = Recording::begin();
    let mut window = Window::document(RAMP_RECT);
    let report = window.paint(false);
    assert!(routes(&report).contains(VectorRoute::Analytic));
    assert!(!routes(&report).contains(VectorRoute::CpuLayer));
}

#[test]
fn a_drawing_that_needs_no_general_route_leaves_the_candidates() {
    // A clip around nothing painted: a candidate no layer can draw and no shape route needs.
    let handle = SceneHandle::new();
    handle.edit(|scene| {
        scene.replace(vec![
            ShapeBuilder::new(kurbo::Rect::new(0.0, 0.0, 20.0, 20.0).to_path(0.1))
                .clipped(kurbo::Circle::new((10.0, 10.0), 8.0).to_path(0.1))
                .build(),
        ]);
    });
    let mut window = Window::new(
        Element::new("root").children(vec![Element::new("mark").canvas(&handle)]),
        CSS,
    );
    let mut recording = Recording::begin();
    let measured = recording.measure(|| {
        window.paint(false);
    });
    assert_eq!(measured.get(Counter::VectorLayerFallbacks), 1);

    // A fresh record, so the drawing is encoded again with the same history.
    window.harness.painter = zgui_paint::Painter::new();
    let measured = recording.measure(|| {
        window.paint(false);
    });
    assert_eq!(
        measured.get(Counter::VectorLayerFallbacks),
        0,
        "the drawing is no candidate any more"
    );
}

#[test]
fn a_scrolled_layer_replays_and_rasterises_nothing() {
    let mut window = Window::document(RAMP);
    let mut recording = Recording::begin();
    window.paint(false);
    let before = window.sprite();
    window.restyle("spacer", "tall");
    let mut report = None;
    let measured = recording.measure(|| report = Some(window.paint(false)));
    let report = report.expect("a frame");
    assert!(report.vector_routes.is_empty(), "the drawing was encoded again");
    assert!(report.layers_owed.is_empty());
    assert_eq!(measured.get(Counter::VectorLayersRasterised), 0);
    assert!(measured.get(Counter::ChunksTranslated) > 0);
    assert_eq!(window.sprite()[1] - before[1], 20.0, "the sprite moved with its box");
}

#[test]
fn a_fractional_move_encodes_a_layer_again() {
    // Layout puts boxes on whole pixels, so the half pixel comes from a scale above the drawing:
    // one pixel of movement inside it is one and a half on the device.
    let css = "root { display: block; width: 400px; height: 300px }
               port { display: block; width: 200px; height: 200px; transform: scale(1.5);
                      transform-origin: 0 0 }
               spacer { display: block; height: 10px }
               spacer.tall { height: 11px }
               mark { display: block; width: 64px; height: 64px }";
    let mut window = Window::new(
        Element::new("root").children(vec![Element::new("port").children(vec![
            Element::new("spacer"),
            Element::new("mark").document(RAMP),
        ])]),
        css,
    );
    let mut recording = Recording::begin();
    window.paint(false);
    window.restyle("spacer", "tall");
    let mut report = None;
    let measured = recording.measure(|| report = Some(window.paint(false)));
    let report = report.expect("a frame");
    assert!(!report.vector_routes.is_empty(), "half a pixel off the grid encodes again");
    assert_eq!(measured.get(Counter::VectorLayersProvisional), 1);
    assert_eq!(report.layers_owed.len(), 1, "a stretched layer is owed a frame");

    // Owed frames encode it again until the drawing has held still for three of them.
    let mut rasterised = 0;
    let mut owed = Vec::new();
    for _ in 0..3 {
        let mut report = None;
        rasterised += recording
            .measure(|| report = Some(window.paint(false)))
            .get(Counter::VectorLayersRasterised);
        owed.push(report.expect("a frame").layers_owed.len());
    }
    assert_eq!(owed, [1, 1, 0]);
    assert_eq!(rasterised, 1, "one exact raster at the new phase");
    // The sprite lands on whole device pixels, and the raster carries the half pixel inside it.
    let top = window.sprite()[1] * 1.5;
    assert!((top - top.round()).abs() < 1.0e-4 && (top - 16.5).abs() <= 1.0, "{top}");
}

/// A document of one triangle-wave outline of `segments` lines under a ramp, `seed` apart from
/// its siblings.
fn zigzag(segments: usize, seed: usize) -> &'static str {
    let mut path = String::from("M0 0");
    for index in 0..segments {
        let x = 32.0 * index as f64 / segments as f64;
        let y = if index % 2 == 0 { 32 } else { seed % 4 };
        path.push_str(&format!(" L{x:.3} {y}"));
    }
    Box::leak(
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#ff0000"/><stop offset="1" stop-color="#0000ff"/></linearGradient></defs><path d="{path} Z" fill="url(#a)"/></svg>"##
        )
        .into_boxed_str(),
    )
}

#[test]
fn a_deferred_drawing_is_not_remembered_and_is_owed_a_frame() {
    // Three drawings of about 3 ms each: two fit the first frame's cold budget.
    let css = "root { display: block; width: 400px; height: 200px }
               mark { display: block; width: 32px; height: 32px }";
    let marks = (0..3)
        .map(|seed| Element::new("mark").document(zigzag(1_500, seed)))
        .collect();
    let mut window = Window::new(Element::new("root").children(marks), css);
    let mut recording = Recording::begin();
    let mut first = None;
    let measured = recording.measure(|| first = Some(window.paint(false)));
    let first = first.expect("a frame");
    assert_eq!(measured.get(Counter::VectorLayersRasterised), 2);
    assert_eq!(measured.get(Counter::VectorLayersDeferred), 1);
    assert_eq!(measured.get(Counter::ChunksIncomplete), 1, "not remembered");
    assert_eq!(first.layers_owed.len(), 1);
    assert_eq!(window.harness.scene().primitives.color_sprites.len(), 2);

    let mut second = None;
    let measured = recording.measure(|| second = Some(window.paint(false)));
    let second = second.expect("a frame");
    assert_eq!(measured.get(Counter::VectorLayersRasterised), 1);
    assert_eq!(second.vector_routes.len(), 1, "only the deferred drawing is encoded");
    assert!(second.layers_owed.is_empty());
    assert_eq!(window.harness.scene().primitives.color_sprites.len(), 3);
}
