//! Recognised shapes reaching the display list as quads through the emit walk.
//!
//! Every fixture is a canvas painted through the real shared atlas, so the analytic route, the mask
//! route and the general route all compete for each shape as they do in a window.

mod support;

use zgui_atlas::AtlasLimits;
use zgui_canvas::{Brush, SceneHandle, ShapeBuilder};
use zgui_color::Color;
use zgui_paint::{PaintReport, VectorCache, VectorRoute};
use zgui_profile::Counter;
use zgui_scene::kurbo::{self, BezPath, Circle, RoundedRect, Shape as _};
use zgui_scene::{ClipId, ClipLink, ClipNode, Paint, PaintKind, Scene};
use zgui_testkit_scene::counters::Recording;

use support::{Element, Harness};

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
    harness.paint_cached_vectors(
        &VectorCache::new(),
        &mut zgui_paint::ContentCache::new(AtlasLimits::default()),
        &zgui_testkit_scene::MonoRaster::new(),
    )
}

/// Whether the one element painted took the analytic route and no other.
fn analytic_only(report: &PaintReport) -> bool {
    let routes = report.vector_routes[0].routes;
    routes.contains(VectorRoute::Analytic)
        && !routes.contains(VectorRoute::AtlasMask)
        && !routes.contains(VectorRoute::GeneralRaster)
}

/// The paint a reference resolves to.
fn paint_of(scene: &Scene, reference: zgui_scene::PaintRef) -> Paint {
    scene
        .paints
        .get(reference.id().expect("a paint"))
        .cloned()
        .expect("an interned paint")
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
    let scene = harness.scene();
    assert!(scene.primitives.vectors.is_empty());
    assert!(scene.primitives.mono_sprites.is_empty());
    let [quad] = scene.primitives.quads.as_slice() else {
        panic!("{} quads", scene.primitives.quads.len());
    };
    assert_eq!(
        quad.bounds,
        [13.0, 13.0, 22.0, 22.0],
        "the stroke's outer edge"
    );
    assert_eq!(quad.radii, [11.0; 8]);
    assert_eq!(quad.border, [2.0; 4]);
    assert_eq!(quad.fill.kind, PaintKind::Solid as u32);
    assert_eq!(quad.stroke.kind, PaintKind::Solid as u32);
    assert_ne!(quad.fill, quad.stroke);
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
    let [fill, stroke] = harness.scene().primitives.quads.as_slice() else {
        panic!("{} quads", harness.scene().primitives.quads.len());
    };
    assert_eq!(fill.bounds, [14.0, 14.0, 20.0, 20.0]);
    assert_eq!(fill.border, [0.0; 4]);
    assert!(fill.stroke.is_none());
    assert_eq!(stroke.bounds, [13.0, 13.0, 22.0, 22.0]);
    assert_eq!(stroke.border, [2.0; 4]);
    assert!(
        stroke.fill.is_none(),
        "the fill under a translucent ring stays the fill's own"
    );
    assert!(stroke.order > fill.order, "the stroke draws over the fill");
}

#[test]
fn separated_markers_share_one_order_through_one_run() {
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
    let scene = harness.scene();
    assert!(scene.primitives.vectors.is_empty());
    assert!(scene.primitives.mono_sprites.is_empty());
    let quads = &scene.primitives.quads;
    assert_eq!(quads.len(), 40);
    assert!(
        quads.iter().all(|quad| quad.order == quads[0].order),
        "disjoint members of one run share its order: {:?}",
        quads.iter().map(|quad| quad.order).collect::<Vec<_>>()
    );
    assert_eq!(scene.order_overlaps().map(|found| found.len()), Ok(0));
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
        harness.scene().primitives.quads.is_empty(),
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
    assert!(harness.scene().primitives.quads.is_empty());
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
    let scene = harness.scene();
    let [quad] = scene.primitives.quads.as_slice() else {
        panic!("{} quads", scene.primitives.quads.len());
    };
    assert_eq!(quad.bounds, [4.0, 4.0, 40.0, 20.0]);
    assert_eq!(quad.paint_origin, [0.0, 0.0]);
    match paint_of(scene, quad.fill) {
        Paint::Gradient {
            kind: zgui_scene::GradientKind::Linear { start, end },
            stops,
            ..
        } => {
            assert_eq!(
                (start.x.0, end.x.0),
                (4.0, 44.0),
                "the ramp in the shape's space"
            );
            assert_eq!(stops.len(), 2);
        }
        other => panic!("expected a linear gradient, found {other:?}"),
    }
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
    let scene = harness.scene();
    let [quad] = scene.primitives.quads.as_slice() else {
        panic!("{} quads", scene.primitives.quads.len());
    };
    match paint_of(scene, quad.fill) {
        Paint::Solid(color) => assert_eq!(
            color.to_premultiplied_srgb(),
            opaque(0, 128, 255).to_premultiplied_srgb()
        ),
        other => panic!("expected a solid fill, found {other:?}"),
    }
}

#[test]
fn a_rect_clipped_to_a_whole_pixel_rect_is_one_quad_under_a_minted_clip() {
    let (mut harness, _handle) = canvas(
        clipped_rect(kurbo::Rect::new(10.0, 10.0, 40.0, 40.0).to_path(0.1)),
        CSS,
    );
    let report = paint(&mut harness);
    assert!(analytic_only(&report));
    let scene = harness.scene();
    let [quad] = scene.primitives.quads.as_slice() else {
        panic!("{} quads", scene.primitives.quads.len());
    };
    let Some(ClipNode::Link { link, .. }) = scene.clips.get(ClipId(quad.clip)) else {
        panic!("the quad draws through no link of its own");
    };
    assert!(!link.is_rounded(), "expected a square link, found {link:?}");
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
    let scene = harness.scene();
    let [quad] = scene.primitives.quads.as_slice() else {
        panic!("{} quads", scene.primitives.quads.len());
    };
    assert!(
        !matches!(
            scene.clips.get(ClipId(quad.clip)),
            Some(ClipNode::Link {
                link: ClipLink::RoundedRect { .. },
                ..
            })
        ),
        "no link is minted for a clip that changes no pixel"
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
    let scene = harness.scene();
    assert!(scene.primitives.quads.iter().all(|quad| {
        scene.clips.rounded_links(ClipId(quad.clip)) <= zgui_scene::ClipTable::MAX_INLINE_ROUNDED
    }));
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
    assert!(harness.scene().primitives.quads.is_empty());
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
        assert!(harness.scene().primitives.quads.is_empty());
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
/// second frame's report, the counters it moved, and how far the first quad moved.
fn moved(
    shapes: Vec<zgui_canvas::Shape>,
) -> (
    PaintReport,
    zgui_testkit_scene::counters::Measurement,
    Option<f32>,
) {
    let css = "root { display: block; width: 400px; height: 200px }
               spacer { display: block; height: 10px }
               spacer.tall { height: 30px }
               mark { display: block; width: 240px; height: 48px; color: rgb(0, 128, 255) }";
    let handle = SceneHandle::new();
    handle.edit(|scene| scene.replace(shapes));
    let tree = Element::new("root").children(vec![
        Element::new("spacer"),
        Element::new("mark").canvas(&handle),
    ]);
    let mut harness = Harness::new(tree, css);
    let vectors = VectorCache::new();
    let mut content = zgui_paint::ContentCache::new(AtlasLimits::default());
    let raster = zgui_testkit_scene::MonoRaster::new();
    let mut recording = Recording::begin();
    let first = harness.paint_cached_vectors_ready(&vectors, &mut content, &raster, true);
    assert_eq!(
        first.vector_routes.len(),
        1,
        "the first frame encodes the canvas"
    );
    let before = harness
        .scene()
        .primitives
        .quads
        .first()
        .map(|quad| quad.bounds[1]);

    let spacer = harness.element("spacer");
    harness.edit_and_restyle(|edit| edit.add_class(spacer, zgui_interned::ClassName::new("tall")));
    // The style reaches the box in place, so the canvas keeps its fragment and only moves.
    zgui_layout::boxtree::patch::restyle(&mut harness.store, &harness.document, [spacer]);
    harness.compose_from_marks(400.0, 200.0);
    let mut report = None;
    let measured = recording.measure(|| {
        report = Some(harness.paint_cached_vectors_ready(&vectors, &mut content, &raster, true));
    });
    let after = harness
        .scene()
        .primitives
        .quads
        .first()
        .map(|quad| quad.bounds[1]);
    let shift = before.zip(after).map(|(before, after)| after - before);
    (report.expect("a frame was painted"), measured, shift)
}

#[test]
fn a_canvas_of_analytic_shapes_replays_when_it_moves() {
    let (report, measured, shift) = moved(vec![
        ShapeBuilder::new(circle(24.0, 24.0, 10.0))
            .fill(Brush::Solid(opaque(255, 0, 0)))
            .build(),
    ]);
    assert!(
        report.vector_routes.is_empty(),
        "the canvas was encoded again: {:?}",
        report.vector_routes
    );
    assert!(measured.get(Counter::ChunksTranslated) > 0);
    assert_eq!(measured.get(Counter::VectorReplaysMoved), 0);
    assert_eq!(shift, Some(20.0), "the replayed quad moved with its box");

    // A vector item holds its path in path space, so its record moves its placement.
    let gradient = Brush::Linear {
        start: kurbo::Point::new(0.0, 0.0),
        end: kurbo::Point::new(40.0, 0.0),
        stops: vec![(0.0, opaque(255, 0, 0)), (1.0, opaque(0, 0, 255))],
        repeating: false,
    };
    let (report, measured, _) = moved(vec![
        ShapeBuilder::new(BezPath::from_svg("M4 4 L40 4 L4 40 Z").expect("a path"))
            .fill(gradient)
            .build(),
    ]);
    assert!(
        report.vector_routes.is_empty(),
        "the drawing was encoded again: {:?}",
        report.vector_routes
    );
    assert!(measured.get(Counter::VectorReplaysMoved) > 0);
}
