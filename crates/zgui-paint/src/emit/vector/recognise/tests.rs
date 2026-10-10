use zgui_geom::Affine2;
use zgui_scene::kurbo::{
    self, BezPath, Circle, Ellipse, PathEl, Point, Rect, RoundedRect, RoundedRectRadii, Shape as _,
};
use zgui_scene::peniko;

use super::{
    BUTT, Decomposition, Limits, Orientation, Part, ROUND, SQUARE, recognise, separated, tau,
};

/// One sixteenth of a pixel, which is tau under no transform.
const TAU: f64 = 1.0 / 16.0;

/// The limits a shape under no transform is recognised with.
fn limits() -> Limits {
    Limits {
        tau: TAU,
        max_prims: 256,
    }
}

/// What filling `path` is made of.
fn filled(path: &BezPath) -> Option<Decomposition> {
    recognise(path, Part::Fill(peniko::Fill::NonZero), limits())
}

/// What stroking `path` with `style` is made of.
fn stroked(path: &BezPath, style: &kurbo::Stroke) -> Option<Decomposition> {
    recognise(path, Part::Stroke(style), limits())
}

/// Whether two lists of numbers agree to within a thousandth.
fn close<const N: usize>(one: [f32; N], two: [f32; N]) -> bool {
    one.iter().zip(two).all(|(a, b)| (a - b).abs() <= 1.0e-3)
}

/// A copy of the plot crate's circle: four cubics with the classic arm and a close.
fn plot_circle(path: &mut BezPath, (x, y): (f64, f64), radius: f64) {
    let kappa = 0.552_284_749_8 * radius;
    path.move_to((x + radius, y));
    path.curve_to(
        (x + radius, y + kappa),
        (x + kappa, y + radius),
        (x, y + radius),
    );
    path.curve_to(
        (x - kappa, y + radius),
        (x - radius, y + kappa),
        (x - radius, y),
    );
    path.curve_to(
        (x - radius, y - kappa),
        (x - kappa, y - radius),
        (x, y - radius),
    );
    path.curve_to(
        (x + kappa, y - radius),
        (x + radius, y - kappa),
        (x + radius, y),
    );
    path.close_path();
}

/// A copy of the plot crate's bar: a rectangle with rounded top corners, which draws an explicit
/// line back to its start before it closes.
fn plot_top_rounded_rect(path: &mut BezPath, [x0, y0, x1, y1]: [f64; 4], radius: f64) {
    let radius = radius.min((x1 - x0) / 2.0).min((y1 - y0) / 2.0).max(0.0);
    let kappa = 0.552_284_749_8;
    path.move_to((x0, y1));
    path.line_to((x1, y1));
    path.line_to((x1, y0 + radius));
    path.curve_to(
        (x1, y0 + radius - kappa * radius),
        (x1 - radius + kappa * radius, y0),
        (x1 - radius, y0),
    );
    path.line_to((x0 + radius, y0));
    path.curve_to(
        (x0 + radius - kappa * radius, y0),
        (x0, y0 + radius - kappa * radius),
        (x0, y0 + radius),
    );
    path.line_to((x0, y1));
    path.close_path();
}

/// A plot circle with control points of its first cubic moved away from the centre by `by`.
fn perturbed(radius: f64, by: [f64; 2]) -> BezPath {
    let centre = Point::new(20.0, 20.0);
    let mut path = BezPath::new();
    plot_circle(&mut path, (centre.x, centre.y), radius);
    let mut elements: Vec<PathEl> = path.elements().to_vec();
    let PathEl::CurveTo(first, second, end) = elements[1] else {
        panic!("a plot circle starts with a cubic");
    };
    let out = |point: Point, by: f64| {
        let radial = point - centre;
        point + radial / radial.hypot() * by
    };
    elements[1] = PathEl::CurveTo(out(first, by[0]), out(second, by[1]), end);
    BezPath::from_vec(elements)
}

#[test]
fn a_kurbo_circle_is_one_disc_at_every_tolerance() {
    for radius in [3.5, 40.0] {
        for tolerance in [1.0, 0.1, 0.01, 1.0e-3] {
            let path = Circle::new((10.25, 20.5), radius).to_path(tolerance);
            let found = filled(&path)
                .unwrap_or_else(|| panic!("radius {radius} at tolerance {tolerance} was declined"));
            assert_eq!(found.count, 1);
            assert_eq!(
                found.discs.len(),
                1,
                "radius {radius}, tolerance {tolerance}"
            );
            assert!(
                close(found.discs[0], [10.25, 20.5, radius as f32, 0.0]),
                "{:?}",
                found.discs[0]
            );
        }
    }
}

#[test]
fn a_plot_circle_is_one_disc() {
    let mut path = BezPath::new();
    plot_circle(&mut path, (7.5, 9.25), 3.5);
    let found = filled(&path).expect("a plot circle is a disc");
    assert_eq!(found.discs.len(), 1);
    assert!(close(found.discs[0], [7.5, 9.25, 3.5, 0.0]));
    assert!(close(found.ink, [4.0, 5.75, 11.0, 12.75]));
}

#[test]
fn a_reversed_circle_is_one_disc_turning_the_other_way() {
    let path = Circle::new((10.0, 10.0), 5.0).to_path(0.1);
    let forward = filled(&path).expect("a circle");
    let backward = filled(&path.reverse_subpaths()).expect("the same circle, reversed");
    assert_eq!(backward.discs, forward.discs);
    assert_ne!(forward.orientation, backward.orientation);
    assert_ne!(backward.orientation, Orientation::Mixed);
}

#[test]
fn an_axis_aligned_ellipse_is_a_box_with_full_elliptical_radii() {
    for (rx, ry) in [(20.0, 8.0), (8.0, 20.0)] {
        let path = Ellipse::new((30.0, 25.0), (rx, ry), 0.0).to_path(0.01);
        let found = filled(&path).unwrap_or_else(|| panic!("{rx} by {ry} was declined"));
        assert!(found.discs.is_empty());
        assert_eq!(found.boxes.len(), 1);
        let prim = found.boxes[0];
        let (rx, ry) = (rx as f32, ry as f32);
        assert!(close(
            prim.rect,
            [30.0 - rx, 25.0 - ry, 30.0 + rx, 25.0 + ry]
        ));
        assert!(close(prim.radii, [rx, ry, rx, ry, rx, ry, rx, ry]));
        assert_eq!(prim.exponent, 2.0);
        assert_eq!(prim.border, 0.0);
    }
}

#[test]
fn a_rotated_ellipse_is_declined() {
    let path = Ellipse::new((30.0, 25.0), (20.0, 8.0), 0.3).to_path(0.01);
    assert_eq!(filled(&path), None);
}

#[test]
fn a_circle_perturbed_by_one_and_a_half_tau_is_declined() {
    // Both control points moved out by 2 tau move the curve's midpoint out by 1.5 tau.
    assert_eq!(filled(&perturbed(10.0, [2.0 * TAU, 2.0 * TAU])), None);
    // A quarter of tau moves it by less than a fifth.
    let found = filled(&perturbed(10.0, [0.25 * TAU, 0.25 * TAU])).expect("within tolerance");
    assert_eq!(found.discs.len(), 1);
}

#[test]
fn tau_follows_the_largest_scale_of_the_transform() {
    let stretched = tau(&Affine2::new(4.0, 0.0, 0.0, 1.0, 0.0, 0.0)).expect("invertible");
    assert!((stretched - TAU / 4.0).abs() < 1.0e-12);
    let (sin, cos) = 0.5_f32.sin_cos();
    let turned = tau(&Affine2::new(cos, sin, -sin, cos, 5.0, 7.0)).expect("invertible");
    assert!((turned - TAU).abs() < 1.0e-6, "a turn stretches nothing");
    assert_eq!(tau(&Affine2::new(1.0, 2.0, 2.0, 4.0, 0.0, 0.0)), None);

    // One control point moved out by 0.75 tau moves the curve by at most a third of tau.
    let path = perturbed(10.0, [0.75 * TAU, 0.0]);
    assert!(filled(&path).is_some(), "within tolerance at scale 1");
    let at_four = Limits {
        tau: stretched,
        max_prims: 256,
    };
    assert_eq!(
        recognise(&path, Part::Fill(peniko::Fill::NonZero), at_four),
        None,
        "four times the error at scale 4"
    );
}

#[test]
fn a_kurbo_rect_and_rounded_rect_are_one_box() {
    for tolerance in [0.1, 1.0e-3] {
        let rect = Rect::new(1.5, 2.25, 30.5, 20.0);
        let found = filled(&rect.to_path(tolerance)).expect("a rectangle");
        assert_eq!(found.boxes.len(), 1);
        assert!(close(found.boxes[0].rect, [1.5, 2.25, 30.5, 20.0]));
        assert_eq!(found.boxes[0].radii, [0.0; 8]);

        // The bottom right corner has no radius, which kurbo writes as one degenerate cubic.
        let rounded = RoundedRect::from_rect(rect, RoundedRectRadii::new(4.0, 6.0, 0.0, 3.0));
        let found = filled(&rounded.to_path(tolerance))
            .unwrap_or_else(|| panic!("a rounded rectangle at {tolerance}"));
        assert_eq!(found.boxes.len(), 1);
        assert!(close(found.boxes[0].rect, [1.5, 2.25, 30.5, 20.0]));
        assert!(close(
            found.boxes[0].radii,
            [4.0, 4.0, 6.0, 6.0, 0.0, 0.0, 3.0, 3.0]
        ));
    }
}

#[test]
fn a_plot_top_rounded_rect_is_one_box() {
    let mut path = BezPath::new();
    plot_top_rounded_rect(&mut path, [10.0, 5.0, 18.0, 40.0], 2.0);
    let found = filled(&path).expect("a bar");
    assert!(close(found.boxes[0].rect, [10.0, 5.0, 18.0, 40.0]));
    assert!(close(
        found.boxes[0].radii,
        [2.0, 2.0, 2.0, 2.0, 0.0, 0.0, 0.0, 0.0]
    ));

    // A radius of half the width leaves the top edge with no length.
    let mut path = BezPath::new();
    plot_top_rounded_rect(&mut path, [10.0, 5.0, 18.0, 40.0], 4.0);
    let found = filled(&path).expect("a bar with a round top");
    assert!(close(
        found.boxes[0].radii,
        [4.0, 4.0, 4.0, 4.0, 0.0, 0.0, 0.0, 0.0]
    ));
}

#[test]
fn a_quadrilateral_with_one_slanted_edge_is_declined() {
    let path = BezPath::from_svg("M0 0 L10 0 L9 10 L0 10 Z").expect("a path");
    assert_eq!(filled(&path), None);
}

#[test]
fn separate_subpaths_are_separate_prims() {
    let mut path = BezPath::new();
    for x in [10.0, 30.0, 50.0] {
        plot_circle(&mut path, (x, 10.0), 4.0);
    }
    path.extend(Rect::new(0.0, 30.0, 12.0, 36.0).path_elements(0.1));
    path.extend(Rect::new(20.0, 30.0, 25.0, 50.0).path_elements(0.1));
    let found = filled(&path).expect("circles and rectangles");
    assert_eq!(found.discs.len(), 3);
    assert_eq!(found.boxes.len(), 2);
    assert_eq!(found.count, 5);
    assert!(close(found.ink, [0.0, 6.0, 54.0, 50.0]));
    assert_eq!(found.max_extent, 20.0);
}

#[test]
fn a_round_capped_horizontal_line_is_one_capsule() {
    let path = BezPath::from_svg("M3 12 L21 12").expect("a path");
    let found = stroked(&path, &kurbo::Stroke::new(2.0)).expect("a capsule");
    assert_eq!(found.capsules, [[3.0, 12.0, 21.0, 12.0]]);
    assert_eq!(found.caps, [ROUND | (ROUND << 2)]);
    assert_eq!(found.half_width, 1.0);
    assert!(close(found.ink, [2.0, 11.0, 22.0, 13.0]));
}

#[test]
fn a_round_join_polyline_has_round_inner_ends_and_styled_outer_ends() {
    let path = BezPath::from_svg("M0 0 L10 0 L10 10 L20 20").expect("a path");
    let style = kurbo::Stroke::new(2.0)
        .with_join(kurbo::Join::Round)
        .with_start_cap(kurbo::Cap::Butt)
        .with_end_cap(kurbo::Cap::Square);
    let found = stroked(&path, &style).expect("three capsules");
    assert_eq!(found.capsules.len(), 3);
    assert_eq!(
        found.caps,
        [
            BUTT | (ROUND << 2),
            ROUND | (ROUND << 2),
            ROUND | (SQUARE << 2)
        ]
    );
}

#[test]
fn collinear_segments_merge_into_one_capsule() {
    let path = BezPath::from_svg("M0 0 L5 0 L12 0").expect("a path");
    let found = stroked(&path, &kurbo::Stroke::new(2.0)).expect("a capsule");
    assert_eq!(found.capsules, [[0.0, 0.0, 12.0, 0.0]]);
}

#[test]
fn a_zero_length_round_capped_segment_is_a_dot() {
    let path = BezPath::from_svg("M5 5 L5 5").expect("a path");
    let found = stroked(&path, &kurbo::Stroke::new(2.0)).expect("a dot");
    assert_eq!(found.capsules, [[5.0, 5.0, 5.0, 5.0]]);
    assert!(close(found.ink, [4.0, 4.0, 6.0, 6.0]));

    let butt = kurbo::Stroke::new(2.0).with_caps(kurbo::Cap::Butt);
    assert_eq!(stroked(&path, &butt).map(|found| found.count), Some(0));
    let square = kurbo::Stroke::new(2.0).with_caps(kurbo::Cap::Square);
    assert_eq!(stroked(&path, &square), None);
}

#[test]
fn an_axis_aligned_miter_polyline_squares_its_inner_ends() {
    let path = BezPath::from_svg("M0 0 L10 0 L10 10").expect("a path");
    let style = kurbo::Stroke::new(2.0)
        .with_join(kurbo::Join::Miter)
        .with_caps(kurbo::Cap::Butt);
    let found = stroked(&path, &style).expect("two capsules");
    assert_eq!(found.caps, [BUTT | (SQUARE << 2), SQUARE | (BUTT << 2)]);
}

#[test]
fn a_bevel_polyline_and_a_sixty_degree_miter_are_declined() {
    let right_angle = BezPath::from_svg("M0 0 L10 0 L10 10").expect("a path");
    let bevel = kurbo::Stroke::new(2.0).with_join(kurbo::Join::Bevel);
    assert_eq!(stroked(&right_angle, &bevel), None);

    let sixty = BezPath::from_svg("M0 0 L10 0 L5 8.660254").expect("a path");
    let miter = kurbo::Stroke::new(2.0).with_join(kurbo::Join::Miter);
    assert_eq!(stroked(&sixty, &miter), None);
}

#[test]
fn a_circle_stroke_is_an_annulus() {
    let path = Circle::new((20.0, 20.0), 10.0).to_path(0.1);
    let found = stroked(&path, &kurbo::Stroke::new(2.0)).expect("an annulus");
    assert_eq!(found.discs.len(), 1);
    assert!(close(found.discs[0], [20.0, 20.0, 11.0, 9.0]));

    // A ring whose inner edge would pass the centre is no annulus.
    assert_eq!(stroked(&path, &kurbo::Stroke::new(20.0)), None);
}

#[test]
fn a_rect_stroke_takes_its_corners_from_the_join() {
    let path = Rect::new(0.0, 0.0, 20.0, 10.0).to_path(0.1);
    let cases = [
        (kurbo::Join::Miter, 4.0, 0.0, 2.0),
        (kurbo::Join::Round, 4.0, 1.0, 2.0),
        (kurbo::Join::Bevel, 4.0, 1.0, 1.0),
        (kurbo::Join::Miter, 1.2, 1.0, 1.0),
    ];
    for (join, limit, radius, exponent) in cases {
        let style = kurbo::Stroke::new(2.0)
            .with_join(join)
            .with_miter_limit(limit);
        let found = stroked(&path, &style).unwrap_or_else(|| panic!("{join:?} at {limit}"));
        assert_eq!(found.boxes.len(), 1);
        let prim = found.boxes[0];
        assert!(close(prim.rect, [-1.0, -1.0, 21.0, 11.0]));
        assert_eq!(prim.radii, [radius; 8], "{join:?} at {limit}");
        assert_eq!(prim.exponent, exponent, "{join:?} at {limit}");
        assert_eq!(prim.border, 2.0);
    }
}

#[test]
fn a_rounded_rect_stroke_has_outer_radius_r_plus_half_width() {
    let path = RoundedRect::new(0.0, 0.0, 30.0, 20.0, 4.0).to_path(0.1);
    let found = stroked(&path, &kurbo::Stroke::new(2.0)).expect("a bordered box");
    let prim = found.boxes[0];
    assert!(close(prim.rect, [-1.0, -1.0, 31.0, 21.0]));
    assert!(close(prim.radii, [5.0; 8]));
    assert_eq!(prim.exponent, 2.0);

    // A bevel joins only the sharp corners, and one exponent cuts every corner.
    let bevel = kurbo::Stroke::new(2.0).with_join(kurbo::Join::Bevel);
    let mixed = RoundedRect::new(0.0, 0.0, 30.0, 20.0, (4.0, 0.0, 4.0, 4.0)).to_path(0.1);
    assert_eq!(stroked(&mixed, &bevel), None);
}

#[test]
fn a_stroked_elliptical_corner_is_declined() {
    // The top left corner runs four across and two down.
    let k = 0.552_284_749_8;
    let mut path = BezPath::new();
    path.move_to((0.0, 2.0));
    path.curve_to((0.0, 2.0 - 2.0 * k), (4.0 - 4.0 * k, 0.0), (4.0, 0.0));
    path.line_to((20.0, 0.0));
    path.line_to((20.0, 10.0));
    path.line_to((0.0, 10.0));
    path.close_path();
    let found = filled(&path).expect("filled, an elliptical corner is a box");
    assert!(close(
        found.boxes[0].radii,
        [4.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    ));
    assert_eq!(stroked(&path, &kurbo::Stroke::new(1.0)), None);
}

#[test]
fn a_dashed_stroke_is_declined() {
    let path = Rect::new(0.0, 0.0, 20.0, 10.0).to_path(0.1);
    let dashed = kurbo::Stroke::new(2.0).with_dashes(0.0, [4.0, 2.0]);
    assert_eq!(stroked(&path, &dashed), None);
    assert!(stroked(&path, &kurbo::Stroke::new(2.0)).is_some());
}

#[test]
fn more_subpaths_than_the_limit_are_declined() {
    let mut path = BezPath::new();
    for at in 0..257 {
        let x = f64::from(at) * 4.0;
        path.extend(Rect::new(x, 0.0, x + 2.0, 2.0).path_elements(0.1));
    }
    assert_eq!(filled(&path), None);
    let wider = Limits {
        tau: TAU,
        max_prims: 257,
    };
    let found = recognise(&path, Part::Fill(peniko::Fill::NonZero), wider).expect("257 boxes");
    assert_eq!(found.count, 257);
}

#[test]
fn prims_two_pixels_apart_are_separated() {
    let rects = [
        [0.0, 0.0, 10.0, 10.0],
        [12.0, 0.0, 22.0, 10.0],
        [0.0, 12.0, 10.0, 22.0],
    ];
    assert!(separated(&rects, 1.0));
    assert!(!separated(
        &[[0.0, 0.0, 10.0, 10.0], [11.5, 0.0, 20.0, 10.0]],
        1.0
    ));
}

#[test]
fn abutting_fractional_rects_are_not_separated() {
    assert!(!separated(
        &[[0.0, 0.0, 10.5, 10.0], [10.5, 0.0, 20.0, 10.0]],
        1.0
    ));
    assert!(separated(
        &[[0.0, 0.0, 10.5, 10.0], [10.5, 0.0, 20.0, 10.0]],
        0.0
    ));
}

#[test]
fn a_crowded_cell_is_not_proven_separated() {
    // One large rectangle far away sets the cell size, so the seventeen small ones share a cell.
    let mut rects = vec![[1000.0, 1000.0, 1200.0, 1200.0]];
    rects.extend((0..17).map(|at| {
        let x = at as f32 * 4.0;
        [x, 0.0, x + 1.0, 1.0]
    }));
    assert!(!separated(&rects, 1.0));
    rects.pop();
    assert!(separated(&rects, 1.0), "sixteen fit one cell");
}
