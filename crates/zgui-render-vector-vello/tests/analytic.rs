//! Analytic quads against the general route, on a real device.
//!
//! One list of shapes is drawn twice: once with the analytic route allowed, as quads, and once
//! through the general route, by the path renderer. See [`support::conformance`] for the rule.

mod support;

use std::sync::Arc;

use zgui_color::Color;
use zgui_geom::Affine2;
use zgui_scene::kurbo::{self, BezPath, Circle, RoundedRect, Shape as _};
use zgui_scene::peniko;
use zgui_svg::{Fill, Ink, Paint, Shape, Stroke};

use support::conformance::{self, Route};
use support::opaque;

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

/// Draws `shapes` through the analytic route and the general route under a uniform `scale`, and
/// checks the two pictures agree.
fn compare(name: &str, shapes: &[Shape], scale: f32) {
    conformance::compare(name, shapes, Affine2::scale(scale, scale), Route::Analytic);
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
