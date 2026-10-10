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
                   spacer.half { height: 30.5px }
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
