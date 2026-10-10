//! Repeated outlines drawn as path glyphs against the same outlines through the path renderer.
//!
//! On the quarter-pixel grid a cell holds the exact coverage, so the glyphs have to agree with the
//! general route as the other recognising routes do. Off the grid a copy is drawn from the
//! nearest phase, which moves it by at most an eighth of a pixel.

mod support;

use std::sync::Arc;

use zgui_atlas::AtlasLimits;
use zgui_bits::DamageSet;
use zgui_canvas::{Brush, CanvasScene, Marker, Series};
use zgui_color::Color;
use zgui_geom::{Affine2, Size};
use zgui_paint::ContentCache;
use zgui_paint::content::Drawing;
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement, draw_drawing};
use zgui_profile::Counter;
use zgui_render_wgpu::Pixels;
use zgui_scene::kurbo::{self, Affine, BezPath};
use zgui_scene::{ChunkPrims, ClipId, Scene, SpatialId, VectorId, peniko};
use zgui_svg::{Fill, Ink, Paint, Shape, Stroke};

use support::conformance::{
    Route, compare, judge, precise, scene_of, scene_of_glyphs, with_glyphs,
};
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

/// Adds a triangle with its top vertex at `(x, y)`, `half` wide on each side and `height` tall.
fn triangle(path: &mut BezPath, (x, y): (f64, f64), half: f64, height: f64) {
    path.move_to((x, y));
    path.line_to((x + half, y + height));
    path.line_to((x - half, y + height));
    path.close_path();
}

/// One filled shape of triangles at `anchors`, in the inherited colour at `alpha`.
fn triangles(anchors: &[(f64, f64)], half: f64, height: f64, alpha: f32) -> Shape {
    let mut path = BezPath::new();
    for &anchor in anchors {
        triangle(&mut path, anchor, half, height);
    }
    Shape {
        path: Arc::new(path),
        fill: Some(Fill {
            paint: Paint::Solid(Ink::Inherited { alpha }),
            rule: peniko::Fill::NonZero,
        }),
        stroke: None,
        clips: Vec::new(),
    }
}

/// Anchors on a `columns` by `rows` grid `step` apart from `(left, top)`, each moved by a
/// different quarter pixel.
fn quarter_grid(
    columns: usize,
    rows: usize,
    step: f64,
    (left, top): (f64, f64),
) -> Vec<(f64, f64)> {
    (0..columns * rows)
        .map(|index| {
            let (column, row) = (index % columns, index / columns);
            (
                left + column as f64 * step + (index % 4) as f64 / 4.0,
                top + row as f64 * step + ((index / 4) % 4) as f64 / 4.0,
            )
        })
        .collect()
}

#[test]
fn separated_triangles_match_the_true_coverage() {
    let shape = triangles(&quarter_grid(8, 8, 15.0, (8.0, 6.0)), 4.5, 9.0, 1.0);
    compare("separated", &[shape], Affine2::IDENTITY, Route::Glyphs);
}

#[test]
fn overlapping_triangles_match_the_true_coverage() {
    let shape = triangles(&quarter_grid(20, 20, 5.5, (8.0, 6.0)), 4.5, 9.0, 1.0);
    compare("overlapping", &[shape], Affine2::IDENTITY, Route::Glyphs);
}

#[test]
fn stroked_crosses_match_the_true_coverage() {
    let mut path = BezPath::new();
    for (x, y) in quarter_grid(6, 6, 19.0, (10.0, 10.0)) {
        path.move_to((x - 6.0, y));
        path.line_to((x + 6.0, y));
        path.move_to((x, y - 6.0));
        path.line_to((x, y + 6.0));
    }
    let shape = Shape {
        path: Arc::new(path),
        fill: None,
        stroke: Some(Stroke {
            paint: Paint::Solid(Ink::Inherited { alpha: 1.0 }),
            style: kurbo::Stroke::new(5.0).with_caps(kurbo::Cap::Round),
        }),
        clips: Vec::new(),
    };
    compare("crosses", &[shape], Affine2::IDENTITY, Route::Glyphs);
}

#[test]
fn glyphs_under_a_scale_and_a_mirror_match_the_true_coverage() {
    let small = triangles(&quarter_grid(8, 8, 7.5, (4.0, 3.0)), 2.25, 4.5, 1.0);
    compare(
        "scale(2)",
        std::slice::from_ref(&small),
        Affine2::new(2.0, 0.0, 0.0, 2.0, 0.0, 0.0),
        Route::Glyphs,
    );
    let shape = triangles(&quarter_grid(8, 8, 15.0, (8.0, 6.0)), 4.5, 9.0, 1.0);
    compare(
        "scale(-1, 1)",
        &[shape],
        Affine2::new(-1.0, 0.0, 0.0, 1.0, SIDE as f32, 0.0),
        Route::Glyphs,
    );
}

/// The outline of [`triangle`] about the origin.
fn marker() -> Arc<BezPath> {
    let mut path = BezPath::new();
    triangle(&mut path, (0.0, 0.0), 4.5, 9.0);
    Arc::new(path)
}

/// A points series of a triangle marker at `data`, mapped by `to_canvas`.
fn markers(data: Arc<[[f32; 2]]>, to_canvas: Affine, alpha: f32) -> Series {
    Series::Points {
        data,
        to_canvas,
        marker: Marker::Path(marker()),
        fill: Some(Brush::Solid(Color::srgb(1.0, 1.0, 1.0, alpha))),
        stroke: None,
    }
}

/// The drawing of a canvas holding `series` under `view`.
fn canvas(series: Series, view: Affine) -> Drawing {
    let mut scene = CanvasScene::default();
    scene.push_series(series);
    scene.set_transform(view);
    Drawing::canvas(&scene, Affine::IDENTITY)
}

/// The scene of `drawing` on black, drawn with `masks`, encoded as chunk `revision`.
fn encode(
    drawing: &Drawing,
    masks: &dyn zgui_paint::content::VectorMaskSource,
    revision: u64,
    retired: Option<u64>,
    scene: &mut Scene,
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

/// Points of a quarter grid, as data the identity maps to the canvas.
fn grid_data(columns: usize, rows: usize, step: f64) -> Arc<[[f32; 2]]> {
    quarter_grid(columns, rows, step, (8.0, 6.0))
        .into_iter()
        .map(|(x, y)| [x as f32, y as f32])
        .collect()
}

#[test]
fn a_path_marker_series_matches_its_shapes() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let anchors = quarter_grid(20, 20, 5.5, (8.0, 6.0));
    let data: Arc<[[f32; 2]]> = anchors.iter().map(|&(x, y)| [x as f32, y as f32]).collect();
    let drawing = canvas(markers(data, Affine::IDENTITY, 1.0), Affine::IDENTITY);
    let mut content = ContentCache::new(AtlasLimits::default());
    let mut quick = Scene::new();
    with_glyphs(&mut content, &mut harness.renderer, |glyphs| {
        encode(&drawing, glyphs, 1, None, &mut quick);
    });
    let shapes = [triangles(&anchors, 4.5, 9.0, 1.0)];
    let general = scene_of(&shapes, None, Affine2::IDENTITY);
    let exact = scene_of(
        &precise(&shapes, Affine2::IDENTITY),
        None,
        Affine2::IDENTITY,
    );
    judge(
        "series",
        &mut harness.renderer,
        &quick,
        &general,
        &exact,
        Route::Glyphs,
    );
}

/// The centroid of the coverage in the `size` square at `(left, top)`, white on black.
fn centroid(pixels: &Pixels, (left, top): (i32, i32), size: i32) -> [f64; 2] {
    let mut sum = [0.0f64; 3];
    for y in top..top + size {
        for x in left..left + size {
            let value = f64::from(pixels.rgba(x, y)[0]);
            sum = [
                sum[0] + value * (f64::from(x) + 0.5),
                sum[1] + value * (f64::from(y) + 0.5),
                sum[2] + value,
            ];
        }
    }
    [sum[0] / sum[2], sum[1] / sum[2]]
}

#[test]
fn a_glyph_lands_within_an_eighth_of_a_pixel() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut fraction = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    // Forty triangles 15 pixels apart, each at its own fraction of a pixel.
    let anchors: Vec<(f64, f64)> = (0..40)
        .map(|index| {
            let (column, row) = ((index % 8) as f64, (index / 8) as f64);
            (
                12.0 + 15.0 * column + fraction(),
                6.0 + 15.0 * row + fraction(),
            )
        })
        .collect();
    let shapes = [triangles(&anchors, 4.5, 9.0, 1.0)];
    let mut content = ContentCache::new(AtlasLimits::default());
    let quick = scene_of_glyphs(
        &shapes,
        Affine2::IDENTITY,
        &mut content,
        &mut harness.renderer,
    );
    assert!(quick.primitives.vectors.is_empty() && !quick.primitives.marks.is_empty());
    let exact = scene_of(
        &precise(&shapes, Affine2::IDENTITY),
        None,
        Affine2::IDENTITY,
    );
    let glyphs = present(&mut harness.renderer, &quick);
    let truth = present(&mut harness.renderer, &exact);
    let mut worst = 0.0f64;
    for &(x, y) in &anchors {
        let window = ((x - 7.0).floor() as i32, (y - 2.0).floor() as i32);
        let [gx, gy] = centroid(&glyphs, window, 14);
        let [tx, ty] = centroid(&truth, window, 14);
        worst = worst.max((gx - tx).abs()).max((gy - ty).abs());
    }
    println!("the farthest glyph lands {worst:.4} pixels from its true centroid");
    assert!(
        worst <= 1.0 / 8.0 + 1.0 / 64.0,
        "a glyph lands {worst:.4} pixels from its true centroid"
    );
}

#[test]
fn a_translucent_overlap_of_glyphs_paints_alpha_once() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    // Two triangles, the second a pixel and a half right of the first, and six more apart.
    let mut anchors = vec![(40.0, 30.0), (41.5, 30.0)];
    anchors.extend((0..6).map(|index| (10.0 + 20.0 * f64::from(index), 90.0)));
    let shapes = [triangles(&anchors, 9.0, 18.0, 0.5)];
    let mut content = ContentCache::new(AtlasLimits::default());
    let quick = scene_of_glyphs(
        &shapes,
        Affine2::IDENTITY,
        &mut content,
        &mut harness.renderer,
    );
    let [mark] = quick.primitives.marks.as_slice() else {
        panic!("one glyph mark");
    };
    assert!(mark.is_union(), "the two overlap");
    let pixels = present(&mut harness.renderer, &quick);
    let both = pixels.rgba(41, 44)[0];
    assert!(
        both.abs_diff(128) <= 1,
        "the overlap paints alpha once: {both}"
    );
}

#[test]
fn a_panned_path_marker_series_uploads_no_payload() {
    let data = grid_data(16, 16, 7.0);
    let series = || markers(Arc::clone(&data), Affine::IDENTITY, 0.5);
    let pan = Affine::translate((13.25, -7.5));
    let panned = {
        let Some(mut harness) = harness(Which::Vello) else {
            return;
        };
        let mut content = ContentCache::new(AtlasLimits::default());
        let mut scene = Scene::new();
        let mut last = None;
        for (at, view) in [Affine::IDENTITY, pan].into_iter().enumerate() {
            let drawing = canvas(series(), view);
            let revision = at as u64 + 1;
            with_glyphs(&mut content, &mut harness.renderer, |glyphs| {
                encode(
                    &drawing,
                    glyphs,
                    revision,
                    (revision > 1).then(|| revision - 1),
                    &mut scene,
                );
            });
            zgui_profile::counter::reset();
            let pixels = present(&mut harness.renderer, &scene);
            scene.clear_chunk_notes();
            last = Some((
                zgui_profile::counter::get(Counter::MarksPayloadBytes),
                pixels,
            ));
        }
        last.expect("two frames")
    };
    let (bytes, pixels) = panned;
    if zgui_profile::COUNTERS_ENABLED {
        assert_eq!(bytes, 0, "the pan uploads no payload");
    }
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut content = ContentCache::new(AtlasLimits::default());
    let mut scene = Scene::new();
    let drawing = canvas(series(), pan);
    with_glyphs(&mut content, &mut harness.renderer, |glyphs| {
        encode(&drawing, glyphs, 1, None, &mut scene);
    });
    assert!(
        !scene.primitives.marks.is_empty(),
        "the series draws as glyphs"
    );
    let expected = present(&mut harness.renderer, &scene);
    assert!(
        (0..SIDE).any(|x| pixels.rgba(x, 40)[0] > 0),
        "the series draws"
    );
    assert_eq!(pixels.max_difference(&expected), 0);
}
