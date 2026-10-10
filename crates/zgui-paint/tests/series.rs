//! Canvas series and the canvas view reaching the display list through the emit walk.
//!
//! Every fixture paints through one content cache across frames, as a window does, so the mark
//! payloads a frame builds are the ones the next frame finds.

mod support;

use std::sync::Arc;

use zgui_atlas::AtlasLimits;
use zgui_canvas::{Brush, Marker, SceneHandle, Series, ShapeBuilder};
use zgui_color::Color;
use zgui_paint::{ContentCache, PaintReport, VectorCache, VectorRoute};
use zgui_profile::Counter;
use zgui_scene::kurbo::{self, Affine, BezPath, Circle, Shape as _};
use zgui_scene::{MarkFlags, MarkItem, Scene};
use zgui_testkit_scene::MonoRaster;
use zgui_testkit_scene::counters::Recording;

use support::{Element, Harness};

/// A root with one canvas in it, at the origin.
const CSS: &str = "root { display: block; width: 400px; height: 200px }
                   mark { display: block; width: 240px; height: 120px; color: rgb(0, 128, 255) }";

/// A canvas over `handle`, with the caches one window holds.
struct Fixture {
    /// The document and its painter.
    harness: Harness,
    /// The scene.
    handle: SceneHandle,
    /// The drawings.
    vectors: VectorCache,
    /// The atlas, the recognitions and the payloads.
    content: ContentCache,
    /// The glyph raster.
    raster: MonoRaster,
    /// Held so no other test of this binary moves the counters while this one paints.
    recording: Recording,
}

impl Fixture {
    /// A canvas over a scene `draw` fills.
    fn new(draw: impl FnOnce(&mut zgui_canvas::CanvasScene)) -> Self {
        Self::styled(CSS, draw)
    }

    /// A canvas over a scene `draw` fills, under `css`.
    fn styled(css: &str, draw: impl FnOnce(&mut zgui_canvas::CanvasScene)) -> Self {
        let handle = SceneHandle::new();
        handle.edit(draw);
        let tree = Element::new("root").children(vec![Element::new("mark").canvas(&handle)]);
        Self {
            harness: Harness::new(tree, css),
            handle,
            vectors: VectorCache::new(),
            content: ContentCache::new(AtlasLimits::default()),
            raster: MonoRaster::new(),
            recording: Recording::begin(),
        }
    }

    /// Paints one frame.
    fn paint(&mut self) -> PaintReport {
        self.harness.paint_cached_vectors_ready(
            &self.vectors,
            &mut self.content,
            &self.raster,
            true,
        )
    }

    /// Sets the view transform and writes the view, as the binding would.
    fn view(&mut self, transform: Affine) {
        assert!(self.handle.set_transform(transform));
        self.harness.write_canvas_view("mark", &self.handle);
    }

    /// The scene.
    fn scene(&self) -> &Scene {
        self.harness.scene()
    }
}

/// An opaque colour.
fn opaque(red: u8, green: u8, blue: u8) -> Color {
    Color::srgb_u8(red, green, blue, 255)
}

/// Twenty points over x and y in `0..1`, some close enough to overlap.
fn points() -> Arc<[[f32; 2]]> {
    (0..20)
        .map(|i| {
            let t = i as f32 / 19.0;
            [t, (t * 6.0).sin() * 0.4 + 0.5]
        })
        .collect()
}

/// Data `0..1` to a canvas 200 by 100, y up.
fn to_canvas() -> Affine {
    Affine::new([200.0, 0.0, 0.0, -100.0, 10.0, 110.0])
}

/// A points series over [`points`] with `marker`, filled red.
fn dots(marker: Marker) -> Series {
    Series::Points {
        data: points(),
        to_canvas: to_canvas(),
        marker,
        fill: Some(Brush::Solid(opaque(255, 0, 0))),
        stroke: None,
    }
}

/// The marks of a scene.
fn marks(scene: &Scene) -> &[MarkItem] {
    &scene.primitives.marks
}

/// A triangle over the whole canvas.
fn cover() -> zgui_canvas::Shape {
    let mut path = BezPath::new();
    path.move_to((0.0, 0.0));
    path.line_to((230.0, 0.0));
    path.line_to((0.0, 115.0));
    path.close_path();
    ShapeBuilder::new(path)
        .fill(Brush::Solid(opaque(0, 255, 0)))
        .build()
}

#[test]
fn a_series_is_one_union_mark_between_its_shapes() {
    let mut fixture = Fixture::new(|scene| {
        scene.push(cover());
        scene.push_series(dots(Marker::Circle { radius: 3.0 }));
        scene.push(cover());
    });
    let report = fixture.paint();
    assert!(report.vector_routes[0].routes.contains(VectorRoute::Marks));
    let scene = fixture.scene();
    let [mark] = marks(scene) else {
        panic!("{} marks", marks(scene).len());
    };
    assert_eq!(mark.flags, MarkFlags::UNION | MarkFlags::SCREEN);
    assert_eq!(mark.discs, 20);
    // The two triangles take whatever route they take; the mark sorts between them.
    let mut orders: Vec<u32> = scene
        .primitives
        .mono_sprites
        .iter()
        .map(|sprite| sprite.order)
        .chain(scene.primitives.vectors.iter().map(|item| item.order))
        .collect();
    orders.sort_unstable();
    assert_eq!(orders.len(), 2, "two triangles");
    assert!(orders[0] < mark.order && mark.order < orders[1]);
}

#[test]
fn a_stroked_points_series_is_a_fill_mark_and_a_ring_mark() {
    let mut fixture = Fixture::new(|scene| {
        scene.push_series(Series::Points {
            data: points(),
            to_canvas: to_canvas(),
            marker: Marker::Circle { radius: 4.0 },
            fill: Some(Brush::Solid(opaque(255, 0, 0))),
            stroke: Some((Brush::Solid(opaque(0, 0, 255)), 2.0)),
        });
    });
    fixture.paint();
    let scene = fixture.scene();
    let [fill, ring] = marks(scene) else {
        panic!("{} marks", marks(scene).len());
    };
    assert!(fill.order < ring.order, "the fill first");
    let payloads = &scene.primitives.mark_payloads;
    assert_eq!(payloads[0].discs[0][2..], [4.0, 0.0]);
    assert_eq!(payloads[1].discs[0][2..], [5.0, 3.0]);
    assert_eq!(ring.flags, MarkFlags::UNION | MarkFlags::SCREEN);
}

#[test]
fn a_square_marker_sets_square_discs() {
    let mut fixture = Fixture::new(|scene| scene.push_series(dots(Marker::Square { half: 2.5 })));
    fixture.paint();
    let [mark] = marks(fixture.scene()) else {
        panic!("one mark");
    };
    assert_eq!(
        mark.flags,
        MarkFlags::UNION | MarkFlags::SCREEN | MarkFlags::SQUARE_DISCS
    );
    assert_eq!(fixture.scene().primitives.mark_payloads[0].discs[0][2], 2.5);
}

#[test]
fn a_line_series_is_one_polyline_mark_with_its_caps() {
    let mut fixture = Fixture::new(|scene| {
        scene.push_series(Series::Line {
            data: points(),
            to_canvas: to_canvas(),
            stroke: kurbo::Stroke::new(3.0)
                .with_start_cap(kurbo::Cap::Round)
                .with_end_cap(kurbo::Cap::Butt),
            brush: Brush::Solid(opaque(255, 0, 0)),
        });
    });
    fixture.paint();
    let [mark] = marks(fixture.scene()) else {
        panic!("one mark");
    };
    assert_eq!(
        mark.flags,
        MarkFlags::UNION | MarkFlags::SCREEN | MarkFlags::caps(MarkFlags::ROUND, MarkFlags::BUTT)
    );
    assert_eq!(mark.half_width, 1.5);
    assert_eq!(
        (mark.discs, mark.vertices),
        (0, 22),
        "one run and two separators"
    );
}

/// A dense zigzag of `count` points, left to right over 0..1.
fn dense(count: usize) -> Arc<[[f32; 2]]> {
    (0..count)
        .map(|i| [i as f32 / count as f32, if i % 2 == 0 { 0.1 } else { 0.9 }])
        .collect()
}

#[test]
fn the_lod_property_reduces_a_line_series_per_column() {
    let line = || Series::Line {
        data: dense(20_000),
        to_canvas: Affine::new([200.0, 0.0, 0.0, -100.0, 0.0, 110.0]),
        stroke: kurbo::Stroke::new(1.0),
        brush: Brush::Solid(opaque(255, 0, 0)),
    };
    let reduced_css = "root { display: block; width: 400px; height: 200px }
                       mark { display: block; width: 240px; height: 120px;
                              --zgui-vector-lod: columns }";
    let mut whole = Fixture::new(|scene| scene.push_series(line()));
    whole.paint();
    let [mark] = marks(whole.scene()) else {
        panic!("one mark");
    };
    assert_eq!(mark.vertices, 20_002, "every point");
    drop(whole);
    let mut reduced = Fixture::styled(reduced_css, |scene| scene.push_series(line()));
    reduced.paint();
    let [mark] = marks(reduced.scene()) else {
        panic!("one mark");
    };
    // 200 device columns a unit is a bucket of 512 columns, two a device column at least: at
    // most four points in each.
    assert!(mark.vertices <= 4 * 513 + 2, "{} vertices", mark.vertices);
}

#[test]
fn a_view_pan_keeps_the_series_payload() {
    let mut fixture = Fixture::new(|scene| scene.push_series(dots(Marker::Circle { radius: 3.0 })));
    fixture.paint();
    let before = marks(fixture.scene())[0];
    let payload = Arc::clone(&fixture.scene().primitives.mark_payloads[0]);

    fixture.view(Affine::translate((13.0, -4.0)));
    fixture.recording.reset();
    let report = fixture.paint();
    let built = zgui_profile::counter::get(Counter::SeriesPayloadsBuilt);
    let routed = zgui_profile::counter::get(Counter::VectorRouteMarks);
    assert!(
        report.vector_routes[0].routes.contains(VectorRoute::Marks),
        "the panned canvas encoded again"
    );
    let after = marks(fixture.scene())[0];
    assert!(Arc::ptr_eq(
        &fixture.scene().primitives.mark_payloads[0],
        &payload
    ));
    assert_eq!(
        [
            after.origin[0] - before.origin[0],
            after.origin[1] - before.origin[1]
        ],
        [13.0, -4.0],
        "the pan moved the origin"
    );
    assert_eq!(after.axes, before.axes);
    assert_eq!(built, 0, "the pan built no payload");
    assert_eq!(routed, 1);
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
        path.extend(Circle::new((x, y), 3.5).path_elements(0.1));
    }
    path
}

#[test]
fn a_view_pan_keeps_a_shape_mark_payload() {
    let mut fixture = Fixture::new(|scene| {
        scene.push(
            ShapeBuilder::new(scatter(300))
                .fill(Brush::Solid(opaque(255, 0, 0)))
                .build(),
        );
    });
    fixture.paint();
    let before = marks(fixture.scene())[0];
    assert!(before.is_union());
    let payload = Arc::clone(&fixture.scene().primitives.mark_payloads[0]);

    fixture.view(Affine::translate((-21.0, 6.0)));
    fixture.paint();
    let after = marks(fixture.scene())[0];
    assert!(
        Arc::ptr_eq(&fixture.scene().primitives.mark_payloads[0], &payload),
        "the payload is the one the recognition was lowered to"
    );
    assert_eq!(
        [
            after.origin[0] - before.origin[0],
            after.origin[1] - before.origin[1]
        ],
        [-21.0, 6.0]
    );
}

#[test]
fn a_turned_view_keeps_the_marker_size() {
    let mut fixture = Fixture::new(|scene| scene.push_series(dots(Marker::Circle { radius: 3.0 })));
    fixture.paint();
    let before = marks(fixture.scene())[0];
    let payload = Arc::clone(&fixture.scene().primitives.mark_payloads[0]);

    fixture.view(Affine::rotate(30.0_f64.to_radians()));
    fixture.paint();
    let after = marks(fixture.scene())[0];
    let held = &fixture.scene().primitives.mark_payloads[0];
    assert!(Arc::ptr_eq(held, &payload));
    assert_eq!(held.discs[0][2], 3.0, "the radius is r times the scale");
    assert_ne!(after.axes, before.axes, "the axes turned");
    let [a, b, ..] = after.axes;
    let [a0, b0, ..] = before.axes;
    assert!(
        ((a * a + b * b).sqrt() - (a0 * a0 + b0 * b0).sqrt()).abs() < 1e-3,
        "a turn keeps the length of an axis"
    );
}

#[test]
fn a_far_pan_draws_the_held_payload_and_owes_a_frame_until_its_build_ends() {
    // 70 000 points over x in 0..70 000, four device pixels apart: past the size a frame builds.
    let data: Arc<[[f32; 2]]> = (0..70_000)
        .map(|i| [i as f32, (i % 7) as f32 * 0.1])
        .collect();
    let series = Series::Points {
        data,
        to_canvas: Affine::new([4.0, 0.0, 0.0, -100.0, 0.0, 110.0]),
        marker: Marker::Circle { radius: 1.0 },
        fill: Some(Brush::Solid(opaque(255, 0, 0))),
        stroke: None,
    };
    let mut fixture = Fixture::new(|scene| scene.push_series(series));
    let first = fixture.paint();
    assert!(first.layers_owed.is_empty());
    let payload = Arc::clone(&fixture.scene().primitives.mark_payloads[0]);

    // The payload centre lands 200 000 pixels away, while points still cover the canvas.
    fixture.view(Affine::translate((-200_000.0, 0.0)));
    fixture.recording.reset();
    let report = fixture.paint();
    assert!(Arc::ptr_eq(
        &fixture.scene().primitives.mark_payloads[0],
        &payload
    ));
    assert!(!report.layers_owed.is_empty(), "the frame owes a frame");
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(
            zgui_profile::counter::get(Counter::SeriesDrawsProvisional),
            1
        );
        assert_eq!(zgui_profile::counter::get(Counter::SeriesBuildsAsync), 1);
    }

    // Each owed frame encodes the canvas again, until the built payload stands.
    let mut owed = report.layers_owed;
    for _ in 0..400 {
        if owed.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        owed = fixture.paint().layers_owed;
    }
    assert!(owed.is_empty(), "the build ends and nothing more is owed");
    let built = &fixture.scene().primitives.mark_payloads[0];
    assert!(!Arc::ptr_eq(built, &payload));
    let mark = marks(fixture.scene())[0];
    assert!(
        mark.origin[0].abs() <= 1.0,
        "the payload is measured from the view: {:?}",
        mark.origin
    );
}
