//! Analytic quads against the general route, on a real device.
//!
//! One list of shapes is drawn twice: once with the analytic route allowed, as quads, and once
//! through the general route, by the path renderer. Where the path renderer paints one flat colour
//! the two pictures have to be equal. Along an edge they may differ by the antialiasing rule: the
//! quad shader ramps coverage over one pixel of distance, and the path renderer measures area.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_color::Color;
use zgui_geom::Matrix4;
use zgui_paint::content::AnalyticOnly;
use zgui_paint::emit::vector::{ShapePaint, VectorPlacement, draw, draw_with_masks};
use zgui_render_wgpu::Pixels;
use zgui_scene::kurbo::{self, BezPath, Circle, RoundedRect, Shape as _};
use zgui_scene::{ClipId, OwnSpace, PropertyOwner, Scene, SpatialId, VectorId, peniko};
use zgui_svg::{Fill, Ink, Paint, Shape, Stroke};

use support::{SIDE, Which, harness, opaque, present, quad, rect};

/// The largest mean difference along the edges, in levels of 255.
const MEAN: f64 = 2.0;

/// The largest difference at one edge pixel: the seam's tolerance between two rasterisers.
const WORST: u8 = 72;

/// A solid paint.
fn solid(color: Color) -> Paint {
    Paint::Solid(Ink::Solid(color))
}

/// White at `alpha`.
fn white(alpha: f32) -> Color {
    Color::srgb(1.0, 1.0, 1.0, alpha)
}

/// A shape filled with `color`.
fn filled(path: BezPath, color: Color) -> Shape {
    Shape {
        path: Arc::new(path),
        fill: Some(Fill {
            paint: solid(color),
            rule: peniko::Fill::NonZero,
        }),
        stroke: None,
        clips: Vec::new(),
    }
}

/// The same shape stroked with `color` in `style`.
fn stroked(mut shape: Shape, color: Color, style: kurbo::Stroke) -> Shape {
    shape.stroke = Some(Stroke {
        paint: solid(color),
        style,
    });
    shape
}

/// A shape that is only stroked.
fn outline(path: BezPath, color: Color, style: kurbo::Stroke) -> Shape {
    let mut shape = filled(path, color);
    shape.fill = None;
    stroked(shape, color, style)
}

/// A circle as a path.
fn circle(x: f64, y: f64, radius: f64) -> BezPath {
    Circle::new((x, y), radius).to_path(0.1)
}

/// `shapes` on black, drawn with the analytic route allowed or through the general route alone,
/// under a uniform `scale`.
fn scene_of(shapes: &[Shape], analytic: bool, scale: f32) -> Scene {
    let mut scene = support::scene();
    quad(
        &mut scene,
        rect(0.0, 0.0, SIDE as f32, SIDE as f32),
        opaque(0, 0, 0),
    );
    let transform = if scale == 1.0 {
        SpatialId::VIEWPORT
    } else {
        let viewport = scene.spatial.viewport();
        let owner = PropertyOwner::new(2).expect("a handle is never the empty word");
        scene.spatial.space_of(
            viewport,
            owner,
            OwnSpace::of(Some(Matrix4::scale(scale, scale, 1.0)), None, false),
        )
    };
    let paint = ShapePaint {
        fill: Color::WHITE,
        stroke: None,
        stroke_width: 1.0,
    };
    let placement = VectorPlacement {
        clip: ClipId::ROOT,
        transform,
        scale: 1.0,
    };
    if analytic {
        draw_with_masks(
            &mut scene,
            VectorId(1),
            shapes,
            paint,
            &AnalyticOnly,
            placement,
        );
    } else {
        draw(&mut scene, VectorId(1), shapes, paint, placement);
    }
    scene.finish(&DamageSet::full());
    scene
}

/// How one picture of some shapes agrees with another.
#[derive(Debug)]
struct Agreement {
    /// Pixels whose neighbourhood the reference paints in one colour.
    interior: u32,
    /// Those of them that a shape covers.
    covered: u32,
    /// The largest channel difference at an interior pixel.
    interior_worst: u8,
    /// Every other pixel that either picture paints.
    edge: u32,
    /// The mean of the largest channel difference over the edge pixels.
    mean: f64,
    /// The largest channel difference at one edge pixel.
    worst: u8,
}

/// How `picture` agrees with `reference`.
fn agreement(picture: &Pixels, reference: &Pixels) -> Agreement {
    let black = [0u8, 0, 0];
    let rgb = |pixels: &Pixels, x: i32, y: i32| {
        let [red, green, blue, _] = pixels.rgba(x, y);
        [red, green, blue]
    };
    let mut found = Agreement {
        interior: 0,
        covered: 0,
        interior_worst: 0,
        edge: 0,
        mean: 0.0,
        worst: 0,
    };
    let mut sum = 0u64;
    for y in 0..SIDE {
        for x in 0..SIDE {
            let want = rgb(reference, x, y);
            let got = rgb(picture, x, y);
            let difference = (0..3).map(|c| got[c].abs_diff(want[c])).max().unwrap_or(0);
            let uniform = (-1..=1).all(|dy| {
                (-1..=1).all(|dx| {
                    let (nx, ny) = ((x + dx).clamp(0, SIDE - 1), (y + dy).clamp(0, SIDE - 1));
                    rgb(reference, nx, ny) == want
                })
            });
            if uniform {
                found.interior += 1;
                found.covered += u32::from(want != black);
                found.interior_worst = found.interior_worst.max(difference);
            } else if got != black || want != black {
                found.edge += 1;
                sum += u64::from(difference);
                found.worst = found.worst.max(difference);
            }
        }
    }
    found.mean = sum as f64 / f64::from(found.edge.max(1));
    found
}

/// The same shapes with every curve, cap and join replaced by lines within a two-hundredth of a
/// device pixel, and every stroke by the outline it covers, filled.
///
/// The path renderer covers a straight edge to within a level and flattens a curve to a quarter
/// of a pixel, so these shapes drawn through it are the true coverage of the originals.
fn precise(shapes: &[Shape], scale: f32) -> Vec<Shape> {
    let tolerance = 0.005 / f64::from(scale);
    let lines = |path: &BezPath| {
        let mut out = BezPath::new();
        kurbo::flatten(path.iter(), tolerance, |element| out.push(element));
        Arc::new(out)
    };
    let mut out = Vec::new();
    for shape in shapes {
        let clips: Vec<zgui_svg::Clip> = shape
            .clips
            .iter()
            .map(|clip| zgui_svg::Clip {
                path: lines(&clip.path),
                rule: clip.rule,
            })
            .collect();
        if let Some(fill) = &shape.fill {
            out.push(Shape {
                path: lines(&shape.path),
                fill: Some(fill.clone()),
                stroke: None,
                clips: clips.clone(),
            });
        }
        if let Some(stroke) = &shape.stroke {
            let covered = kurbo::stroke(
                shape.path.iter(),
                &stroke.style,
                &kurbo::StrokeOpts::default(),
                tolerance,
            );
            out.push(Shape {
                path: lines(&covered),
                fill: Some(Fill {
                    paint: stroke.paint.clone(),
                    rule: peniko::Fill::NonZero,
                }),
                stroke: None,
                clips,
            });
        }
    }
    out
}

/// Draws `shapes` both ways under `scale` and checks the two pictures agree.
///
/// Interior pixels agree to one level, the rounding of a translucent paint through the path
/// renderer's scratch. Edge pixels agree within [`MEAN`] on average and [`WORST`] at most. Where a
/// curve makes them differ by more, both pictures are measured against the true coverage, and the
/// analytic one has to be the closer of the two.
fn compare(name: &str, shapes: &[Shape], scale: f32) {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let quick = scene_of(shapes, true, scale);
    let general = scene_of(shapes, false, scale);
    let exact = scene_of(&precise(shapes, scale), false, scale);
    assert!(
        quick.primitives.vectors.is_empty(),
        "{name}: a shape left the analytic route"
    );
    assert_eq!(
        general.primitives.quads.len(),
        1,
        "{name}: the general picture holds quads besides its background"
    );
    assert!(
        quick.primitives.quads.len() > 1,
        "{name}: the analytic picture drew no quad"
    );
    let by_quads = present(&mut harness.renderer, &quick);
    let by_paths = present(&mut harness.renderer, &general);
    let by_lines = present(&mut harness.renderer, &exact);
    let found = agreement(&by_quads, &by_paths);
    let quads_error = agreement(&by_quads, &by_lines);
    let paths_error = agreement(&by_paths, &by_lines);
    println!("{name}: against the general route {found:?}");
    println!("{name}: analytic against the true coverage {quads_error:?}");
    println!("{name}: general against the true coverage {paths_error:?}");
    assert!(
        found.interior >= 2_000,
        "{name}: only {} interior pixels were compared",
        found.interior
    );
    assert!(
        found.covered >= 100 && found.edge >= 100,
        "{name}: the shapes cover almost nothing: {found:?}"
    );
    assert!(
        found.interior_worst <= 1,
        "{name}: the two differ by {} inside a flat area",
        found.interior_worst
    );
    let close = found.mean <= MEAN && found.worst <= WORST;
    let closer = quads_error.mean <= paths_error.mean && quads_error.worst <= WORST;
    assert!(
        close || closer,
        "{name}: the edges differ by {:.2} on average and {} at most, and the analytic picture is \
         {:.2} from the true coverage where the general one is {:.2}",
        found.mean,
        found.worst,
        quads_error.mean,
        paths_error.mean
    );
}

#[test]
fn analytic_circles_match_the_general_route() {
    let ring = kurbo::Stroke::new(2.0);
    let mut shapes = Vec::new();
    for (row, y) in [(0, 20.3), (1, 62.6), (2, 105.4)] {
        for (x, radius) in [(8.7, 3.5), (27.4, 6.25), (64.6, 18.0)] {
            let shape = filled(circle(x, y, radius), white(1.0));
            shapes.push(match row {
                0 => shape,
                1 => stroked(shape, opaque(255, 64, 0), ring.clone()),
                _ => stroked(shape, Color::srgb(1.0, 0.25, 0.0, 0.5), ring.clone()),
            });
        }
    }
    compare("circles", &shapes, 1.0);
}

#[test]
fn analytic_rects_match_the_general_route() {
    let k = 0.552_284_749_8;
    let mut bar = BezPath::new();
    let (x0, y0, x1, y1, r) = (90.25, 10.5, 110.75, 100.25, 4.0);
    bar.move_to((x0, y1));
    bar.line_to((x1, y1));
    bar.line_to((x1, y0 + r));
    bar.curve_to((x1, y0 + r - k * r), (x1 - r + k * r, y0), (x1 - r, y0));
    bar.line_to((x0 + r, y0));
    bar.curve_to((x0 + r - k * r, y0), (x0, y0 + r - k * r), (x0, y0 + r));
    bar.line_to((x0, y1));
    bar.close_path();
    let shapes = [
        filled(
            kurbo::Rect::new(10.25, 8.5, 40.75, 29.25).to_path(0.1),
            white(1.0),
        ),
        filled(
            RoundedRect::new(10.5, 40.25, 70.5, 80.75, 7.5).to_path(0.1),
            white(1.0),
        ),
        filled(
            kurbo::Rect::new(50.5, 8.25, 80.25, 30.75).to_path(0.1),
            white(0.5),
        ),
        filled(bar, white(1.0)),
    ];
    compare("rects", &shapes, 1.0);
}

#[test]
fn analytic_strokes_match_the_general_route() {
    let mut shapes = Vec::new();
    for (at, cap) in [kurbo::Cap::Butt, kurbo::Cap::Square, kurbo::Cap::Round]
        .into_iter()
        .enumerate()
    {
        let style = kurbo::Stroke::new(3.0).with_caps(cap);
        let y = 10.5 + at as f64 * 12.0;
        shapes.push(outline(
            BezPath::from_svg(&format!("M10.25 {y} L50.75 {y}")).expect("a path"),
            white(1.0),
            style.clone(),
        ));
        let x = 64.5 + at as f64 * 10.0;
        shapes.push(outline(
            BezPath::from_svg(&format!("M{x} 8.25 L{x} 40.5")).expect("a path"),
            white(1.0),
            style,
        ));
    }
    for (at, join) in [kurbo::Join::Miter, kurbo::Join::Round, kurbo::Join::Bevel]
        .into_iter()
        .enumerate()
    {
        let x = 10.25 + at as f64 * 38.0;
        shapes.push(outline(
            kurbo::Rect::new(x, 60.5, x + 26.0, 110.25).to_path(0.1),
            white(1.0),
            kurbo::Stroke::new(4.0).with_join(join),
        ));
    }
    compare("strokes", &shapes, 1.0);
}

#[test]
fn analytic_shapes_under_a_scale_match_the_general_route() {
    for scale in [2.0, 0.8] {
        let inverse = 1.0 / f64::from(scale);
        let shapes = [
            filled(
                circle(30.3 * inverse, 30.6 * inverse, 16.0 * inverse),
                white(1.0),
            ),
            filled(
                RoundedRect::new(
                    60.25 * inverse,
                    12.5 * inverse,
                    118.5 * inverse,
                    60.75 * inverse,
                    6.0 * inverse,
                )
                .to_path(0.1),
                white(1.0),
            ),
            outline(
                kurbo::Rect::new(
                    12.5 * inverse,
                    72.25 * inverse,
                    110.75 * inverse,
                    116.5 * inverse,
                )
                .to_path(0.1),
                white(1.0),
                kurbo::Stroke::new(3.0 * inverse),
            ),
        ];
        compare(&format!("scale {scale}"), &shapes, scale);
    }
}

#[test]
fn an_analytic_whole_pixel_clip_matches_the_general_route() {
    let mut shape = filled(
        kurbo::Rect::new(8.0, 8.0, 120.0, 120.0).to_path(0.1),
        white(1.0),
    );
    shape.clips.push(zgui_svg::Clip {
        path: Arc::new(kurbo::Rect::new(20.0, 24.0, 100.0, 104.0).to_path(0.1)),
        rule: peniko::Fill::NonZero,
    });
    compare("clip", &[shape], 1.0);
}
