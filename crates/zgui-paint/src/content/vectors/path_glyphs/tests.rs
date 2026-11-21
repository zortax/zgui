//! What a part's outlines are, and how long a split is kept.

use std::sync::Arc;

use kurbo::{self, BezPath, Shape as _};

use super::{MAX_SPLITS, PartStyle, Splits, outlines};
use crate::emit::vector::split::{geometry_of, split};

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

/// Triangles of circumradius 4 at `points`.
fn triangles(points: impl IntoIterator<Item = (f64, f64)>) -> BezPath {
    let mut path = BezPath::new();
    for (x, y) in points {
        triangle(&mut path, x, y, 4.0);
    }
    path
}

/// A pseudo-random point in a 960 by 540 box, for each index.
fn scattered(index: usize) -> (f64, f64) {
    let hash = (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let x = (hash >> 11) as f64 / (1u64 << 53) as f64 * 960.0;
    let y = ((hash.rotate_left(29)) >> 11) as f64 / (1u64 << 53) as f64 * 540.0;
    (x, y)
}

#[test]
fn an_outline_is_its_geometry_in_device_pixels_from_its_origin() {
    let mut path = BezPath::new();
    triangle(&mut path, 0.0, 0.0, 4.0);
    let geometry = geometry_of(&path, [2.0, 0.0, 0.0, 2.0]).expect("a marker");
    let outlines = outlines(std::slice::from_ref(&geometry), PartStyle::Fill).expect("outlines");
    let bounds = outlines.shapes[0].bounding_box();
    let half = 4.0 * 3f64.sqrt();
    // The marker is written about its origin, and the device map doubles every length.
    for (found, wanted) in [bounds.x0, bounds.y0, bounds.x1, bounds.y1]
        .into_iter()
        .zip([-half, -8.0, half, 4.0])
    {
        assert!((found - wanted).abs() <= 1.0 / 128.0, "{bounds:?}");
    }
    assert_eq!(
        outlines.reach[0],
        [bounds.x0, bounds.y0, bounds.x1, bounds.y1]
    );
}

#[test]
fn an_outline_past_sixty_four_pixels_declines() {
    let mut wide = BezPath::new();
    wide.move_to((-32.0, -2.0));
    wide.line_to((32.5, -2.0));
    wide.line_to((32.5, 2.0));
    wide.close_path();
    let geometry = geometry_of(&wide, IDENTITY).expect("a marker");
    assert!(outlines(std::slice::from_ref(&geometry), PartStyle::Fill).is_none());
    let narrower = geometry_of(&wide, [0.98, 0.0, 0.0, 1.0]).expect("a marker");
    assert!(outlines(std::slice::from_ref(&narrower), PartStyle::Fill).is_some());
}

#[test]
fn a_stroke_is_outlined_at_its_device_width() {
    let mut cross = BezPath::new();
    cross.move_to((-3.0, 0.0));
    cross.line_to((3.0, 0.0));
    let geometry = geometry_of(&cross, [2.0, 0.0, 0.0, 2.0]).expect("a marker");
    let stroke = kurbo::Stroke::new(1.5).with_caps(kurbo::Cap::Butt);
    let style = PartStyle::Stroke {
        style: &stroke,
        scale: 2.0,
    };
    let outlines = outlines(std::slice::from_ref(&geometry), style).expect("outlines");
    // Three device pixels wide, so the outline reaches a pixel and a half past the line.
    let [x0, y0, x1, y1] = outlines.reach[0];
    assert!((x0 - -6.0).abs() < 1.0e-3 && (x1 - 6.0).abs() < 1.0e-3);
    assert!((y0 - -1.5).abs() < 1.0e-3 && (y1 - 1.5).abs() < 1.0e-3);
}

#[test]
fn a_split_is_reused_by_path_identity_and_dies_with_it() {
    let mut splits = Splits::default();
    splits.begin_frame();
    let path = Arc::new(triangles((0..8).map(scattered)));
    assert!(splits.lookup(&path, IDENTITY).is_none());
    let found = Arc::new(split(&path, IDENTITY).expect("one outline"));
    splits
        .insert(&path, IDENTITY, Ok(Arc::clone(&found)))
        .expect("kept");
    let held = splits.lookup(&path, IDENTITY).expect("held");
    assert!(Arc::ptr_eq(held.outcome.as_ref().expect("a split"), &found));
    assert!(
        splits.lookup(&path, [2.0, 0.0, 0.0, 2.0]).is_none(),
        "the device map is part of the key"
    );
    let copy = Arc::new((*path).clone());
    assert!(
        splits.lookup(&copy, IDENTITY).is_none(),
        "an equal path in another allocation misses"
    );
    splits.end_frame();
    splits.begin_frame();
    assert_eq!(splits.len(), 1);
    drop(path);
    splits.end_frame();
    assert_eq!(splits.len(), 0, "the entry dies with its path");
}

#[test]
fn a_full_split_map_drops_the_least_recently_touched() {
    let mut splits = Splits::default();
    splits.begin_frame();
    let paths: Vec<Arc<BezPath>> = (0..=MAX_SPLITS)
        .map(|_| Arc::new(triangles((0..8).map(scattered))))
        .collect();
    let first = &paths[0];
    splits.insert(first, IDENTITY, Err(super::SplitDeclined::Few));
    splits.end_frame();
    splits.begin_frame();
    for path in &paths[1..] {
        splits.insert(path, IDENTITY, Err(super::SplitDeclined::Few));
    }
    assert!(splits.len() <= MAX_SPLITS);
    assert!(
        splits.lookup(first, IDENTITY).is_none(),
        "the oldest entry made room"
    );
}
