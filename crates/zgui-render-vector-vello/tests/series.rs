//! Canvas series and canvas views on a real device.
//!
//! A pan of a view re-encodes the canvas chunk and keeps its payloads, so it uploads none and
//! draws the pixels a fresh encoding draws. Markers and lines keep their size under a zoom or a
//! turn, data far from the origin lands on its pixel, and marks through a fit draw as the placed
//! shapes do.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_canvas::{Brush, CanvasScene, Marker, Series};
use zgui_color::Color;
use zgui_geom::Size;
use zgui_paint::content::{CachedMarks, Drawing, MarksOnly};
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement, draw_drawing, draw_with_masks};
use zgui_profile::Counter;
use zgui_render_wgpu::Pixels;
use zgui_scene::kurbo::{self, Affine, BezPath, Circle, Shape as _};
use zgui_scene::{ChunkPrims, ClipId, Scene, SpatialId, VectorId, peniko};
use zgui_svg::{Fill, Ink, Paint, Shape, Stroke};

use support::{SIDE, Which, harness, opaque, present, quad, rect};

/// White fill, the inherited colour of every shape here.
const PAINT: ShapePaint = ShapePaint {
    fill: Color::WHITE,
    stroke: None,
    stroke_width: 1.0,
};

/// Drawn at the origin of the surface, with no transform and no clip.
const PLACEMENT: VectorPlacement = VectorPlacement {
    clip: ClipId::ROOT,
    transform: SpatialId::VIEWPORT,
    scale: 1.0,
};

/// A white brush.
fn white() -> Brush {
    Brush::Solid(Color::WHITE)
}

/// A canvas scene holding `series`, viewed through `view`.
fn canvas(series: Vec<Series>, shapes: Vec<Shape>, view: Affine) -> CanvasScene {
    let mut scene = CanvasScene::default();
    scene.replace(shapes);
    for series in series {
        scene.push_series(series);
    }
    scene.set_transform(view);
    scene
}

/// One frame: a black background and `drawing`, encoded as chunk `revision` and retiring
/// `retired`.
fn encode(
    scene: &mut Scene,
    drawing: &Drawing,
    masks: &CachedMarks,
    revision: u64,
    retired: Option<u64>,
) {
    scene.begin_frame(Size::new(SIDE, SIDE));
    quad(
        scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    scene.begin_chunk_capture(ChunkPrims::default());
    draw_drawing(scene, VectorId(1), drawing, PAINT, masks, PLACEMENT);
    let chunk = Arc::new(scene.take_chunk_capture());
    scene.note_chunk_inserted(revision, chunk);
    if let Some(old) = retired {
        scene.note_chunk_retired(old);
    }
    scene.bind_capture(revision);
    scene.finish(&DamageSet::full());
}

/// Draws a frame `encode` built and returns its payload bytes and pixels.
fn draw(renderer: &mut zgui_render_wgpu::WgpuRenderer, scene: &mut Scene) -> (u64, Pixels) {
    zgui_profile::counter::reset();
    let pixels = present(renderer, scene);
    scene.clear_chunk_notes();
    (
        zgui_profile::counter::get(Counter::MarksPayloadBytes),
        pixels,
    )
}

/// The pixels of `drawing` encoded once, on a renderer of its own.
fn fresh(drawing: &Drawing) -> Option<Pixels> {
    let mut harness = harness(Which::Vello)?;
    let mut scene = Scene::new();
    encode(&mut scene, drawing, &CachedMarks::new(), 1, None);
    Some(draw(&mut harness.renderer, &mut scene).1)
}

/// Draws `canvas` at `views[0]`, then pans it to each later view with one cache and one renderer,
/// and returns the payload bytes and pixels of the last frame.
fn panned(
    canvas: &mut CanvasScene,
    fit: Affine,
    views: &[Affine],
) -> Option<(u64, Pixels, Drawing)> {
    let mut harness = harness(Which::Vello)?;
    let masks = CachedMarks::new();
    let mut scene = Scene::new();
    let mut last = None;
    for (at, &view) in views.iter().enumerate() {
        canvas.set_transform(view);
        let drawing = Drawing::canvas(canvas, fit);
        let revision = at as u64 + 1;
        encode(
            &mut scene,
            &drawing,
            &masks,
            revision,
            (revision > 1).then(|| revision - 1),
        );
        let (bytes, pixels) = draw(&mut harness.renderer, &mut scene);
        masks.end_frame();
        last = Some((bytes, pixels, drawing));
    }
    last
}

/// Points scattered over `0..1`, many of them overlapping at radius 3 on 100 pixels.
fn scatter(count: usize) -> Arc<[[f32; 2]]> {
    let mut state = 0x5CA7_7E12_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 11) as f64 / (1u64 << 53) as f64) as f32
    };
    (0..count).map(|_| [next(), next()]).collect()
}

#[test]
fn a_panned_series_draws_as_a_fresh_encoding_and_uploads_no_payload() {
    let series = Series::Points {
        data: scatter(400),
        to_canvas: Affine::new([100.0, 0.0, 0.0, -100.0, 14.0, 114.0]),
        marker: Marker::Circle { radius: 3.0 },
        fill: Some(Brush::Solid(Color::srgb(1.0, 1.0, 1.0, 0.5))),
        stroke: Some((white(), 1.0)),
    };
    let mut scene = canvas(vec![series], Vec::new(), Affine::IDENTITY);
    let pan = Affine::translate((13.25, -7.5));
    let Some((bytes, pixels, drawing)) =
        panned(&mut scene, Affine::IDENTITY, &[Affine::IDENTITY, pan])
    else {
        return;
    };
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(bytes, 0, "the pan uploads no payload");
    }
    let Some(expected) = fresh(&drawing) else {
        return;
    };
    assert!(coverage(&pixels) > 1000.0, "the series draws");
    assert_eq!(pixels.max_difference(&expected), 0);
}

/// The summed coverage of white on black, in pixels.
fn coverage(pixels: &Pixels) -> f64 {
    let mut sum = 0.0;
    for y in 0..SIDE {
        for x in 0..SIDE {
            sum += f64::from(pixels.rgba(x, y)[0]) / 255.0;
        }
    }
    sum
}

#[test]
fn a_series_marker_keeps_its_size_under_a_zoom() {
    for (marker, area) in [
        (Marker::Circle { radius: 6.0 }, core::f64::consts::PI * 36.0),
        (Marker::Square { half: 5.0 }, 100.0),
    ] {
        for zoom in [0.25, 1.0, 4.0] {
            let series = Series::Points {
                data: Arc::from([[0.5_f32, 0.5]]),
                to_canvas: Affine::new([40.0, 0.0, 0.0, 40.0, 44.3, 43.6]),
                marker: marker.clone(),
                fill: Some(white()),
                stroke: None,
            };
            let centre = (64.3, 63.6);
            let view = Affine::translate(centre)
                * Affine::scale(zoom)
                * Affine::translate((-centre.0, -centre.1));
            let drawing =
                Drawing::canvas(&canvas(vec![series], Vec::new(), view), Affine::IDENTITY);
            let Some(pixels) = fresh(&drawing) else {
                return;
            };
            let covered = coverage(&pixels);
            println!("{marker:?} at zoom {zoom}: {covered:.2} of {area:.2}");
            assert!(
                (covered - area).abs() <= 0.02 * area,
                "{marker:?} at zoom {zoom} covers {covered:.2}, not {area:.2}"
            );
        }
    }
}

/// Draws `data` as discs of radius 3 through `to_canvas`, and asserts that the coverage centroid
/// of each of the first 16 discs lies within 1/8 px of its place on a 4 by 4 grid at 20 px with a
/// 22.2 px step.
fn assert_discs_land(data: Arc<[[f32; 2]]>, to_canvas: Affine) {
    let series = Series::Points {
        data,
        to_canvas,
        marker: Marker::Circle { radius: 3.0 },
        fill: Some(white()),
        stroke: None,
    };
    let drawing = Drawing::canvas(
        &canvas(vec![series], Vec::new(), Affine::IDENTITY),
        Affine::IDENTITY,
    );
    let Some(pixels) = fresh(&drawing) else {
        return;
    };
    for i in 0..16 {
        let (cx, cy) = (20.0 + 22.2 * (i % 4) as f64, 20.0 + 22.2 * (i / 4) as f64);
        let (mut sum, mut sx, mut sy) = (0.0, 0.0, 0.0);
        for y in (cy as i32 - 8)..(cy as i32 + 8) {
            for x in (cx as i32 - 8)..(cx as i32 + 8) {
                let c = f64::from(pixels.rgba(x, y)[0]) / 255.0;
                sum += c;
                sx += c * (f64::from(x) + 0.5);
                sy += c * (f64::from(y) + 0.5);
            }
        }
        let (mx, my) = (sx / sum, sy / sum);
        assert!(
            (mx - cx).abs() <= 0.125 && (my - cy).abs() <= 0.125,
            "disc {i} lands at ({mx:.3}, {my:.3}), not ({cx:.3}, {cy:.3})"
        );
    }
}

#[test]
fn a_series_far_from_the_origin_lands_on_its_pixel() {
    // A scale with no exact f32 product, so raw f32 positions at 1e7 miss their pixels.
    let data: Arc<[[f32; 2]]> = (0..16)
        .map(|i| [1.0e7 + 6.0 * (i % 4) as f32, 1.0e7 + 6.0 * (i / 4) as f32])
        .collect();
    let to_canvas =
        Affine::translate((20.0, 20.0)) * Affine::scale(3.7) * Affine::translate((-1.0e7, -1.0e7));
    assert_discs_land(data, to_canvas);
}

#[test]
fn a_series_with_a_far_centre_lands_on_its_pixel() {
    // The data centre lands near 1.85e7, past the 65 536 limit, while the first 16 points are in
    // view.
    let mut data: Vec<[f32; 2]> = (0..16)
        .map(|i| [6.0 * (i % 4) as f32, 6.0 * (i / 4) as f32])
        .collect();
    data.push([1.0e7, 1.0e7]);
    let to_canvas = Affine::translate((20.0, 20.0)) * Affine::scale(3.7);
    assert_discs_land(data.into(), to_canvas);
}

/// The points of a zigzag over 100 by 100.
fn zigzag() -> Vec<[f32; 2]> {
    vec![
        [12.25, 20.5],
        [40.5, 90.25],
        [64.75, 30.5],
        [90.25, 100.75],
        [116.5, 24.25],
    ]
}

#[test]
fn a_line_series_matches_the_same_polyline_shape() {
    let points = zigzag();
    let series = Series::Line {
        data: points.iter().copied().collect(),
        to_canvas: Affine::IDENTITY,
        stroke: kurbo::Stroke::new(3.0).with_caps(kurbo::Cap::Round),
        brush: white(),
    };
    let drawing = Drawing::canvas(
        &canvas(vec![series], Vec::new(), Affine::IDENTITY),
        Affine::IDENTITY,
    );
    let Some(line) = fresh(&drawing) else {
        return;
    };

    let on_screen: Vec<kurbo::Point> = points
        .iter()
        .map(|&[x, y]| kurbo::Point::new(f64::from(x), f64::from(y)))
        .collect();
    let style = kurbo::Stroke::new(3.0)
        .with_caps(kurbo::Cap::Round)
        .with_join(kurbo::Join::Round);
    let Some(expected) = polyline_shape(&on_screen, style) else {
        return;
    };
    assert!(
        line.max_difference(&expected) <= 1,
        "the line series differs by {}",
        line.max_difference(&expected)
    );
}

/// The pixels of a polyline through `points` stroked with `style`, as one shape on the marks
/// route with no fit.
fn polyline_shape(points: &[kurbo::Point], style: kurbo::Stroke) -> Option<Pixels> {
    let mut path = BezPath::new();
    path.move_to(points[0]);
    for &point in &points[1..] {
        path.line_to(point);
    }
    let shape = Shape {
        path: Arc::new(path),
        fill: None,
        stroke: Some(Stroke {
            paint: Paint::Solid(Ink::Solid(Color::WHITE)),
            style,
        }),
        clips: Vec::new(),
    };
    let mut harness = harness(Which::Vello)?;
    let mut scene = support::scene();
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    draw_with_masks(
        &mut scene,
        VectorId(1),
        &[shape],
        PAINT,
        &MarksOnly,
        PLACEMENT,
    );
    assert_eq!(
        scene.primitives.marks.len(),
        1,
        "the shape takes the marks route"
    );
    scene.finish(&DamageSet::full());
    Some(present(&mut harness.renderer, &scene))
}

/// The summed coverage of the pixels whose centres lie within `radius` of `centre`.
fn coverage_within(pixels: &Pixels, centre: kurbo::Point, radius: f64) -> f64 {
    let mut sum = 0.0;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let at = kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            if at.distance(centre) <= radius {
                sum += f64::from(pixels.rgba(x, y)[0]) / 255.0;
            }
        }
    }
    sum
}

#[test]
fn a_line_series_keeps_its_width_under_turned_and_stretched_axes() {
    // A zigzag in canvas units, stretched apart in data space and turned and zoomed by the view.
    let corners = [
        (-20.0, -18.0),
        (-8.0, 15.0),
        (4.0, -12.0),
        (16.0, 17.0),
        (24.0, -14.0),
    ];
    let to_canvas = Affine::scale_non_uniform(3.0, 0.5);
    let view = Affine::translate((64.0, 64.0))
        * Affine::rotate(30.0_f64.to_radians())
        * Affine::scale(2.0);
    let data: Arc<[[f32; 2]]> = corners
        .iter()
        .map(|&(x, y)| [(x / 3.0) as f32, (y / 0.5) as f32])
        .collect();
    let on_screen: Vec<kurbo::Point> = data
        .iter()
        .map(|&[x, y]| view * to_canvas * kurbo::Point::new(f64::from(x), f64::from(y)))
        .collect();
    let width = 3.0;
    for cap in [kurbo::Cap::Round, kurbo::Cap::Butt, kurbo::Cap::Square] {
        let series = Series::Line {
            data: Arc::clone(&data),
            to_canvas,
            stroke: kurbo::Stroke::new(width).with_caps(cap),
            brush: white(),
        };
        let drawing = Drawing::canvas(&canvas(vec![series], Vec::new(), view), Affine::IDENTITY);
        let Some(line) = fresh(&drawing) else {
            return;
        };
        // The middle of the second segment, more than 20 px from every other segment.
        let middle = on_screen[1].midpoint(on_screen[2]);
        let across = coverage_within(&line, middle, 12.0);
        println!("{cap:?}: {across:.2} of {:.2}", 24.0 * width);
        assert!(
            (across - 24.0 * width).abs() <= 0.05 * 24.0 * width,
            "{cap:?}: the line covers {across:.2} of a 24 px chord, not {:.2}",
            24.0 * width
        );
        let style = kurbo::Stroke::new(width)
            .with_caps(cap)
            .with_join(kurbo::Join::Round);
        let Some(expected) = polyline_shape(&on_screen, style) else {
            return;
        };
        assert!(coverage(&line) > 300.0, "{cap:?}: the line draws");
        assert!(
            line.max_difference(&expected) <= 1,
            "{cap:?}: the line series differs from the shape by {}",
            line.max_difference(&expected)
        );
    }
}

/// Circles as one path.
fn circles(centres: &[(f64, f64, f64)]) -> BezPath {
    let mut path = BezPath::new();
    for &(x, y, radius) in centres {
        path.extend(Circle::new((x, y), radius).path_elements(0.1));
    }
    path
}

/// A white filled shape.
fn filled(path: BezPath) -> Shape {
    Shape {
        path: Arc::new(path),
        fill: Some(Fill {
            paint: Paint::Solid(Ink::Solid(Color::WHITE)),
            rule: peniko::Fill::NonZero,
        }),
        stroke: None,
        clips: Vec::new(),
    }
}

/// 300 circles of radius 3 at random over 100 by 100, most of them overlapping.
fn overlapping() -> Shape {
    let points = scatter(300);
    let centres: Vec<(f64, f64, f64)> = points
        .iter()
        .map(|&[x, y]| {
            (
                14.0 + f64::from(x) * 100.0,
                14.0 + f64::from(y) * 100.0,
                3.0,
            )
        })
        .collect();
    filled(circles(&centres))
}

#[test]
fn a_panned_shape_scene_uploads_no_payload() {
    let mut scene = canvas(Vec::new(), vec![overlapping()], Affine::IDENTITY);
    let pan = Affine::translate((-9.0, 5.25));
    let Some((bytes, pixels, drawing)) = panned(
        &mut scene,
        Affine::IDENTITY,
        &[Affine::IDENTITY, Affine::translate((-4.0, 2.0)), pan],
    ) else {
        return;
    };
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(bytes, 0, "the pan uploads no payload");
    }
    let Some(expected) = fresh(&drawing) else {
        return;
    };
    assert_eq!(pixels.max_difference(&expected), 0);
}

/// Discs apart and overlapping, a ring and a round-joined polyline, within 45 by 45.
fn small_shapes() -> Vec<Shape> {
    vec![
        filled(circles(&[
            (8.3, 8.6, 4.5),
            (13.7, 10.2, 3.25),
            (30.5, 9.25, 3.5),
        ])),
        Shape {
            path: Arc::new(circles(&[(22.4, 30.6, 7.0)])),
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Solid(Ink::Solid(Color::WHITE)),
                style: kurbo::Stroke::new(1.5),
            }),
            clips: Vec::new(),
        },
        Shape {
            path: Arc::new(
                BezPath::from_svg("M4.25 40.5 L14.5 30.25 L26.75 42.5").expect("a path"),
            ),
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Solid(Ink::Solid(Color::WHITE)),
                style: kurbo::Stroke::new(1.25)
                    .with_caps(kurbo::Cap::Round)
                    .with_join(kurbo::Join::Round),
            }),
            clips: Vec::new(),
        },
    ]
}

#[test]
fn marks_through_a_fit_draw_as_the_placed_shapes() {
    let fit = Affine::translate((10.5, 7.25)) * Affine::scale(2.5);
    let shapes = small_shapes();
    let drawing = Drawing::fitted(shapes.clone(), fit);
    let Some(through) = fresh(&drawing) else {
        return;
    };
    let placed: Vec<Shape> = shapes
        .iter()
        .map(|shape| zgui_svg::document::place::shape(shape, fit))
        .collect();
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut scene = support::scene();
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    draw_with_masks(
        &mut scene,
        VectorId(1),
        &placed,
        PAINT,
        &MarksOnly,
        PLACEMENT,
    );
    assert_eq!(
        scene.primitives.marks.len(),
        3,
        "every shape takes the marks route"
    );
    scene.finish(&DamageSet::full());
    let expected = present(&mut harness.renderer, &scene);
    assert!(
        through.max_difference(&expected) <= 1,
        "marks through the fit differ by {}",
        through.max_difference(&expected)
    );
}

/// A random walk of `count` points left to right over x in 0..1, its y within 0..1.
fn walk(count: usize) -> Arc<[[f32; 2]]> {
    let mut state = 0x0057_A1C5_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut y = 0.5_f64;
    (0..count)
        .map(|i| {
            // About a fifth of a pixel a step on a 100 pixel tall plot.
            y = (y + (next() - 0.5) * 0.007).clamp(0.05, 0.95);
            [(i as f64 / count as f64) as f32, y as f32]
        })
        .collect()
}

/// Draws `canvas` at its own view, then at `far` while its builds run, then at `far` once they
/// end, with one cache and one renderer, and returns the last frame and its drawing.
fn settled(canvas: &mut CanvasScene, far: Affine) -> Option<(Pixels, Drawing)> {
    let mut harness = harness(Which::Vello)?;
    let masks = CachedMarks::new();
    let mut scene = Scene::new();
    let mut revision = 0;
    let mut frame = |canvas: &CanvasScene, masks: &CachedMarks| {
        revision += 1;
        let drawing = Drawing::canvas(canvas, Affine::IDENTITY);
        encode(
            &mut scene,
            &drawing,
            masks,
            revision,
            (revision > 1).then(|| revision - 1),
        );
        let standing = zgui_profile::counter::get(Counter::SeriesDrawsProvisional);
        let pixels = draw(&mut harness.renderer, &mut scene).1;
        masks.end_frame();
        (pixels, standing, drawing)
    };
    frame(canvas, &masks);
    canvas.set_transform(far);
    let (_, standing, _) = frame(canvas, &masks);
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(
            standing, 2,
            "both parts draw a held payload while they build"
        );
    }
    masks.settle();
    let (pixels, standing, drawing) = frame(canvas, &masks);
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(standing, 0, "the built payloads stand");
    }
    Some((pixels, drawing))
}

#[test]
fn a_far_zoom_settles_to_a_fresh_encoding() {
    // Past the size a frame builds: 2 000 000 line points and 100 000 discs over x in 0..1.
    let line = Series::Line {
        data: walk(2_000_000),
        to_canvas: PLOT,
        stroke: kurbo::Stroke::new(1.0),
        brush: white(),
    };
    let dots = Series::Points {
        data: walk(100_000),
        to_canvas: PLOT,
        marker: Marker::Circle { radius: 1.5 },
        fill: Some(Brush::Solid(Color::srgb(1.0, 1.0, 1.0, 0.5))),
        stroke: None,
    };
    let mut canvas = CanvasScene::default();
    canvas.push_series_lod(line, zgui_canvas::Lod::Columns);
    canvas.push_series(dots);
    // 1 200 times wider along x, with data x 0.98 at the centre: the payload centres land about
    // 69 000 pixels away, and the line takes a reduction ten buckets finer.
    let at = 120.0 * 0.98 + 4.0;
    let far = Affine::new([1200.0, 0.0, 0.0, 1.0, 64.0 - 1200.0 * at, 0.0]);

    let Some((pixels, drawing)) = settled(&mut canvas, far) else {
        return;
    };
    let Some(expected) = fresh(&drawing) else {
        return;
    };
    assert!(coverage(&pixels) > 100.0, "the series draw");
    assert_eq!(pixels.max_difference(&expected), 0);
}

/// Three periods of a sine sampled `count` times left to right over x in 0..1.
fn sine(count: usize) -> Arc<[[f32; 2]]> {
    (0..count)
        .map(|i| {
            let x = i as f64 / count as f64;
            let y = 0.5 + 0.4 * (std::f64::consts::TAU * 3.0 * x).sin();
            [x as f32, y as f32]
        })
        .collect()
}

/// Data to the 128 pixel surface: 120 device columns a unit.
const PLOT: Affine = Affine::new([120.0, 0.0, 0.0, -100.0, 4.0, 114.0]);

/// A black surface `side` square holding `data` stroked `width` wide through `plot` as one shape
/// on the general route.
fn general_line(data: &[[f32; 2]], plot: Affine, width: f64, side: i32) -> Scene {
    let mut path = BezPath::new();
    for (index, &[x, y]) in data.iter().enumerate() {
        let point = plot * kurbo::Point::new(f64::from(x), f64::from(y));
        if index == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    let shape = Shape {
        path: Arc::new(path),
        fill: None,
        stroke: Some(Stroke {
            paint: Paint::Solid(Ink::Solid(Color::WHITE)),
            style: kurbo::Stroke::new(width),
        }),
        clips: Vec::new(),
    };
    let mut scene = support::scene_at(side);
    quad(
        &mut scene,
        rect(0.0, 0.0, side as f32, side as f32),
        opaque(0, 0, 0),
    );
    draw_with_masks(
        &mut scene,
        VectorId(1),
        &[shape],
        PAINT,
        &zgui_paint::content::NoVectorMasks,
        PLACEMENT,
    );
    scene.finish(&DamageSet::full());
    scene
}

/// How many samples the true coverage takes along each side of a pixel.
const SAMPLES: usize = 32;

/// The true coverage of `data` stroked `width` wide through `plot` on a surface `side` square, in
/// levels of 255 a pixel: the share of its 32 by 32 samples that lie in the stroke.
///
/// The stroke is the union of its segments, each with round ends where it meets another and butt
/// ends at the ends of the line. The path renderer fills a stroke by the sum of its outline's area,
/// which is too much where a line folds over itself, so it is no reference there.
fn true_line(data: &[[f32; 2]], plot: Affine, width: f64, side: i32) -> Vec<u8> {
    let points: Vec<kurbo::Point> = data
        .iter()
        .map(|&[x, y]| plot * kurbo::Point::new(f64::from(x), f64::from(y)))
        .collect();
    let side = side as usize;
    let rows = side * SAMPLES;
    let words = rows.div_ceil(64);
    let mut inside = vec![0_u64; rows * words];
    let half = width / 2.0;
    let scale = SAMPLES as f64;
    // The sample at `at` along an axis, rounded up or down.
    let first = |at: f64| (at * scale - 0.5).ceil().max(0.0) as usize;
    let last = |at: f64| (at * scale - 0.5).floor().min(rows as f64 - 1.0);
    for (index, pair) in points.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let ends = (index > 0, index + 2 < points.len());
        let bottom = last(a.y.max(b.y) + half);
        if bottom < 0.0 {
            continue;
        }
        for row in first(a.y.min(b.y) - half)..=bottom as usize {
            let y = (row as f64 + 0.5) / scale;
            let Some((left, right)) = span(a, b, half, ends, y) else {
                continue;
            };
            let right = last(right);
            if right < 0.0 {
                continue;
            }
            for column in first(left)..=right as usize {
                inside[row * words + column / 64] |= 1 << (column % 64);
            }
        }
    }
    let mut levels = vec![0_u8; side * side];
    for (at, level) in levels.iter_mut().enumerate() {
        let (x, y) = (at % side, at / side);
        let column = x * SAMPLES;
        let mask = u64::MAX >> (64 - SAMPLES) << (column % 64);
        let count: u32 = (y * SAMPLES..(y + 1) * SAMPLES)
            .map(|row| (inside[row * words + column / 64] & mask).count_ones())
            .sum();
        *level = (f64::from(count) / (scale * scale) * 255.0).round() as u8;
    }
    levels
}

/// Where the row at `y` crosses the segment from `a` to `b` stroked `half` wide on each side, with
/// a round end at `a` and at `b` as `ends` says.
///
/// The segment's outline is convex, so the row crosses it in one span: the hull of where it crosses
/// the rectangle and the round ends.
fn span(
    a: kurbo::Point,
    b: kurbo::Point,
    half: f64,
    ends: (bool, bool),
    y: f64,
) -> Option<(f64, f64)> {
    let hull = |found: Option<(f64, f64)>, left: f64, right: f64| {
        if left > right {
            return found;
        }
        Some(found.map_or((left, right), |(low, high)| {
            (low.min(left), high.max(right))
        }))
    };
    let mut found = None;
    let along = b - a;
    let length = along.hypot();
    if length > 0.0 {
        let (dx, dy) = (along.x / length, along.y / length);
        // Along the segment u = (x − a.x) dx + (y − a.y) dy lies in 0..length, and across it
        // v = −(x − a.x) dy + (y − a.y) dx lies in −half..half.
        let (mut left, mut right) = (f64::NEG_INFINITY, f64::INFINITY);
        for (slope, offset, low, high) in [
            (dx, (y - a.y) * dy, 0.0, length),
            (-dy, (y - a.y) * dx, -half, half),
        ] {
            if slope.abs() < 1e-12 {
                if offset < low || offset > high {
                    right = f64::NEG_INFINITY;
                }
                continue;
            }
            let (p, q) = ((low - offset) / slope, (high - offset) / slope);
            left = left.max(a.x + p.min(q));
            right = right.min(a.x + p.max(q));
        }
        found = hull(found, left, right);
    }
    for (centre, round) in [(a, ends.0), (b, ends.1)] {
        let rise = y - centre.y;
        if round && rise.abs() <= half {
            let reach = (half * half - rise * rise).sqrt();
            found = hull(found, centre.x - reach, centre.x + reach);
        }
    }
    found
}

/// The red channel of a readback, one level a pixel.
fn levels(pixels: &Pixels) -> Vec<u8> {
    let size = pixels.size();
    (0..size.height)
        .flat_map(|y| (0..size.width).map(move |x| pixels.rgba(x, y)[0]))
        .collect()
}

/// How two pictures of one line differ.
#[derive(Debug)]
struct Difference {
    /// Pixels more than sixteen levels apart.
    far: usize,
    /// The mean difference over the pixels either side inks.
    mean: f64,
    /// Pixels `ours` inks by more than sixteen levels where `reference` inks nothing.
    spurious: usize,
}

fn difference(reference: &[u8], ours: &[u8]) -> Difference {
    let (mut far, mut inked, mut summed, mut spurious) = (0, 0_usize, 0_u64, 0);
    for (&a, &b) in reference.iter().zip(ours) {
        let d = a.abs_diff(b);
        far += usize::from(d > 16);
        spurious += usize::from(a == 0 && b > 16);
        if a > 0 || b > 0 {
            inked += 1;
            summed += u64::from(d);
        }
    }
    Difference {
        far,
        mean: summed as f64 / inked.max(1) as f64,
        spurious,
    }
}

/// The reduced and the whole line series over `data` on the marks route, and the true line.
fn lines(data: &Arc<[[f32; 2]]>) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    let line = || Series::Line {
        data: Arc::clone(data),
        to_canvas: PLOT,
        stroke: kurbo::Stroke::new(1.0),
        brush: white(),
    };
    let mut whole = CanvasScene::default();
    whole.push_series(line());
    let mut reduced = CanvasScene::default();
    reduced.push_series_lod(line(), zgui_canvas::Lod::Columns);
    let reduced = Drawing::canvas(&reduced, Affine::IDENTITY);
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    draw_drawing(
        &mut scene,
        VectorId(1),
        &reduced,
        PAINT,
        &CachedMarks::new(),
        PLACEMENT,
    );
    let vertices = scene.primitives.marks[0].vertices;
    // 120 device columns a unit is a bucket of 256 columns.
    assert!(
        vertices <= 4 * 257 + 2,
        "four points a column at most: {vertices} vertices"
    );
    let whole = fresh(&Drawing::canvas(&whole, Affine::IDENTITY))?;
    let reduced = fresh(&reduced)?;
    let truth = true_line(data, PLOT, 1.0, SIDE);
    Some((levels(&reduced), levels(&whole), truth))
}

#[test]
fn a_reduced_dense_line_stays_close_to_the_true_line() {
    // 333 points a device column, smooth within each.
    let Some((reduced, whole, truth)) = lines(&sine(40_000)) else {
        return;
    };
    let ours = difference(&truth, &reduced);
    let theirs = difference(&truth, &whole);
    let against_whole = difference(&whole, &reduced);
    println!("reduced against the true line: {ours:?}");
    println!("whole against the true line: {theirs:?}");
    println!("reduced against whole: {against_whole:?}");
    let inked: u32 = truth.iter().map(|&level| u32::from(level)).sum();
    assert!(inked > 200 * 255, "the line draws");
    assert!(ours.mean <= 4.0, "{ours:?}");
    assert!(
        ours.far as f64 <= 0.005 * f64::from(SIDE * SIDE),
        "{ours:?}"
    );
    assert_eq!(ours.spurious, 0);
    // The whole line keeps the largest coverage of its overlapping segments, so it stays as close.
    assert!(theirs.mean <= 4.0, "{theirs:?}");
    assert_eq!(theirs.spurious, 0);
}

#[test]
fn a_reduced_random_walk_keeps_its_envelope() {
    // 333 points a device column, a fifth of a pixel apart in y: the whole line fills each column
    // between its lowest and highest point, and the reduced line covers each half of it with
    // three strokes. This is why the reduction is never automatic.
    let Some((reduced, whole, truth)) = lines(&walk(40_000)) else {
        return;
    };
    let ours = difference(&truth, &reduced);
    let theirs = difference(&truth, &whole);
    println!("reduced against the true line: {ours:?}");
    println!("whole against the true line: {theirs:?}");
    println!("reduced against whole: {:?}", difference(&whole, &reduced));
    assert_eq!(ours.spurious, 0, "{ours:?}");
    assert!(ours.mean <= 12.0, "{ours:?}");
    assert!(theirs.mean <= 4.0, "{theirs:?}");
}

/// The side of the surface the dense lines are drawn on.
const WIDE: i32 = 320;

/// Data to the wide surface: 300 device columns and 280 device rows a unit.
const WIDE_PLOT: Affine = Affine::new([300.0, 0.0, 0.0, -280.0, 10.0, 300.0]);

/// The whole line series over `data` stroked `width` wide on the marks route and the same line on
/// the general route, on the wide surface.
fn wide_lines(data: &Arc<[[f32; 2]]>, width: f64) -> Option<(Vec<u8>, Vec<u8>)> {
    let mut canvas = CanvasScene::default();
    canvas.push_series(Series::Line {
        data: Arc::clone(data),
        to_canvas: WIDE_PLOT,
        stroke: kurbo::Stroke::new(width),
        brush: white(),
    });
    let drawing = Drawing::canvas(&canvas, Affine::IDENTITY);
    let mut scene = support::scene_at(WIDE);
    quad(
        &mut scene,
        rect(0.0, 0.0, WIDE as f32, WIDE as f32),
        opaque(0, 0, 0),
    );
    draw_drawing(
        &mut scene,
        VectorId(1),
        &drawing,
        PAINT,
        &CachedMarks::new(),
        PLACEMENT,
    );
    assert!(scene.primitives.vectors.is_empty(), "the line is a mark");
    assert!(
        scene.primitives.marks[0].is_union(),
        "the line overlaps itself"
    );
    scene.finish(&DamageSet::full());
    let general = general_line(data, WIDE_PLOT, width, WIDE);
    let mut harness = support::harness_at(WIDE, Which::Vello)?;
    let marks = present(&mut harness.renderer, &scene);
    let general = present(&mut harness.renderer, &general);
    Some((levels(&marks), levels(&general)))
}

#[test]
fn a_dense_line_draws_as_the_true_line() {
    // 67 points a device column. Many segments of one line cross each pixel: the line covers each
    // quarter of the pixel as far as its nearest segment does.
    let mut found = Vec::new();
    for (name, data) in [("sine", sine(20_000)), ("walk", walk(20_000))] {
        for width in [1.0, 2.0] {
            let Some((marks, general)) = wide_lines(&data, width) else {
                return;
            };
            let truth = true_line(&data, WIDE_PLOT, width, WIDE);
            let ours = difference(&truth, &marks);
            println!("{name} {width} px, marks against the true line: {ours:?}");
            println!(
                "{name} {width} px, general against the true line: {:?}",
                difference(&truth, &general)
            );
            found.push((name, width, ours));
        }
    }
    for (name, width, ours) in found {
        assert!(ours.mean <= 4.0, "{name} {width} px: {ours:?}");
        assert_eq!(ours.spurious, 0, "{name} {width} px: {ours:?}");
    }
}
