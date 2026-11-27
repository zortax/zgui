//! Recognised shapes reaching the compositor as quads through the emit walk.
//!
//! Every fixture is a canvas painted through the real content cache, so the analytic route, the
//! marks routes and the general route all compete for each shape as they do in a window.

mod support;

use kurbo::{self, BezPath, Circle, RoundedRect, Shape as _};
use zgui_canvas::{Brush, SceneHandle, ShapeBuilder};
use zgui_color::Color;
use zgui_paint::{PaintReport, VectorCache, VectorRoute};
use zgui_testkit_compositor::{Drawing, Instance};

use support::drawn::numbers;
use support::{Element, Harness, unpoisoned};

/// A root with one canvas in it, at the origin, with no box paint of its own.
const CSS: &str = "root { display: block; width: 400px; height: 200px }
                   mark { display: block; width: 240px; height: 48px; color: rgb(0, 128, 255) }";

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
    harness.paint_cached_vectors(&VectorCache::new(), &mut zgui_paint::ContentCache::new())
}

/// Whether the one element painted took the analytic route and no other.
fn analytic_only(report: &PaintReport) -> bool {
    let routes = report.vector_routes[0].routes;
    routes.contains(VectorRoute::Analytic) && !routes.contains(VectorRoute::GeneralRaster)
}

/// The quads the whole output draws, which hold no other vector content.
fn quads(drawing: &Drawing) -> Vec<&Instance> {
    for kind in ["path", "mono", "marks"] {
        assert_eq!(drawing.of_kind(kind).count(), 0, "a {kind} in {drawing}");
    }
    drawing.quads()
}

/// A circle as a path.
fn circle(x: f64, y: f64, radius: f64) -> BezPath {
    Circle::new((x, y), radius).to_path(0.1)
}

#[test]
fn a_filled_circle_with_an_opaque_stroke_is_one_quad() {
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(circle(24.0, 24.0, 10.0))
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .stroke(Brush::Solid(opaque(0, 0, 255)), 2.0)
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(analytic_only(&report));
    let drawing = harness.redraw();
    let [quad] = quads(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(
        unpoisoned(quad.bounds()),
        [13.0, 13.0, 22.0, 22.0],
        "the stroke's outer edge"
    );
    assert_eq!(numbers(quad, "radii"), Some(vec![11.0; 8]));
    assert_eq!(numbers(quad, "border"), Some(vec![2.0; 4]));
    assert_eq!(quad.color("fill"), Some([1.0, 0.0, 0.0, 1.0]));
    assert_eq!(quad.color("stroke"), Some([0.0, 0.0, 1.0, 1.0]));
}

#[test]
fn a_filled_circle_with_a_translucent_stroke_is_two_quads_fill_first() {
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(circle(24.0, 24.0, 10.0))
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .stroke(Brush::Solid(Color::srgb_u8(0, 0, 255, 128)), 2.0)
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(analytic_only(&report));
    let drawing = harness.redraw();
    let [fill, stroke] = quads(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(unpoisoned(fill.bounds()), [14.0, 14.0, 20.0, 20.0]);
    assert_eq!(fill.field("border"), None);
    assert_eq!(
        unpoisoned(stroke.bounds()),
        [13.0, 13.0, 22.0, 22.0],
        "drawn second"
    );
    assert_eq!(numbers(stroke, "border"), Some(vec![2.0; 4]));
    assert_eq!(
        stroke.field("fill"),
        Some("none"),
        "the fill under a translucent ring stays the fill's own"
    );
}

#[test]
fn separated_markers_are_one_quad_each() {
    // Forty circles 12 pixels apart in two rows: 5 pixels between neighbours.
    let mut path = BezPath::new();
    for at in 0..40 {
        let x = 6.0 + f64::from(at % 20) * 12.0;
        let y = 6.0 + f64::from(at / 20) * 12.0;
        path.extend(circle(x, y, 3.5).path_elements(0.1));
    }
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(path)
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(analytic_only(&report));
    assert_eq!(quads(&harness.redraw()).len(), 40);
}

#[test]
fn abutting_fractional_rects_take_another_route() {
    let mut path = kurbo::Rect::new(0.0, 0.0, 10.5, 10.0).to_path(0.1);
    path.extend(kurbo::Rect::new(10.5, 0.0, 20.0, 10.0).path_elements(0.1));
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(path)
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(
        !report.vector_routes[0]
            .routes
            .contains(VectorRoute::Analytic)
    );
    assert!(
        harness.redraw().quads().is_empty(),
        "two quads would each paint the shared edge pixel, and darken it"
    );
}

#[test]
fn overlapping_translucent_circles_take_another_route() {
    let mut path = circle(20.0, 20.0, 8.0);
    path.extend(circle(30.0, 20.0, 8.0).path_elements(0.1));
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(path)
                .fill(Brush::Solid(Color::srgb_u8(255, 0, 0, 128)))
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(
        !report.vector_routes[0]
            .routes
            .contains(VectorRoute::Analytic)
    );
    assert!(harness.redraw().quads().is_empty());
}

#[test]
fn a_gradient_filled_rect_is_one_quad_with_its_gradient() {
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(kurbo::Rect::new(4.0, 4.0, 44.0, 24.0).to_path(0.1))
                .fill(Brush::Linear {
                    start: kurbo::Point::new(4.0, 0.0),
                    end: kurbo::Point::new(44.0, 0.0),
                    stops: vec![(0.0, opaque(255, 0, 0)), (1.0, opaque(0, 0, 255))],
                    repeating: false,
                })
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(analytic_only(&report));
    let drawing = harness.redraw();
    let [quad] = quads(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(unpoisoned(quad.bounds()), [4.0, 4.0, 40.0, 20.0]);
    assert_eq!(quad.field("paint_origin"), None);
    assert_eq!(quad.paint("fill").as_deref(), Some("linear"), "{drawing}");
    let ramp = support::poison();
    assert_eq!(
        quad.field("from"),
        Some(format!("({}, 0)", 4.0 + ramp).as_str()),
        "the ramp in the shape's space: {drawing}"
    );
    assert_eq!(
        quad.field("to"),
        Some(format!("({}, 0)", 44.0 + ramp).as_str())
    );
}

#[test]
fn an_inherited_fill_paints_the_quad_in_the_elements_colour() {
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(circle(24.0, 24.0, 6.0))
                .fill(Brush::Inherited { alpha: 1.0 })
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(analytic_only(&report));
    let drawing = harness.redraw();
    let [quad] = quads(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(quad.color("fill"), Some([0.0, 0.502, 1.0, 1.0]));
}

#[test]
fn a_rect_clipped_to_a_whole_pixel_rect_is_one_quad_under_a_square_clip() {
    let (mut harness, _handle) = canvas(
        clipped_rect(kurbo::Rect::new(10.0, 10.0, 40.0, 40.0).to_path(0.1)),
        CSS,
    );
    let report = paint(&mut harness);
    assert!(analytic_only(&report));
    let drawing = harness.redraw();
    let [quad] = quads(&drawing)[..] else {
        panic!("{drawing}");
    };
    let at = support::poison();
    assert_eq!(
        quad.field("clip"),
        Some(format!("[rect({}, {}, 30, 30)]", 10.0 + at, 10.0 + at).as_str()),
        "the quad draws through one square link: {drawing}"
    );
}

/// A clip tested at pixel centres has a hard edge, so only an edge on a whole pixel is exact.
#[test]
fn a_clip_with_an_edge_inside_a_pixel_takes_another_route() {
    for clip in [
        RoundedRect::new(10.0, 10.0, 40.0, 40.0, 6.0).to_path(0.1),
        kurbo::Rect::new(10.5, 10.0, 40.0, 40.0).to_path(0.1),
    ] {
        let (mut harness, _handle) = canvas(clipped_rect(clip), CSS);
        let report = paint(&mut harness);
        assert!(
            !report.vector_routes[0]
                .routes
                .contains(VectorRoute::Analytic)
        );
    }
}

#[test]
fn a_clip_that_holds_the_whole_shape_is_left_out() {
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(kurbo::Circle::new((25.0, 25.0), 8.0).to_path(0.1))
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .clipped(RoundedRect::new(0.5, 0.5, 49.5, 49.5, 6.0).to_path(0.1))
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(analytic_only(&report));
    let drawing = harness.redraw();
    let [quad] = quads(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(
        quad.field("clip"),
        None,
        "no link is added for a clip that changes no pixel"
    );
}

/// A canvas showing `shapes` inside `depth` nested boxes that clip to rounded corners.
fn rounded_ports(shapes: Vec<zgui_canvas::Shape>, depth: usize) -> (Harness, SceneHandle) {
    let handle = SceneHandle::new();
    handle.edit(|scene| scene.replace(shapes));
    let mut tree = Element::new("mark").canvas(&handle);
    for _ in 0..depth {
        tree = Element::new("port").children(vec![tree]);
    }
    let css = format!(
        "{CSS}
         port {{ display: block; width: 300px; height: 100px; overflow: hidden;
                 border-radius: 8px }}"
    );
    (
        Harness::new(Element::new("root").children(vec![tree]), &css),
        handle,
    )
}

/// A rect clipped to `clip`.
fn clipped_rect(clip: BezPath) -> Vec<zgui_canvas::Shape> {
    vec![
        ShapeBuilder::new(kurbo::Rect::new(4.0, 4.0, 44.0, 44.0).to_path(0.1))
            .fill(Brush::Solid(opaque(255, 0, 0)))
            .clipped(clip)
            .build(),
    ]
}

#[test]
fn a_square_clip_inside_rounded_ports_stays_analytic() {
    let square = kurbo::Rect::new(10.0, 10.0, 40.0, 40.0).to_path(0.1);
    let (mut harness, _handle) = rounded_ports(clipped_rect(square), 2);
    assert!(
        analytic_only(&paint(&mut harness)),
        "a square clip needs no rounded test"
    );
    assert_eq!(quads(&harness.redraw()).len(), 1);
}

#[test]
fn a_rect_clipped_to_a_triangle_takes_another_route() {
    let triangle = BezPath::from_svg("M10 10 L40 10 L10 40 Z").expect("a path");
    let (mut harness, _handle) = canvas(
        vec![
            ShapeBuilder::new(kurbo::Rect::new(4.0, 4.0, 44.0, 44.0).to_path(0.1))
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .clipped(triangle)
                .build(),
        ],
        CSS,
    );
    let report = paint(&mut harness);
    assert!(
        !report.vector_routes[0]
            .routes
            .contains(VectorRoute::Analytic)
    );
    assert!(harness.redraw().quads().is_empty());
}

#[test]
fn a_rotated_canvas_circle_takes_another_route() {
    for transform in ["rotate(30deg)", "scale(0.5)", "scale(2, 1)"] {
        let css = format!(
            "{CSS}
             mark {{ transform: {transform} }}"
        );
        let (mut harness, _handle) = canvas(
            vec![
                ShapeBuilder::new(circle(24.0, 24.0, 10.0))
                    .fill(Brush::Solid(opaque(255, 0, 0)))
                    .build(),
            ],
            &css,
        );
        let report = paint(&mut harness);
        assert!(
            !report.vector_routes[0]
                .routes
                .contains(VectorRoute::Analytic),
            "`transform: {transform}` drew quads the shader cannot antialias"
        );
        assert!(harness.redraw().quads().is_empty());
    }
    // A quarter turn and a scale-up keep the route.
    for transform in ["rotate(90deg)", "scale(2)"] {
        let css = format!(
            "{CSS}
             mark {{ transform: {transform} }}"
        );
        let (mut harness, _handle) = canvas(
            vec![
                ShapeBuilder::new(circle(24.0, 24.0, 10.0))
                    .fill(Brush::Solid(opaque(255, 0, 0)))
                    .build(),
            ],
            &css,
        );
        assert!(
            analytic_only(&paint(&mut harness)),
            "`transform: {transform}`"
        );
    }
}

/// Paints `shapes` in a canvas below a spacer, grows the spacer, and paints again. Answers the
/// second frame's report, the chunk offsets it wrote, and how far the first quad moved.
fn moved(shapes: Vec<zgui_canvas::Shape>, turned: bool) -> (PaintReport, usize, Option<f32>) {
    let css = "root { display: block; width: 400px; height: 200px }
               spacer { display: block; height: 10px }
               spacer.tall { height: 30px }
               mark { display: block; width: 240px; height: 48px; color: rgb(0, 128, 255) }
               mark.turned { transform: rotate(1deg) }";
    let handle = SceneHandle::new();
    handle.edit(|scene| scene.replace(shapes));
    let mark = Element::new("mark").canvas(&handle);
    let mark = if turned {
        mark.classes(&["turned"])
    } else {
        mark
    };
    let tree = Element::new("root").children(vec![Element::new("spacer"), mark]);
    let mut harness = Harness::new(tree, css);
    let vectors = VectorCache::new();
    let mut content = zgui_paint::ContentCache::new();
    let first = harness.paint_cached_vectors_ready(&vectors, &mut content, true);
    assert_eq!(
        first.vector_routes.len(),
        1,
        "the first frame encodes the canvas"
    );
    let top = |drawing: Drawing| drawing.quads().first().map(|quad| quad.bounds()[1]);
    let before = top(harness.redraw());

    let spacer = harness.element("spacer");
    harness.edit_and_restyle(|edit| edit.add_class(spacer, zgui_interned::ClassName::new("tall")));
    // The style reaches the box in place, so the canvas keeps its fragment and only moves.
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [spacer]);
    harness.compose_from_marks(400.0, 200.0);
    let report = harness.paint_cached_vectors_ready(&vectors, &mut content, true);
    let offsets = harness.compositor.sync.offsets;
    let after = top(harness.redraw());
    let shift = before.zip(after).map(|(before, after)| after - before);
    (report, offsets, shift)
}

#[test]
fn a_canvas_of_analytic_shapes_replays_when_it_moves() {
    let (report, offsets, shift) = moved(
        vec![
            ShapeBuilder::new(circle(24.0, 24.0, 10.0))
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .build(),
        ],
        false,
    );
    assert!(
        report.vector_routes.is_empty(),
        "the canvas was encoded again: {:?}",
        report.vector_routes
    );
    assert!(offsets > 0, "the canvas's chunk moved");
    assert_eq!(shift, Some(20.0), "the replayed quad moved with its box");

    // A path holds its geometry in path space, so its chunk moves with the box. Turned, so the
    // drawing is no CPU layer.
    let gradient = Brush::Linear {
        start: kurbo::Point::new(0.0, 0.0),
        end: kurbo::Point::new(40.0, 0.0),
        stops: vec![(0.0, opaque(255, 0, 0)), (1.0, opaque(0, 0, 255))],
        repeating: false,
    };
    let (report, offsets, _) = moved(
        vec![
            ShapeBuilder::new(BezPath::from_svg("M4 4 L40 4 L4 40 Z").expect("a path"))
                .fill(gradient)
                .build(),
        ],
        true,
    );
    assert!(
        report.vector_routes.is_empty(),
        "the drawing was encoded again: {:?}",
        report.vector_routes
    );
    assert!(offsets > 0, "the drawing's chunk moved");
}
