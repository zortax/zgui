use zgui_scene::kurbo::{BezPath, PathEl};

use super::{MAX_GEOMETRIES, SplitDeclined, geometry_of, phase_of, split};

/// The identity map.
const IDENTITY: [f64; 4] = [1.0, 0.0, 0.0, 1.0];

/// Adds an upright triangle of circumradius `r` about `(x, y)`, starting at its top vertex.
fn triangle(path: &mut BezPath, x: f64, y: f64, r: f64) {
    let half = r * 3f64.sqrt() / 2.0;
    path.move_to((x, y - r));
    path.line_to((x + half, y + r / 2.0));
    path.line_to((x - half, y + r / 2.0));
    path.close_path();
}

/// `count` triangles of circumradius `size(index)` on a grid 20 units apart.
fn triangles(count: usize, size: impl Fn(usize) -> f64) -> BezPath {
    let mut path = BezPath::new();
    for index in 0..count {
        let (column, row) = ((index % 50) as f64, (index / 50) as f64);
        triangle(
            &mut path,
            10.0 + 20.0 * column,
            10.0 + 20.0 * row,
            size(index),
        );
    }
    path
}

#[test]
fn translated_triangles_are_one_outline_at_many_anchors() {
    let path = triangles(1000, |_| 4.0);
    let found = split(&path, IDENTITY).expect("one repeated outline");
    assert_eq!(found.geometries.len(), 1);
    assert_eq!(found.anchors.len(), 1000);
    assert!(found.of.iter().all(|&index| index == 0));
    assert_eq!(
        found.anchors[1],
        [30.0, 6.0],
        "the anchor is the move point"
    );
    let geometry = &found.geometries[0];
    assert_eq!(geometry.commands[..3], [super::MOVE, 0, 0]);
    let half = 2.0 * 3f32.sqrt();
    assert!((geometry.bounds[0] + half).abs() < 1.0 / 256.0);
    assert_eq!(geometry.bounds[1], 0.0);
    assert!((geometry.bounds[2] - half).abs() < 1.0 / 256.0);
    assert_eq!(geometry.bounds[3], 6.0);
    assert!(geometry.area > 0.0, "clockwise on the screen is positive");
}

#[test]
fn the_device_map_scales_and_mirrors_the_outline() {
    let path = triangles(8, |_| 4.0);
    let doubled = split(&path, [2.0, 0.0, 0.0, 2.0]).expect("an outline");
    assert_eq!(doubled.geometries[0].bounds[3], 12.0);
    let mirrored = split(&path, [-1.0, 0.0, 0.0, 1.0]).expect("an outline");
    let plain = split(&path, IDENTITY).expect("an outline");
    assert_eq!(
        mirrored.geometries[0].area, -plain.geometries[0].area,
        "a mirror turns the outline the other way"
    );
    assert_eq!(
        mirrored.anchors, plain.anchors,
        "anchors stay in path units"
    );
}

#[test]
fn seven_subpaths_decline_at_once() {
    let mut path = triangles(7, |_| 4.0);
    assert_eq!(split(&path, IDENTITY), Err(SplitDeclined::Few));
    // Nothing past the count is read: a non-finite point is no other answer.
    path.line_to((f64::NAN, 0.0));
    assert_eq!(split(&path, IDENTITY), Err(SplitDeclined::Few));
}

#[test]
fn seventeen_distinct_subpaths_decline_within_the_first_sixty_four() {
    let path = triangles(1000, |index| 2.0 + (index % 17) as f64 * 0.25);
    match split(&path, IDENTITY) {
        Err(SplitDeclined::Distinct { read }) => {
            assert!(read <= 64, "read {read} subpaths");
            assert_eq!(read, 17);
        }
        other => panic!("expected a decline, found {other:?}"),
    }
    let sixteen = triangles(1000, |index| 2.0 + (index % 16) as f64 * 0.25);
    let found = split(&sixteen, IDENTITY).expect("sixteen outlines");
    assert_eq!(found.geometries.len(), 16);
}

#[test]
fn distinct_outlines_past_an_eighth_of_the_subpaths_decline() {
    // 200 subpaths allow 25 outlines. The first 64 hold 16 of them, and a new one starts every
    // eighth subpath after that.
    let size = |index: usize| {
        let outline = if index < 64 {
            index % 16
        } else {
            16 + (index - 64) / 8
        };
        2.0 + outline as f64 * 0.125
    };
    match split(&triangles(200, size), IDENTITY) {
        Err(SplitDeclined::Distinct { read }) => {
            assert_eq!(read, 64 + 8 * 9 + 1, "the 26th outline declines");
        }
        other => panic!("expected a decline, found {other:?}"),
    }
    let capped = |index: usize| size(index.min(64 + 8 * 9 - 1));
    let found = split(&triangles(200, capped), IDENTITY).expect("25 outlines");
    assert_eq!(found.geometries.len(), 25);
    assert!(found.geometries.len() <= MAX_GEOMETRIES);
}

#[test]
fn float_noise_below_a_256th_keeps_one_outline() {
    let mut path = BezPath::new();
    for index in 0..64 {
        let x = 10_000.0 + index as f64 * 7.123_456_7;
        let y = -10_000.0 + index as f64 * 3.987_654_3;
        triangle(&mut path, x, y, 4.0);
    }
    let found = split(&path, IDENTITY).expect("one outline");
    assert_eq!(found.geometries.len(), 1);
}

#[test]
fn a_point_far_from_its_anchor_or_not_finite_declines() {
    let mut path = triangles(8, |_| 4.0);
    path.move_to((0.0, 0.0));
    path.line_to((65.0, 0.0));
    assert_eq!(split(&path, IDENTITY), Err(SplitDeclined::Large));
    let mut path = triangles(8, |_| 4.0);
    path.move_to((0.0, 0.0));
    path.line_to((f64::INFINITY, 0.0));
    assert_eq!(split(&path, IDENTITY), Err(SplitDeclined::Shape));
}

#[test]
fn a_marker_is_one_outline_about_its_origin() {
    let mut cross = BezPath::new();
    cross.move_to((-3.0, 0.0));
    cross.line_to((3.0, 0.0));
    cross.move_to((0.0, -3.0));
    cross.line_to((0.0, 3.0));
    let geometry = geometry_of(&cross, [2.0, 0.0, 0.0, 2.0]).expect("a marker");
    assert_eq!(geometry.bounds, [-6.0, -6.0, 6.0, 6.0]);
    assert_eq!(geometry.commands[..3], [super::MOVE, -6 * 256, 0]);
    let elements = cross.elements().len();
    assert_eq!(elements, 4);
    assert!(matches!(cross.elements()[0], PathEl::MoveTo(_)));
}

#[test]
fn the_phase_split_errs_at_most_an_eighth_of_a_pixel() {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64
    };
    for _ in 0..100_000 {
        let device = ((next() - 0.5) * 4096.0) as f32;
        let (pixel, phase) = phase_of(device);
        assert!(phase < 4);
        let quantised = pixel as f32 + f32::from(phase) / 4.0;
        assert!(
            (quantised - device).abs() <= 0.125 + 1.0e-4,
            "{device} splits into {pixel} and {phase}"
        );
        // A tie rounds up here and away from zero for a pen; every other position agrees.
        let ties = (4.0 * device).fract().abs() == 0.5;
        if !ties {
            let pen = zgui_text::PenPosition::of(device);
            assert_eq!(pen.pen(), pixel as f32, "{device}");
            assert_eq!(pen.offset().0, phase, "{device}");
        }
    }
    assert_eq!(phase_of(-0.3), (-1, 3));
    assert_eq!(phase_of(2.9), (3, 0));
}
