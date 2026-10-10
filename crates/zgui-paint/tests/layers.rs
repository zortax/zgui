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
    /// The extent a restyle lays the document out in.
    extent: (f32, f32),
}

impl Window {
    fn new(tree: Element, css: &str) -> Self {
        Self {
            extent: (400.0, 200.0),
            ..Self::sized(tree, css, (400.0, 400.0))
        }
    }

    /// The same, over a surface of `size`.
    fn sized(tree: Element, css: &str, size: (f32, f32)) -> Self {
        Self {
            harness: Harness::sized(tree, css, size.0, size.1),
            vectors: VectorCache::new(),
            content: ContentCache::new(AtlasLimits::default()),
            raster: zgui_testkit_scene::MonoRaster::new(),
            extent: size,
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
        self.harness.paint_cached_vectors_ready(
            &self.vectors,
            &mut self.content,
            &self.raster,
            ready,
        )
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
        self.harness
            .compose_from_marks(self.extent.0, self.extent.1);
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
    assert_eq!(
        sprite.bounds,
        [0.0, 10.0, 64.0, 64.0],
        "the sprite covers the box"
    );
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
    // A triangle under a ramp, which no analytic route takes.
    let canvas = |series: bool| {
        let handle = SceneHandle::new();
        handle.edit(|scene| {
            let mut triangle = kurbo::BezPath::new();
            triangle.move_to((0.0, 0.0));
            triangle.line_to((40.0, 0.0));
            triangle.line_to((0.0, 40.0));
            triangle.close_path();
            scene.replace(vec![ShapeBuilder::new(triangle).fill(ramp()).build()]);
            if series {
                scene.push_series(Series::Points {
                    data: std::sync::Arc::from(vec![[0.2_f32, 0.2], [0.5, 0.5], [0.8, 0.3]]),
                    to_canvas: Affine::new([60.0, 0.0, 0.0, -60.0, 0.0, 60.0]),
                    marker: Marker::Circle { radius: 3.0 },
                    fill: Some(Brush::Solid(Color::srgb_u8(255, 0, 0, 255))),
                    stroke: None,
                });
            }
        });
        let mut window = Window::new(
            Element::new("root").children(vec![Element::new("mark").canvas(&handle)]),
            CSS,
        );
        let report = window.paint(false);
        let sprites = window.harness.scene().primitives.color_sprites.len();
        (routes(&report).contains(VectorRoute::CpuLayer), sprites)
    };
    assert_eq!(canvas(false), (true, 1), "the shapes alone take a layer");
    assert_eq!(
        canvas(true),
        (false, 0),
        "a series keeps the drawing off the layer"
    );
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
    assert_eq!(
        sprite.clip_id(),
        clip,
        "the chain clip applies at composite"
    );
    assert_ne!(clip, zgui_scene::ClipId::ROOT);
    assert_eq!(sprite.opacity, 0.5, "the folded opacity rides the sprite");
    assert_eq!(
        sprite.bounds,
        [0.0, 0.0, 64.0, 64.0],
        "nothing is cut from the raster"
    );
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
    assert!(
        report.vector_routes.is_empty(),
        "the drawing was encoded again"
    );
    assert!(report.layers_owed.is_empty());
    assert_eq!(measured.get(Counter::VectorLayersRasterised), 0);
    assert!(measured.get(Counter::ChunksTranslated) > 0);
    assert_eq!(
        window.sprite()[1] - before[1],
        20.0,
        "the sprite moved with its box"
    );
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
    assert!(
        !report.vector_routes.is_empty(),
        "half a pixel off the grid encodes again"
    );
    assert_eq!(measured.get(Counter::VectorLayersProvisional), 1);
    assert_eq!(
        report.layers_owed.len(),
        1,
        "a stretched layer is owed a frame"
    );

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
    assert!(
        (top - top.round()).abs() < 1.0e-4 && (top - 16.5).abs() <= 1.0,
        "{top}"
    );
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
    assert_eq!(
        second.vector_routes.len(),
        1,
        "only the deferred drawing is encoded"
    );
    assert!(second.layers_owed.is_empty());
    assert_eq!(window.harness.scene().primitives.color_sprites.len(), 3);
}

/// A 3000 by 3000 document: a ground, a ramp over a quadrilateral no analytic route takes, and a
/// few solid shapes. Too large for one layer.
const HUGE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 3000 3000"><defs><linearGradient id="a" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#203040"/><stop offset="1" stop-color="#a0c0e0"/></linearGradient></defs><path d="M0 0 L3000 0 L3000 3000 L0 3000 Z" fill="#102030"/><path d="M0 0 L3000 200 L2800 3000 L0 2900 Z" fill="url(#a)"/><circle cx="300" cy="300" r="200" fill="#ff8000"/><circle cx="1500" cy="1500" r="400" fill="#00a060"/><path d="M100 900 L700 1100 L200 1400 Z" fill="#e0e020"/></svg>"##;

/// A huge drawing below a spacer, in a 400 by 200 root.
fn huge() -> Window {
    let css = "root { display: block; width: 400px; height: 200px }
               spacer { display: block; height: 10px }
               spacer.up { height: 10px; margin-top: -400px }
               mark { display: block; width: 3000px; height: 3000px }";
    Window::new(
        Element::new("root").children(vec![
            Element::new("spacer"),
            Element::new("mark").document(HUGE),
        ]),
        css,
    )
}

/// How many colour sprites draw something.
fn drawn_sprites(window: &Window) -> usize {
    window
        .harness
        .scene()
        .primitives
        .color_sprites
        .iter()
        .filter(|sprite| sprite.bounds != [0.0; 4])
        .count()
}

#[test]
fn a_huge_drawing_draws_named_tiles_and_no_vector_item() {
    let mut window = huge();
    let mut recording = Recording::begin();
    let mut report = None;
    let measured = recording.measure(|| report = Some(window.paint(false)));
    let report = report.expect("a frame");
    assert!(routes(&report).contains(VectorRoute::CpuLayer));
    assert!(!routes(&report).contains(VectorRoute::GeneralRaster));
    assert!(report.layers_owed.is_empty());
    let scene = window.harness.scene();
    assert!(scene.primitives.vectors.is_empty(), "no vector item");
    assert!(!scene.has_unresolved_resources());
    let rasterised = measured.get(Counter::VectorLayerTilesRasterised);
    assert!(rasterised >= 1);
    assert_eq!(drawn_sprites(&window) as u64, rasterised);
    assert_eq!(measured.get(Counter::VectorLayersRasterised), 0);
    assert_eq!(measured.get(Counter::VectorRouteLayer), 1);

    // The next frames replay the drawing and rasterise no tile the surface does not show.
    for _ in 0..3 {
        let mut report = None;
        let measured = recording.measure(|| report = Some(window.paint(false)));
        assert_eq!(measured.get(Counter::VectorLayerTilesRasterised), 0);
        assert!(report.expect("a frame").vector_routes.is_empty());
    }
    assert_eq!(drawn_sprites(&window) as u64, rasterised);
}

/// The port a huge drawing is shown in, on a 1600 by 1000 surface.
const PORT: (f32, f32) = (1000.0, 600.0);

/// How far the drawing in the port is moved left, through a transform of its own.
const PORT_SHIFT: f32 = 300.0;

/// A huge drawing below a spacer, moved left by a transform, in a port that clips it.
///
/// The port's clip is measured outside the drawing's transform, so the scene names every tile and
/// leaves the clip to the shader.
fn huge_in_port() -> Window {
    let css = "root { display: block; width: 1000px; height: 600px; overflow: hidden }
               spacer { display: block; height: 10px }
               spacer.up { height: 10px; margin-top: -600px }
               mark { display: block; width: 3000px; height: 3000px;
                      transform: translateX(-300px) }";
    Window::sized(
        Element::new("root").children(vec![
            Element::new("spacer"),
            Element::new("mark").document(HUGE),
        ]),
        css,
        (1600.0, 1000.0),
    )
}

/// The cells of the 512 px grid of the drawing in the port that the port shows, when the drawing
/// is laid out at `top`.
fn cells_in_port(top: f32) -> Vec<(i32, i32)> {
    let side = 512.0;
    let mut cells = Vec::new();
    for row in 0..6 {
        for column in 0..6 {
            let x0 = side * column as f32 - PORT_SHIFT;
            let x1 = (x0 + side).min(3000.0 - PORT_SHIFT);
            let y0 = top + side * row as f32;
            let y1 = (y0 + side).min(top + 3000.0);
            if x0 < PORT.0 && x1 > 0.0 && y0 < PORT.1 && y1 > 0.0 {
                cells.push((column, row));
            }
        }
    }
    cells.sort_unstable();
    cells
}

/// The cells whose sprites draw something, when the drawing is laid out at `top`.
fn placed_cells(window: &Window, top: f32) -> Vec<(i32, i32)> {
    let mut cells: Vec<(i32, i32)> = window
        .harness
        .scene()
        .primitives
        .color_sprites
        .iter()
        .filter(|sprite| sprite.bounds != [0.0; 4])
        .map(|sprite| {
            // The frame is the sprite's own rectangle grown by one pixel.
            let x = sprite.frame[0] + 1.0;
            let y = sprite.frame[1] + 1.0 - top;
            ((x / 512.0).round() as i32, (y / 512.0).round() as i32)
        })
        .collect();
    cells.sort_unstable();
    cells
}

/// Paints `frames` frames and returns how many tiles they rasterised, checking that every owed
/// rectangle is in the port.
fn paint_tiles(window: &mut Window, recording: &mut Recording, frames: usize) -> u64 {
    let mut rasterised = 0;
    for _ in 0..frames {
        let mut report = None;
        rasterised += recording
            .measure(|| report = Some(window.paint(false)))
            .get(Counter::VectorLayerTilesRasterised);
        for owed in report.expect("a frame").layers_owed {
            assert!(
                owed.origin.x < PORT.0 as i32
                    && owed.origin.y < PORT.1 as i32
                    && owed.origin.x + owed.size.width <= PORT.0 as i32
                    && owed.origin.y + owed.size.height <= PORT.1 as i32,
                "an owed tile is in the port: {owed:?}"
            );
        }
    }
    rasterised
}

#[test]
fn a_huge_drawing_in_a_port_rasterises_only_the_tiles_the_port_shows() {
    let mut window = huge_in_port();
    let mut recording = Recording::begin();
    let rasterised = paint_tiles(&mut window, &mut recording, 4);
    let shown = cells_in_port(10.0);
    assert_eq!(shown.len(), 6, "three columns and two rows");
    assert_eq!(
        rasterised,
        shown.len() as u64,
        "no tile outside the port is rasterised"
    );
    assert_eq!(placed_cells(&window, 10.0), shown);
}

#[test]
fn a_scrolled_huge_drawing_rasterises_only_the_exposed_tiles() {
    let mut window = huge_in_port();
    let mut recording = Recording::begin();
    paint_tiles(&mut window, &mut recording, 4);
    let before = placed_cells(&window, 10.0);
    // 600 px up: the first row of tiles leaves the port and the third enters it.
    window.restyle("spacer", "up");
    let mut report = None;
    let measured = recording.measure(|| report = Some(window.paint(false)));
    assert!(
        report.expect("a frame").vector_routes.is_empty(),
        "the drawing replays at its new place"
    );
    assert!(measured.get(Counter::ChunksTranslated) > 0);
    let exposed = measured.get(Counter::VectorLayerTilesRasterised)
        + paint_tiles(&mut window, &mut recording, 3);
    let top = 10.0 - 600.0;
    let entered: Vec<(i32, i32)> = cells_in_port(top)
        .into_iter()
        .filter(|cell| !cells_in_port(10.0).contains(cell))
        .collect();
    assert_eq!(entered.len(), 3, "one row of three");
    assert_eq!(exposed, entered.len() as u64, "only the tiles that entered");
    let placed = placed_cells(&window, top);
    for cell in placed.iter().filter(|cell| !before.contains(cell)) {
        assert!(
            entered.contains(cell),
            "{cell:?} was rasterised outside the port"
        );
    }
    for cell in &entered {
        assert!(placed.contains(cell), "{cell:?} entered and is drawn");
    }
}

#[test]
fn a_record_whose_tiled_source_is_gone_encodes_again() {
    let mut window = huge();
    let _recording = Recording::begin();
    window.paint(false);
    // A lost device clears every cache, and the tiled source with it.
    window.content.clear();
    let report = window.paint(false);
    assert_eq!(
        report.vector_routes.len(),
        1,
        "the record names a source that is gone, so the drawing is encoded again"
    );
    assert!(!window.harness.scene().has_unresolved_resources());
    assert!(drawn_sprites(&window) >= 1);
}
