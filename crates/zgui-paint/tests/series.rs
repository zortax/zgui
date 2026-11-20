//! Canvas series and the canvas view reaching the compositor through the emit walk.
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
use zgui_testkit_compositor::{Drawing, Instance};
use zgui_testkit_scene::counters::Recording;

use support::drawn::{number, numbers};
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
            recording: Recording::begin(),
        }
    }

    /// Paints one frame.
    fn paint(&mut self) -> PaintReport {
        self.harness
            .paint_cached_vectors_ready(&self.vectors, &mut self.content, true)
    }

    /// Sets the view transform and writes the view, as the binding would.
    fn view(&mut self, transform: Affine) {
        assert!(self.handle.set_transform(transform));
        self.harness.write_canvas_view("mark", &self.handle);
    }

    /// What the whole output draws.
    fn drawn(&mut self) -> Drawing {
        self.harness.redraw()
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

/// The marks items of a drawing.
fn marks(drawing: &Drawing) -> Vec<&Instance> {
    drawing.of_kind("marks").collect()
}

/// The flags a marks item is drawn with, in the order the transcript names them.
fn flags(mark: &Instance) -> Vec<&'static str> {
    ["union", "screen", "square_discs"]
        .into_iter()
        .filter(|flag| mark.has(flag))
        .collect()
}

/// The value of the count `name` of a marks item.
fn count(mark: &Instance, name: &str) -> f32 {
    number(mark, name).expect("a count")
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
    let drawing = fixture.drawn();
    let [mark] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(flags(mark), ["union", "screen"]);
    assert_eq!(count(mark, "discs"), 20.0);
    // The two triangles take whatever route they take; the mark draws between them.
    let kinds: Vec<&str> = drawing
        .instances()
        .iter()
        .map(Instance::kind)
        .filter(|kind| matches!(*kind, "mono" | "path" | "marks"))
        .collect();
    assert_eq!(kinds.len(), 3, "two triangles and the mark: {drawing}");
    assert_eq!(kinds[1], "marks", "{drawing}");
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
    let drawing = fixture.drawn();
    let [fill, ring] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(
        fill.color("paint"),
        Some([1.0, 0.0, 0.0, 1.0]),
        "the fill first"
    );
    assert_eq!(ring.color("paint"), Some([0.0, 0.0, 1.0, 1.0]));
    assert_eq!(flags(ring), ["union", "screen"]);
}

#[test]
fn a_square_marker_sets_square_discs() {
    let mut fixture = Fixture::new(|scene| scene.push_series(dots(Marker::Square { half: 2.5 })));
    fixture.paint();
    let drawing = fixture.drawn();
    let [mark] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(flags(mark), ["union", "screen", "square_discs"]);
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
    let drawing = fixture.drawn();
    let [mark] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(flags(mark), ["union", "screen"]);
    assert_eq!(mark.field("caps"), Some("round/butt"));
    assert_eq!(number(mark, "half_width"), Some(1.5));
    assert_eq!(
        (count(mark, "discs"), count(mark, "vertices")),
        (0.0, 22.0),
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
    let drawing = whole.drawn();
    let [mark] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    assert_eq!(count(mark, "vertices"), 20_002.0, "every point");
    drop(whole);
    let mut reduced = Fixture::styled(reduced_css, |scene| scene.push_series(line()));
    reduced.paint();
    let drawing = reduced.drawn();
    let [mark] = marks(&drawing)[..] else {
        panic!("{drawing}");
    };
    // 200 device columns a unit is a bucket of 512 columns, two a device column at least: at
    // most four points in each.
    let vertices = count(mark, "vertices");
    assert!(vertices <= (4 * 513 + 2) as f32, "{vertices} vertices");
}

#[test]
fn a_view_pan_keeps_the_series_payload() {
    let mut fixture = Fixture::new(|scene| scene.push_series(dots(Marker::Circle { radius: 3.0 })));
    fixture.paint();
    let before = placed(&fixture.drawn());

    fixture.view(Affine::translate((13.0, -4.0)));
    fixture.recording.reset();
    let report = fixture.paint();
    let built = zgui_profile::counter::get(Counter::SeriesPayloadsBuilt);
    let routed = zgui_profile::counter::get(Counter::VectorRouteMarks);
    assert!(
        report.vector_routes[0].routes.contains(VectorRoute::Marks),
        "the panned canvas encoded again"
    );
    let after = placed(&fixture.drawn());
    assert_eq!(
        [after.0[0] - before.0[0], after.0[1] - before.0[1]],
        [13.0, -4.0],
        "the pan moved the origin"
    );
    assert_eq!(after.1, before.1);
    assert_eq!(built, 0, "the pan built no payload");
    assert_eq!(routed, 1);
}

/// The origin and the axes of the first marks item of `drawing`.
fn placed(drawing: &Drawing) -> ([f32; 2], Option<Vec<f32>>) {
    let mark = marks(drawing)[0];
    let origin = numbers(mark, "origin").unwrap_or_else(|| vec![0.0; 2]);
    ([origin[0], origin[1]], numbers(mark, "axes"))
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
    let drawing = fixture.drawn();
    assert!(marks(&drawing)[0].has("union"));
    let before = placed(&drawing);

    fixture.view(Affine::translate((-21.0, 6.0)));
    fixture.recording.reset();
    fixture.paint();
    let after = placed(&fixture.drawn());
    assert_eq!(
        [after.0[0] - before.0[0], after.0[1] - before.0[1]],
        [-21.0, 6.0]
    );
}

#[test]
fn a_turned_view_keeps_the_marker_size() {
    let mut fixture = Fixture::new(|scene| scene.push_series(dots(Marker::Circle { radius: 3.0 })));
    fixture.paint();
    let before = placed(&fixture.drawn()).1.expect("axes");

    fixture.view(Affine::rotate(30.0_f64.to_radians()));
    fixture.recording.reset();
    fixture.paint();
    let built = zgui_profile::counter::get(Counter::SeriesPayloadsBuilt);
    let drawing = fixture.drawn();
    let after = placed(&drawing).1.expect("axes");
    assert!(
        marks(&drawing)[0].has("screen"),
        "the radius is in local units"
    );
    assert_eq!(built, 0, "the turn built no payload");
    assert_ne!(after, before, "the axes turned");
    let length = |axes: &[f32]| (axes[0] * axes[0] + axes[1] * axes[1]).sqrt();
    assert!(
        (length(&after) - length(&before)).abs() < 1e-3,
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

    // The payload centre lands 200 000 pixels away, while points still cover the canvas.
    fixture.view(Affine::translate((-200_000.0, 0.0)));
    fixture.recording.reset();
    let report = fixture.paint();
    let drawing = fixture.drawn();
    let stand_in = placed(&drawing).0;
    assert!(
        stand_in[0].abs() > 1.0,
        "the held payload, measured from its old centre, stands in: {drawing}"
    );
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
    let origin = placed(&fixture.drawn()).0;
    assert!(
        origin[0].abs() <= 1.0,
        "the built payload is measured from the view: {origin:?}"
    );
}
