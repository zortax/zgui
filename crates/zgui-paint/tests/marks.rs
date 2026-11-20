//! Recognised shapes reaching the compositor as marks through the emit walk.
//!
//! Every fixture is a canvas painted through the real content cache, so the analytic route, the
//! marks routes and the general route all compete for each shape as they do in a window.

mod support;

use zgui_atlas::AtlasLimits;
use zgui_canvas::{Brush, SceneHandle, ShapeBuilder};
use zgui_color::Color;
use zgui_paint::{PaintReport, VectorCache, VectorRoute};
use zgui_scene::kurbo::{self, BezPath, Circle, Shape as _};
use zgui_testkit_compositor::{Drawing, Instance};

use support::drawn::{number, numbers};
use support::{Element, Harness, poison};

/// A root with one canvas in it, at the origin, with no box paint of its own.
const CSS: &str = "root { display: block; width: 400px; height: 200px }
                   mark { display: block; width: 240px; height: 120px; color: rgb(0, 128, 255) }";

/// An opaque colour.
fn opaque(red: u8, green: u8, blue: u8) -> Color {
    Color::srgb_u8(red, green, blue, 255)
}

/// A canvas showing `shapes`, under `css`.
fn canvas(shapes: Vec<zgui_canvas::Shape>, css: &str) -> (Harness, SceneHandle) {
    let handle = SceneHandle::new();
    handle.edit(|scene| scene.replace(shapes));
    let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
    (Harness::new(tree, css), handle)
}

/// Paints one frame through the shared atlas with fresh caches.
fn paint(harness: &mut Harness) -> PaintReport {
    harness.paint_cached_vectors(
        &VectorCache::new(),
        &mut zgui_paint::ContentCache::new(AtlasLimits::default()),
    )
}

/// Whether the one element painted took the marks route and no other.
fn marks_only(report: &PaintReport) -> bool {
    let routes = report.vector_routes[0].routes;
    routes.contains(VectorRoute::Marks)
        && !routes.contains(VectorRoute::Analytic)
        && !routes.contains(VectorRoute::GeneralRaster)
}

/// The one marks item of a drawing, which holds no other vector content.
fn the_mark(drawing: &Drawing) -> &Instance {
    for kind in ["path", "mono", "quad"] {
        assert_eq!(drawing.of_kind(kind).count(), 0, "a {kind} in {drawing}");
    }
    let marks: Vec<_> = drawing.of_kind("marks").collect();
    let [mark] = marks[..] else {
        panic!("{drawing}");
    };
    mark
}

/// The counts of discs, boxes and vertices a marks item draws.
fn counts(mark: &Instance) -> [f32; 3] {
    ["discs", "boxes", "vertices"].map(|name| number(mark, name).expect("a count"))
}

/// A circle as a path.
fn circle(x: f64, y: f64, radius: f64) -> BezPath {
    Circle::new((x, y), radius).to_path(0.1)
}

/// `count` circles of radius 3.5 at random over 200 by 100.
fn scatter(count: usize) -> BezPath {
    let mut state = 0x5CA7_7E12_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut path = BezPath::new();
    for _ in 0..count {
        let (x, y) = (10.0 + next() * 200.0, 10.0 + next() * 100.0);
        path.extend(circle(x, y, 3.5).path_elements(0.1));
    }
    path
}

#[test]
fn overlapping_circles_are_one_union_mark() {
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(scatter(300))
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(marks_only(&report));
    let drawing = harness.redraw();
    let mark = the_mark(&drawing);
    assert!(mark.has("union"));
    assert_eq!(counts(mark), [300.0, 0.0, 0.0]);
}

#[test]
fn separated_markers_past_the_quad_limit_are_one_direct_mark() {
    // 400 circles 12 pixels apart: five pixels between neighbours, more than A0 takes.
    let mut path = BezPath::new();
    for at in 0..400 {
        let x = 6.0 + f64::from(at % 20) * 12.0;
        let y = 6.0 + f64::from(at / 20) * 12.0;
        path.extend(circle(x, y, 3.5).path_elements(0.1));
    }
    let css = "root { display: block; width: 400px; height: 300px }
               mark { display: block; width: 240px; height: 240px }";
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(path)
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .build(),
        ],
        css,
    );
    let report = paint(&mut harness);
    assert!(marks_only(&report));
    let drawing = harness.redraw();
    let mark = the_mark(&drawing);
    assert!(!mark.has("union"), "apart prims are painted one by one");
    assert_eq!(counts(mark)[0], 400.0);
}

#[test]
fn a_slanted_round_joined_polyline_is_one_union_mark() {
    let path = BezPath::from_svg("M4 4 L20 30 L40 10").expect("a path");
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(path)
                .stroke(Brush::Solid(opaque(255, 0, 0)), 3.0)
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(marks_only(&report));
    let drawing = harness.redraw();
    let mark = the_mark(&drawing);
    assert!(mark.has("union"), "the two segments overlap at their join");
    assert_eq!(
        counts(mark)[2],
        5.0,
        "one run: a separator, three vertices, a separator"
    );
    assert_eq!(number(mark, "half_width"), Some(1.5));
    assert_eq!(mark.field("caps"), Some("round/round"));
}

#[test]
fn a_transformed_canvas_circle_takes_the_marks_route() {
    for transform in ["rotate(30deg)", "scale(0.5)", "scale(2, 1)"] {
        let css = format!(
            "{CSS}
             mark {{ transform: {transform} }}"
        );
        // At the box's centre, which every transform about it leaves on the output.
        let (mut harness, _handle) = canvas(
            vec![
                ShapeBuilder::new(circle(120.0, 60.0, 10.0))
                    .fill(Brush::Solid(opaque(255, 0, 0)))
                    .build(),
            ],
            &css,
        );
        let report = paint(&mut harness);
        assert!(marks_only(&report), "`transform: {transform}`");
        let drawing = harness.redraw();
        let mark = the_mark(&drawing);
        assert!(!mark.has("union"), "one prim is never a union");
        assert_eq!(counts(mark)[0], 1.0);
    }
}

#[test]
fn a_polygon_takes_another_route() {
    let triangle = BezPath::from_svg("M4 4 L40 4 L4 40 Z").expect("a path");
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(triangle)
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(!report.vector_routes[0].routes.contains(VectorRoute::Marks));
    assert_eq!(harness.redraw().of_kind("marks").count(), 0);
}

#[test]
fn a_gradient_scatter_paints_through_its_mark() {
    // The plot's workaround: one colour as a two-stop ramp.
    let color = opaque(92, 205, 108);
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(scatter(300))
                .fill(Brush::Linear {
                    start: kurbo::Point::new(0.0, 0.0),
                    end: kurbo::Point::new(1.0, 0.0),
                    stops: vec![(0.0, color), (1.0, color)],
                    repeating: false,
                })
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(marks_only(&report));
    let drawing = harness.redraw();
    let mark = the_mark(&drawing);
    assert_eq!(mark.paint("paint").as_deref(), Some("linear"), "{drawing}");
    assert_eq!(
        mark.field("stops").map(|stops| stops.matches('@').count()),
        Some(2),
        "{drawing}"
    );
}

/// A copy of the plot's waves example at `width` by `height`: a grid, the damped sine as joined
/// line segments, and noisy cosine samples as circles, with no workaround.
fn waves(width: f64, height: f64) -> Vec<zgui_canvas::Shape> {
    let x_of = |x: f64| x / (4.0 * std::f64::consts::PI) * width;
    let y_of = |y: f64| (1.4 - y) / 2.8 * height;
    let mut shapes = Vec::new();
    // The grid: one layer of vertical lines and one of horizontal ones, translucent.
    let grid = Color::srgb_u8(140, 150, 170, 90);
    for vertical in [true, false] {
        let mut path = BezPath::new();
        let (count, step) = if vertical {
            (12, width / 12.0)
        } else {
            (7, height / 7.0)
        };
        for index in 0..=count {
            let at = f64::from(index) * step;
            if vertical {
                path.move_to((at, 0.0));
                path.line_to((at, height));
            } else {
                path.move_to((0.0, at));
                path.line_to((width, at));
            }
        }
        shapes.push(
            ShapeBuilder::new(path)
                .stroke(Brush::Solid(grid.with_alpha(0.8 * 90.0 / 255.0)), 1.0)
                .build(),
        );
    }
    // The wave: 601 samples, one segment per pair, joined into one path.
    let wave: Vec<(f64, f64)> = (0..=600)
        .map(|i| {
            let x = f64::from(i) * 4.0 * std::f64::consts::PI / 600.0;
            (x, x.sin() * (-x / 8.0).exp())
        })
        .collect();
    let mut line = BezPath::new();
    for pair in wave.windows(2) {
        line.move_to((x_of(pair[0].0), y_of(pair[0].1)));
        line.line_to((x_of(pair[1].0), y_of(pair[1].1)));
    }
    shapes.push(
        ShapeBuilder::new(line)
            .stroke(Brush::Solid(Color::srgb_u8(110, 168, 255, 255)), 2.0)
            .build(),
    );
    // The samples: 80 circles of radius 3.5, filled.
    let mut samples = BezPath::new();
    for i in 0..80 {
        let x = f64::from(i) * 4.0 * std::f64::consts::PI / 79.0;
        let jitter = (f64::from(i) * 12.9898).sin() * 43_758.545;
        let y = x.cos() * 0.8 + (jitter - jitter.floor() - 0.5) * 0.3;
        samples.extend(circle(x_of(x), y_of(y), 3.5).path_elements(0.1));
    }
    shapes.push(
        ShapeBuilder::new(samples)
            .fill(Brush::Solid(Color::srgb_u8(255, 182, 72, 255)))
            .build(),
    );
    shapes
}

#[test]
fn the_waves_plot_takes_no_general_route() {
    let css = "root { display: block; width: 700px; height: 400px }
               mark { display: block; width: 640px; height: 360px }";
    let (mut harness, _handle) = canvas(waves(640.0, 360.0), css);
    let report = paint(&mut harness);
    let routes = report.vector_routes[0].routes;
    assert!(!routes.contains(VectorRoute::GeneralRaster), "{routes:?}");
    assert!(routes.contains(VectorRoute::Marks), "the wave is a mark");
    let drawing = harness.redraw();
    assert_eq!(drawing.of_kind("path").count(), 0, "{drawing}");
}

#[test]
fn a_marks_canvas_replays_when_it_moves() {
    let css = "root { display: block; width: 400px; height: 200px }
               spacer { display: block; height: 10px }
               spacer.tall { height: 30px }
               mark { display: block; width: 240px; height: 120px }";
    let handle = SceneHandle::new();
    handle.edit(|scene| {
        scene.replace(vec![
            ShapeBuilder::new(scatter(300))
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .build(),
        ]);
    });
    let tree = Element::new("root").children(vec![
        Element::new("spacer"),
        Element::new("mark").canvas(&handle),
    ]);
    let mut harness = Harness::new(tree, css);
    let vectors = VectorCache::new();
    let mut content = zgui_paint::ContentCache::new(AtlasLimits::default());
    let raster = zgui_testkit_scene::MonoRaster::new();
    let first = harness.paint_cached_vectors_ready(&vectors, &mut content, &raster, true);
    assert!(first.vector_routes[0].routes.contains(VectorRoute::Marks));
    let place = |drawing: &Drawing| {
        let mark = the_mark(drawing);
        let origin = numbers(mark, "origin").expect("an origin");
        (
            mark.bounds()[1],
            [origin[0] - poison(), origin[1] - poison()],
        )
    };
    let before = place(&harness.redraw());

    let spacer = harness.element("spacer");
    harness.edit_and_restyle(|edit| edit.add_class(spacer, zgui_interned::ClassName::new("tall")));
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [spacer]);
    harness.compose_from_marks(400.0, 200.0);
    let report = harness.paint_cached_vectors_ready(&vectors, &mut content, &raster, true);
    assert!(
        report.vector_routes.is_empty(),
        "the canvas was encoded again: {:?}",
        report.vector_routes
    );
    let sync = harness.compositor.sync;
    assert_eq!(sync.offsets, 1, "{sync:?}");
    let after = place(&harness.redraw());
    assert_eq!(after.0 - before.0, 20.0, "the mark moved with its box");
    assert_eq!(before.1, [0.0, 10.0], "the fit places the payload");
    assert_eq!(after.1, [0.0, 30.0], "and the chunk offset moves it");
}
