//! Marks against the general route and the true coverage, on a real device.
//!
//! One list of shapes is drawn through the marks route and through the general route, by the path
//! renderer. See [`support::conformance`] for the rule. Two more tests check what only a union
//! gets right: a translucent overlap painted once, and two abutting boxes with no seam. The last
//! checks that strokes and rings thinner than a pixel cover their width.

mod support;

use std::sync::Arc;

use zgui_color::Color;
use zgui_geom::Affine2;
use zgui_scene::kurbo::{self, BezPath, Circle, RoundedRect, Shape as _};
use zgui_scene::peniko;
use zgui_svg::{Fill, Ink, Paint, Shape, Stroke};

use support::conformance::{self, Route, scene_of};
use support::{Which, harness, opaque, present};

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

/// A shape that is only stroked, with `color` in `style`.
fn outline(path: BezPath, color: Color, style: kurbo::Stroke) -> Shape {
    Shape {
        path: Arc::new(path),
        fill: None,
        stroke: Some(Stroke {
            paint: solid(color),
            style,
        }),
        clips: Vec::new(),
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

/// Discs that overlap, discs that are apart, and rings.
fn discs_and_rings() -> Vec<Shape> {
    vec![
        filled(
            circles(&[(20.3, 20.6, 9.5), (31.7, 24.2, 8.25), (25.4, 33.1, 6.0)]),
            white(1.0),
        ),
        filled(
            circles(&[(70.5, 14.25, 5.5), (90.75, 14.5, 5.5), (110.25, 14.75, 5.5)]),
            white(1.0),
        ),
        outline(
            circles(&[(40.4, 80.6, 14.0), (60.2, 86.3, 12.5)]),
            white(1.0),
            kurbo::Stroke::new(3.0),
        ),
        filled(
            circles(&[(100.3, 70.6, 12.0), (104.6, 98.4, 13.25)]),
            Color::srgb(1.0, 0.5, 0.25, 1.0),
        ),
    ]
}

#[test]
fn marks_discs_and_rings_match_the_true_coverage() {
    conformance::compare(
        "discs and rings",
        &discs_and_rings(),
        Affine2::IDENTITY,
        Route::Marks,
    );
}

#[test]
fn marks_boxes_match_the_true_coverage() {
    let mut boxes = kurbo::Rect::new(10.25, 8.5, 40.75, 29.25).to_path(0.1);
    boxes.extend(
        kurbo::Rect::new(30.5, 20.25, 60.25, 50.75)
            .to_path(0.1)
            .elements()
            .iter()
            .copied(),
    );
    let mut rounded = RoundedRect::new(10.5, 60.25, 70.5, 100.75, 7.5).to_path(0.1);
    rounded.extend(
        RoundedRect::new(50.25, 80.5, 118.75, 118.25, 4.0)
            .to_path(0.1)
            .elements()
            .iter()
            .copied(),
    );
    let shapes = vec![
        filled(boxes, white(1.0)),
        filled(rounded, white(1.0)),
        outline(
            kurbo::Rect::new(76.5, 10.25, 116.75, 50.5).to_path(0.1),
            white(1.0),
            kurbo::Stroke::new(4.0).with_join(kurbo::Join::Round),
        ),
    ];
    conformance::compare("boxes", &shapes, Affine2::IDENTITY, Route::Marks);
}

/// Strokes with every cap, round joins, and a slanted run.
fn polylines() -> Vec<Shape> {
    let mut shapes = Vec::new();
    for (at, cap) in [kurbo::Cap::Butt, kurbo::Cap::Square, kurbo::Cap::Round]
        .into_iter()
        .enumerate()
    {
        let y = 12.5 + at as f64 * 14.0;
        shapes.push(outline(
            BezPath::from_svg(&format!("M10.25 {y} L40.75 {} L70.5 {y}", y + 8.0)).expect("a path"),
            white(1.0),
            kurbo::Stroke::new(3.0).with_caps(cap),
        ));
    }
    shapes.push(outline(
        BezPath::from_svg("M14.5 70.25 L40.25 112.5 L80.75 76.25 L116.5 110.75").expect("a path"),
        white(1.0),
        kurbo::Stroke::new(5.0),
    ));
    shapes.push(outline(
        BezPath::from_svg("M84.5 10.25 L118.25 50.5").expect("a path"),
        white(1.0),
        kurbo::Stroke::new(2.5).with_caps(kurbo::Cap::Square),
    ));
    shapes
}

#[test]
fn marks_polylines_match_the_true_coverage() {
    conformance::compare("polylines", &polylines(), Affine2::IDENTITY, Route::Marks);
}

/// `placement` composed so the picture stays centred on the surface.
fn centred(placement: Affine2) -> Affine2 {
    let centre = 64.0;
    Affine2::translation(-centre, -centre)
        .then(placement)
        .then(Affine2::translation(centre, centre))
}

#[test]
fn marks_under_a_turn_match_the_true_coverage() {
    let turn = centred(Affine2::rotation(30.0_f32.to_radians()));
    conformance::compare("turned discs", &discs_and_rings(), turn, Route::Marks);
    conformance::compare("turned polylines", &polylines(), turn, Route::Marks);
}

#[test]
fn marks_under_a_non_uniform_scale_match_the_true_coverage() {
    for (name, placement) in [
        ("scale(2, 1)", centred(Affine2::scale(2.0, 1.0))),
        ("scale(0.5)", centred(Affine2::scale(0.5, 0.5))),
    ] {
        conformance::compare(name, &discs_and_rings(), placement, Route::Marks);
    }
}

#[test]
fn a_translucent_overlap_paints_alpha_once() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let shapes = [filled(
        circles(&[(50.0, 64.0, 24.0), (78.0, 64.0, 24.0)]),
        white(0.5),
    )];
    let scene = scene_of(&shapes, Some(Route::Marks), Affine2::IDENTITY);
    assert!(scene.primitives.marks[0].is_union());
    let pixels = present(&mut harness.renderer, &scene);
    let single = pixels.rgba(40, 64)[0];
    let overlap = pixels.rgba(64, 64)[0];
    println!("translucent overlap: one disc {single}, both discs {overlap}");
    assert!(
        single.abs_diff(overlap) <= 1,
        "the overlap paints alpha once ({overlap}), as one disc does ({single}), never twice"
    );
    assert!(
        (126..=129).contains(&single),
        "half white on black: {single}"
    );
}

#[test]
fn abutting_boxes_leave_no_seam() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    let mut path = kurbo::Rect::new(0.0, 20.0, 10.5, 100.0).to_path(0.1);
    path.extend(
        kurbo::Rect::new(10.5, 20.0, 20.0, 100.0)
            .to_path(0.1)
            .elements()
            .iter()
            .copied(),
    );
    let shapes = [filled(path, opaque(255, 255, 255))];
    let scene = scene_of(&shapes, Some(Route::Marks), Affine2::IDENTITY);
    assert!(scene.primitives.marks[0].is_union());
    let pixels = present(&mut harness.renderer, &scene);
    for y in [30, 60, 90] {
        let interior = pixels.rgba(5, y)[0];
        let seam = pixels.rgba(10, y)[0];
        println!("abutting boxes, row {y}: interior {interior}, column 10 {seam}");
        assert!(
            interior.abs_diff(seam) <= 1,
            "column 10 at row {y} is {seam} where the interior is {interior}"
        );
    }
}

/// Polylines stroked `width` wide: level, upright and slanted, on and between pixel centres.
fn hairline_polylines(width: f64) -> Vec<Shape> {
    [
        "M8.25 10.5 L120.75 10.5",
        "M8.5 20 L120.25 20",
        "M8.75 30.25 L120.5 30.25",
        "M10.5 40.25 L10.5 120.75",
        "M20.25 44.5 L64.75 76.25 L118.5 50.75",
        "M20.5 118.25 L70.25 88.5 L118.75 112.25",
    ]
    .into_iter()
    .map(|path| {
        outline(
            BezPath::from_svg(path).expect("a path"),
            white(1.0),
            kurbo::Stroke::new(width),
        )
    })
    .collect()
}

/// Rings stroked `width` wide.
fn hairline_rings(width: f64) -> Vec<Shape> {
    vec![outline(
        circles(&[
            (24.5, 24.5, 14.0),
            (64.25, 30.75, 18.5),
            (100.6, 28.3, 12.25),
            (36.4, 84.2, 24.0),
            (96.5, 92.5, 20.5),
        ]),
        white(1.0),
        kurbo::Stroke::new(width),
    )]
}

/// The coverage a picture of white on black holds, in pixels.
fn coverage(pixels: &zgui_render_wgpu::Pixels) -> f64 {
    let mut sum = 0u64;
    for y in 0..support::SIDE {
        for x in 0..support::SIDE {
            sum += u64::from(pixels.rgba(x, y)[0]);
        }
    }
    sum as f64 / 255.0
}

#[test]
fn hairlines_cover_their_width() {
    let Some(mut harness) = harness(Which::Vello) else {
        return;
    };
    for width in [0.25, 0.5] {
        for (name, shapes) in [
            ("polylines", hairline_polylines(width)),
            ("rings", hairline_rings(width)),
        ] {
            let quick = scene_of(&shapes, Some(Route::Marks), Affine2::IDENTITY);
            assert!(
                quick.primitives.vectors.is_empty() && !quick.primitives.marks.is_empty(),
                "{name} {width}: a shape left the marks route"
            );
            let exact = scene_of(
                &conformance::precise(&shapes, Affine2::IDENTITY),
                None,
                Affine2::IDENTITY,
            );
            let by_marks = coverage(&present(&mut harness.renderer, &quick));
            let by_lines = coverage(&present(&mut harness.renderer, &exact));
            println!(
                "{name} {width}: marks cover {by_marks:.1} pixels, the true coverage {by_lines:.1}"
            );
            assert!(
                (by_marks / by_lines - 1.0).abs() <= 0.02,
                "{name} {width}: marks cover {by_marks:.1} pixels where the true coverage is \
                 {by_lines:.1}"
            );
        }
    }
}
