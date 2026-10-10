//! Repeated outlines drawn as glyph marks through the emit walk.
//!
//! Every fixture paints through a real content cache, so the path glyph route competes with the
//! other shape routes as it does in a window.

mod support;

use zgui_atlas::AtlasLimits;
use zgui_canvas::{Brush, SceneHandle, ShapeBuilder};
use zgui_color::Color;
use zgui_paint::{ContentCache, PaintReport, VectorCache, VectorRoute};
use zgui_profile::Counter;
use zgui_scene::MarkItem;
use zgui_scene::kurbo::BezPath;
use zgui_testkit_scene::counters::Recording;

use support::{Element, Harness};

/// A root with one canvas in it, at the origin.
const CSS: &str = "root { display: block; width: 400px; height: 200px }
                   mark { display: block; width: 240px; height: 120px; color: rgb(0, 128, 255) }";

/// A canvas over `handle`, with the caches one window holds.
struct Window {
    harness: Harness,
    handle: SceneHandle,
    vectors: VectorCache,
    content: ContentCache,
    raster: zgui_testkit_scene::MonoRaster,
}

impl Window {
    /// A canvas of `shapes`, with `style` added to the stylesheet.
    fn new(shapes: Vec<zgui_canvas::Shape>, style: &str) -> Self {
        let handle = SceneHandle::new();
        handle.edit(|scene| scene.replace(shapes));
        let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
        Self {
            harness: Harness::new(tree, &format!("{CSS}\n{style}")),
            handle,
            vectors: VectorCache::new(),
            content: ContentCache::new(AtlasLimits::default()),
            raster: zgui_testkit_scene::MonoRaster::new(),
        }
    }

    /// Paints one frame with the general rasteriser built.
    fn paint(&mut self) -> PaintReport {
        self.harness.paint_cached_vectors_ready(
            &self.vectors,
            &mut self.content,
            &self.raster,
            true,
        )
    }

    /// The marks of the last frame.
    fn marks(&self) -> &[MarkItem] {
        &self.harness.scene().primitives.marks
    }
}

/// The routes every element of a frame took.
fn routes(report: &PaintReport) -> zgui_paint::VectorRoutes {
    let mut routes = zgui_paint::VectorRoutes::NONE;
    for entry in &report.vector_routes {
        routes.union_with(entry.routes);
    }
    routes
}

/// Adds an upright triangle of circumradius `r` about `(x, y)`; clockwise on the screen unless
/// `reversed`.
fn triangle(path: &mut BezPath, (x, y): (f64, f64), r: f64, reversed: bool) {
    let half = r * 3f64.sqrt() / 2.0;
    path.move_to((x, y - r));
    if reversed {
        path.line_to((x - half, y + r / 2.0));
        path.line_to((x + half, y + r / 2.0));
    } else {
        path.line_to((x + half, y + r / 2.0));
        path.line_to((x - half, y + r / 2.0));
    }
    path.close_path();
}

/// 300 triangles of circumradius 4, 3.1 units apart, so neighbours overlap.
fn crowded() -> Vec<(f64, f64)> {
    (0..300)
        .map(|index| {
            (
                10.0 + (index % 60) as f64 * 3.1,
                10.0 + (index / 60) as f64 * 3.3,
            )
        })
        .collect()
}

/// 60 triangles of circumradius 4, 14 units apart, so no two come near.
fn apart() -> Vec<(f64, f64)> {
    (0..60)
        .map(|index| {
            (
                10.0 + (index % 15) as f64 * 14.0,
                10.0 + (index / 15) as f64 * 14.0,
            )
        })
        .collect()
}

/// One path of triangles at `points`, filled red.
fn triangles(points: &[(f64, f64)]) -> zgui_canvas::Shape {
    let mut path = BezPath::new();
    for &point in points {
        triangle(&mut path, point, 4.0, false);
    }
    ShapeBuilder::new(path)
        .fill(Brush::Solid(Color::srgb_u8(255, 0, 0, 255)))
        .build()
}

#[test]
fn scattered_triangles_are_one_union_glyph_mark() {
    let mut window = Window::new(vec![triangles(&crowded())], "");
    let mut recording = Recording::begin();
    let mut report = None;
    let measured = recording.measure(|| report = Some(window.paint()));
    let report = report.expect("a frame");
    assert!(routes(&report).contains(VectorRoute::PathGlyphs));
    let primitives = &window.harness.scene().primitives;
    assert!(primitives.vectors.is_empty(), "no vector item");
    assert!(primitives.mono_sprites.is_empty(), "no mask sprite");
    assert!(primitives.quads.is_empty(), "no quad");
    let [mark] = window.marks() else {
        panic!("{} marks", window.marks().len());
    };
    assert!(mark.is_union(), "neighbours overlap");
    assert_eq!(mark.tiles, 16, "one outline, sixteen phases");
    assert_eq!(mark.glyphs, 16 + 300, "the table and one copy per triangle");
    assert_eq!(measured.get(Counter::VectorRoutePathGlyphs), 1);
    assert_eq!(measured.get(Counter::PathGlyphTilesRasterised), 16);
    assert_eq!(measured.get(Counter::VectorRouteGeneral), 0);
    assert_eq!(measured.get(Counter::VectorRouteMask), 0);

    // A pan of the view by a fraction of a pixel finds the split, the sheet and the payload.
    let payload = std::sync::Arc::clone(&window.harness.scene().primitives.mark_payloads[0]);
    assert!(
        window
            .handle
            .set_transform(zgui_scene::kurbo::Affine::translate((5.25, -3.5)))
    );
    window.harness.write_canvas_view("mark", &window.handle);
    let measured = recording.measure(|| {
        window.paint();
    });
    assert_eq!(
        measured.get(Counter::VectorRoutePathGlyphs),
        1,
        "a new view draws the shape again"
    );
    assert_eq!(measured.get(Counter::PathGlyphTilesRasterised), 0);
    let [again] = window.harness.scene().primitives.mark_payloads.as_slice() else {
        panic!("one payload");
    };
    assert!(
        std::sync::Arc::ptr_eq(again, &payload),
        "a pan keeps the payload"
    );
}

#[test]
fn separated_triangles_are_one_direct_glyph_mark() {
    let _recording = Recording::begin();
    let mut window = Window::new(vec![triangles(&apart())], "");
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::PathGlyphs));
    let [mark] = window.marks() else {
        panic!("{} marks", window.marks().len());
    };
    assert!(!mark.is_union(), "the copies are apart");
    assert_eq!(mark.glyphs, 16 + 60);
}

#[test]
fn a_glyph_record_holds_its_sheets() {
    let _recording = Recording::begin();
    let mut window = Window::new(vec![triangles(&apart())], "");
    window.paint();
    assert_eq!(window.marks().len(), 1);
    let report = window.content.report();
    assert_eq!(report.tiles, 1, "one sheet");
    assert_eq!(
        report.referenced_tiles, 1,
        "the record holds the sheet its mark reads"
    );
}

#[test]
fn an_even_odd_or_mixed_overlap_takes_another_route() {
    let _recording = Recording::begin();
    let red = || Brush::Solid(Color::srgb_u8(255, 0, 0, 255));
    let mut even_odd = BezPath::new();
    let mut mixed = BezPath::new();
    for (index, &point) in crowded().iter().enumerate() {
        triangle(&mut even_odd, point, 4.0, false);
        triangle(&mut mixed, point, 4.0, index % 2 == 1);
    }
    for shape in [
        ShapeBuilder::new(even_odd).fill_even_odd(red()).build(),
        ShapeBuilder::new(mixed).fill(red()).build(),
    ] {
        let mut window = Window::new(vec![shape], "");
        let report = window.paint();
        assert!(!routes(&report).contains(VectorRoute::PathGlyphs));
        assert!(window.marks().is_empty());
    }
    // Apart, neither rule nor turning matters: each copy is drawn alone.
    let mut separated = BezPath::new();
    for (index, &point) in apart().iter().enumerate() {
        triangle(&mut separated, point, 4.0, index % 2 == 1);
    }
    let mut window = Window::new(vec![ShapeBuilder::new(separated).fill(red()).build()], "");
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::PathGlyphs));
}

#[test]
fn a_turned_canvas_takes_another_route() {
    let _recording = Recording::begin();
    let mut window = Window::new(
        vec![triangles(&crowded())],
        "mark { transform: rotate(30deg) }",
    );
    let report = window.paint();
    assert!(!routes(&report).contains(VectorRoute::PathGlyphs));
    assert!(window.marks().is_empty());
}

#[test]
fn a_mirrored_canvas_keeps_its_glyphs() {
    let _recording = Recording::begin();
    for transform in ["scale(-1, 1)", "rotate(90deg)", "scale(2)"] {
        let mut window = Window::new(
            vec![triangles(&crowded())],
            &format!("mark {{ transform: {transform} }}"),
        );
        let report = window.paint();
        assert!(
            routes(&report).contains(VectorRoute::PathGlyphs),
            "`transform: {transform}`"
        );
        assert_eq!(window.marks().len(), 1, "`transform: {transform}`");
    }
}

#[test]
fn a_lone_triangle_keeps_the_mask_route() {
    let _recording = Recording::begin();
    let mut window = Window::new(vec![triangles(&[(20.0, 20.0)])], "");
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::AtlasMask));
    assert!(window.marks().is_empty());
}

#[test]
fn a_stroke_under_a_non_uniform_scale_declines() {
    let _recording = Recording::begin();
    let stroked = || {
        let mut path = BezPath::new();
        for &point in &apart() {
            triangle(&mut path, point, 4.0, false);
        }
        ShapeBuilder::new(path)
            .stroke(Brush::Solid(Color::srgb_u8(255, 0, 0, 255)), 1.0)
            .build()
    };
    let mut window = Window::new(vec![stroked()], "");
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::PathGlyphs));
    let mut window = Window::new(vec![stroked()], "mark { transform: scale(2, 1) }");
    let report = window.paint();
    assert!(!routes(&report).contains(VectorRoute::PathGlyphs));
}
