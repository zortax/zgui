//! Repeated outlines drawn as marks of placements through the emit walk.
//!
//! Every fixture paints through a real content cache, so the path glyph route competes with the
//! other shape routes as it does in a window.

mod support;

use std::sync::Arc;
use zgui_atlas::AtlasLimits;

use kurbo::{Affine, BezPath};
use zgui_canvas::{Brush, Marker, SceneHandle, Series, ShapeBuilder};
use zgui_color::Color;
use zgui_paint::{ContentCache, PaintReport, VectorCache, VectorRoute};
use zgui_profile::Counter;
use zgui_testkit_compositor::{Drawing, Instance};
use zgui_testkit_scene::counters::Recording;

use support::drawn::number;
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
}

impl Window {
    /// A canvas of `shapes`, with `style` added to the stylesheet.
    fn new(shapes: Vec<zgui_canvas::Shape>, style: &str) -> Self {
        Self::canvas(|scene| scene.replace(shapes), style)
    }

    /// A canvas over a scene `draw` fills, with `style` added to the stylesheet.
    fn canvas(draw: impl FnOnce(&mut zgui_canvas::CanvasScene), style: &str) -> Self {
        let handle = SceneHandle::new();
        handle.edit(draw);
        let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
        Self {
            harness: Harness::new(tree, &format!("{CSS}\n{style}")),
            handle,
            vectors: VectorCache::new(),
            content: ContentCache::new(AtlasLimits::default()),
        }
    }

    /// Paints one frame with the general rasteriser built.
    fn paint(&mut self) -> PaintReport {
        self.harness
            .paint_cached_vectors_ready(&self.vectors, &mut self.content, true)
    }

    /// What the whole output draws.
    fn drawn(&mut self) -> Drawing {
        self.harness.redraw()
    }
}

/// The marks items of `drawing`.
fn marks(drawing: &Drawing) -> Vec<&Instance> {
    drawing.of_kind("marks").collect()
}

/// The placements a marks item draws.
fn placements(mark: &Instance) -> f32 {
    number(mark, "placements").expect("a count")
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
    let drawing = window.drawn();
    for kind in ["path", "mono", "quad"] {
        assert_eq!(drawing.of_kind(kind).count(), 0, "a {kind} in {drawing}");
    }
    let [mark] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert!(mark.has("union"), "neighbours overlap");
    assert_eq!(placements(mark), 300.0, "one copy per triangle");
    assert_eq!(measured.get(Counter::VectorRoutePathGlyphs), 1);
    assert_eq!(measured.get(Counter::VectorRouteGeneral), 0);

    // A pan of the view by a fraction of a pixel draws the shape again.
    assert!(window.handle.set_transform(Affine::translate((5.25, -3.5))));
    window.harness.write_canvas_view("mark", &window.handle);
    let measured = recording.measure(|| {
        window.paint();
    });
    assert_eq!(
        measured.get(Counter::VectorRoutePathGlyphs),
        1,
        "a new view draws the shape again"
    );
}

#[test]
fn separated_triangles_are_one_direct_glyph_mark() {
    let _recording = Recording::begin();
    let mut window = Window::new(vec![triangles(&apart())], "");
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::PathGlyphs));
    let drawing = window.drawn();
    let [mark] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert!(!mark.has("union"), "the copies are apart");
    assert_eq!(placements(mark), 60.0);
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
        assert!(marks(&window.drawn()).is_empty());
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

/// Adds a figure-eight of half-width `r` about `(x, y)`: two triangles that meet at the centre
/// and wind opposite ways.
fn figure_eight(path: &mut BezPath, (x, y): (f64, f64), r: f64) {
    path.move_to((x - r, y - r));
    path.line_to((x + r, y + r));
    path.line_to((x + r, y - r));
    path.line_to((x - r, y + r));
    path.close_path();
}

#[test]
fn overlapping_figure_eights_take_another_route() {
    let _recording = Recording::begin();
    let red = || Brush::Solid(Color::srgb_u8(255, 0, 0, 255));
    // Where a left lobe meets a right one the winding is zero, which a union would paint.
    let mut crowded_eights = BezPath::new();
    for &point in &crowded() {
        figure_eight(&mut crowded_eights, point, 3.0);
    }
    let mut window = Window::new(
        vec![ShapeBuilder::new(crowded_eights).fill(red()).build()],
        "",
    );
    let report = window.paint();
    assert!(!routes(&report).contains(VectorRoute::PathGlyphs));
    assert!(marks(&window.drawn()).is_empty());
    // Apart, each copy is drawn alone.
    let mut apart_eights = BezPath::new();
    for &point in &apart() {
        figure_eight(&mut apart_eights, point, 3.0);
    }
    let mut window = Window::new(
        vec![ShapeBuilder::new(apart_eights).fill(red()).build()],
        "",
    );
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::PathGlyphs));
    // A filled figure-eight marker is summed as a union, so it takes the general route.
    let mut outline = BezPath::new();
    figure_eight(&mut outline, (0.0, 0.0), 3.0);
    let mut window = Window::canvas(|scene| scene.push_series(markers_of(outline)), "");
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::GeneralRaster));
    assert!(!routes(&report).contains(VectorRoute::PathGlyphs));
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
    assert!(marks(&window.drawn()).is_empty());
}

#[test]
fn a_mirrored_canvas_keeps_its_glyphs() {
    let _recording = Recording::begin();
    for transform in ["scale(-1, 1)", "rotate(90deg)", "scale(2)"] {
        // About the corner, which keeps the doubled triangles on the output.
        let origin = if transform == "scale(2)" {
            "; transform-origin: 0 0"
        } else {
            ""
        };
        let mut window = Window::new(
            vec![triangles(&crowded())],
            &format!("mark {{ transform: {transform}{origin} }}"),
        );
        let report = window.paint();
        assert!(
            routes(&report).contains(VectorRoute::PathGlyphs),
            "`transform: {transform}`"
        );
        let drawing = window.drawn();
        assert_eq!(
            marks(&drawing).len(),
            1,
            "`transform: {transform}`: {drawing}"
        );
    }
}

#[test]
fn a_lone_triangle_is_a_path() {
    let _recording = Recording::begin();
    let mut window = Window::new(vec![triangles(&[(20.0, 20.0)])], "");
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::GeneralRaster));
    assert!(marks(&window.drawn()).is_empty());
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

/// A points series of 40 points over a canvas of 200 by 100, with a triangle of circumradius 4
/// about each.
fn markers() -> Series {
    let mut outline = BezPath::new();
    triangle(&mut outline, (0.0, 0.0), 4.0, false);
    markers_of(outline)
}

/// The points of [`markers`], with `outline` about each.
fn markers_of(outline: BezPath) -> Series {
    Series::Points {
        data: (0..40)
            .map(|index| {
                let t = index as f32 / 39.0;
                [t, (t * 6.0).sin() * 0.4 + 0.5]
            })
            .collect(),
        to_canvas: Affine::new([200.0, 0.0, 0.0, -100.0, 10.0, 110.0]),
        marker: Marker::Path(Arc::new(outline)),
        fill: Some(Brush::Solid(Color::srgb_u8(255, 0, 0, 255))),
        stroke: None,
    }
}

#[test]
fn a_path_marker_series_is_one_glyph_mark() {
    let mut window = Window::canvas(|scene| scene.push_series(markers()), "");
    let mut recording = Recording::begin();
    let mut report = None;
    let measured = recording.measure(|| report = Some(window.paint()));
    let report = report.expect("a frame");
    assert!(routes(&report).contains(VectorRoute::PathGlyphs));
    let drawing = window.drawn();
    let [mark] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert!(mark.has("union"));
    assert!(mark.has("screen"));
    assert_eq!(placements(mark), 40.0);
    assert_eq!(drawing.of_kind("path").count(), 0, "{drawing}");
    assert_eq!(measured.get(Counter::VectorRoutePathGlyphs), 1);
}

#[test]
fn a_view_pan_keeps_the_path_marker_payload() {
    let mut window = Window::canvas(|scene| scene.push_series(markers()), "");
    let mut recording = Recording::begin();
    window.paint();
    for (step, transform) in [
        Affine::translate((13.25, -7.5)),
        Affine::translate((40.0, 0.0)) * Affine::scale(1.5),
    ]
    .into_iter()
    .enumerate()
    {
        assert!(window.handle.set_transform(transform));
        window.harness.write_canvas_view("mark", &window.handle);
        let measured = recording.measure(|| {
            window.paint();
        });
        assert_eq!(
            measured.get(Counter::VectorRoutePathGlyphs),
            1,
            "step {step}"
        );
        assert_eq!(measured.get(Counter::SeriesPayloadsBuilt), 0, "step {step}");
    }
}

#[test]
fn a_turned_path_marker_series_draws_through_the_general_route() {
    let mut window = Window::canvas(
        |scene| scene.push_series(markers()),
        "mark { transform: rotate(30deg) }",
    );
    let _recording = Recording::begin();
    let report = window.paint();
    assert!(routes(&report).contains(VectorRoute::GeneralRaster));
    assert!(!routes(&report).contains(VectorRoute::PathGlyphs));
    let drawing = window.drawn();
    assert!(marks(&drawing).is_empty());
    let paths: Vec<_> = drawing.of_kind("path").collect();
    let [path] = paths[..] else {
        panic!("one path for the fill: {drawing}");
    };
    // Forty triangles, each one move.
    assert_eq!(path.text().matches('M').count(), 40, "{drawing}");
}
