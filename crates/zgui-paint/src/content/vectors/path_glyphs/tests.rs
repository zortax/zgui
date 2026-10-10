//! What the sheets hold, what a request rasterises, and how long a split is kept.

use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasLimits, MemorySink, TextureId, TextureKind};
use zgui_scene::kurbo::{self, BezPath};
use zgui_scene::peniko;

use super::{GlyphRequest, GlyphSheets, MAX_SPLITS, PHASES, PathGlyphs, Splits};
use crate::content::vectors::mask::{MASK_NAMESPACE, VectorMaskStyle};
use crate::emit::vector::split::{Geometry, geometry_of, split};

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

/// Path glyphs, an atlas, and the next handle of the mask namespace.
struct Rig {
    glyphs: PathGlyphs,
    atlas: Atlas,
    next: u64,
}

impl Rig {
    fn new() -> Self {
        let mut rig = Self {
            glyphs: PathGlyphs::default(),
            atlas: Atlas::new(AtlasLimits::default()),
            next: MASK_NAMESPACE,
        };
        rig.frame();
        rig
    }

    /// Starts the next frame.
    fn frame(&mut self) {
        self.glyphs.end_frame();
        self.atlas.begin_frame();
        self.glyphs.begin_frame();
    }

    /// The sheets of `geometries`, filled.
    fn fill(&mut self, geometries: &[Geometry]) -> Option<GlyphSheets> {
        self.glyphs.sheets_for(
            &mut self.atlas,
            &mut self.next,
            GlyphRequest {
                geometries,
                style: VectorMaskStyle::Fill(peniko::Fill::NonZero),
                scale: 1.0,
            },
        )
    }
}

#[test]
fn a_thousand_translated_triangles_take_sixteen_tiles() {
    let path = triangles((0..1000).map(scattered));
    let found = split(&path, IDENTITY).expect("one outline");
    assert_eq!(found.geometries.len(), 1);
    let mut rig = Rig::new();
    let sheets = rig.fill(&found.geometries).expect("sheets");
    assert_eq!(rig.glyphs.len(), 1, "one sheet");
    assert_eq!(rig.atlas.len(), 1, "one atlas tile");
    assert_eq!(sheets.table.len(), PHASES, "sixteen cells");
    assert_eq!(sheets.keys.len(), 1);
}

#[test]
fn a_changed_membership_rasterises_no_tile() {
    let mut rig = Rig::new();
    let first = split(&triangles((0..1000).map(scattered)), IDENTITY).expect("one outline");
    let before = rig.fill(&first.geometries).expect("sheets");
    rig.frame();
    // 900 of the anchors and 50 new ones.
    let changed = triangles((100..1000).chain(5000..5050).map(scattered));
    let second = split(&changed, IDENTITY).expect("one outline");
    let after = rig.fill(&second.geometries).expect("sheets");
    assert_eq!(after, before, "the same cells in the same sheet");
    assert_eq!(rig.atlas.len(), 1, "no tile was rasterised");
    assert_eq!(rig.glyphs.spent.tiles, 0, "the frame spent nothing");
}

/// The coverage of every cell of the first sheet in `sheets`, read back from the atlas.
fn cells(rig: &mut Rig, sheets: &GlyphSheets) -> Vec<(Vec<u8>, [i32; 2], [i32; 2])> {
    let mut sink = MemorySink::new();
    rig.atlas.flush_uploads(&mut sink).expect("a memory sink");
    let texture = TextureId::new(TextureKind::Mono, sheets.texture & 0xffff);
    sheets
        .table
        .iter()
        .map(|&[at, size, ox, oy]| {
            let (x, y) = ((at & 0xffff) as i32, (at >> 16) as i32);
            let (w, h) = ((size & 0xffff) as i32, (size >> 16) as i32);
            let mut coverage = Vec::new();
            for row in 0..h {
                for column in 0..w {
                    coverage.push(sink.texel(texture, x + column, y + row).expect("a texel")[0]);
                }
            }
            (coverage, [w, h], [ox as i32, oy as i32])
        })
        .collect()
}

/// The centroid of a cell's coverage, from the pixel of the anchor.
fn centroid((coverage, size, offset): &(Vec<u8>, [i32; 2], [i32; 2])) -> [f64; 2] {
    let mut sum = [0.0f64; 3];
    for (index, &value) in coverage.iter().enumerate() {
        let x = (index as i32 % size[0]) as f64 + 0.5 + f64::from(offset[0]);
        let y = (index as i32 / size[0]) as f64 + 0.5 + f64::from(offset[1]);
        let value = f64::from(value);
        sum = [sum[0] + value * x, sum[1] + value * y, sum[2] + value];
    }
    [sum[0] / sum[2], sum[1] / sum[2]]
}

#[test]
fn each_cell_is_its_outline_moved_by_its_phase() {
    let mut star = BezPath::new();
    for index in 0..10 {
        let angle = core::f64::consts::PI * index as f64 / 5.0;
        let radius = if index % 2 == 0 { 24.0 } else { 10.0 };
        let point = (radius * angle.sin(), -radius * angle.cos());
        if index == 0 {
            star.move_to(point);
        } else {
            star.line_to(point);
        }
    }
    star.close_path();
    let geometry = geometry_of(&star, IDENTITY).expect("a marker");
    let mut rig = Rig::new();
    let sheets = rig.fill(std::slice::from_ref(&geometry)).expect("sheets");
    let cells = cells(&mut rig, &sheets);
    let still = centroid(&cells[0]);
    for (phase, cell) in cells.iter().enumerate() {
        let moved = centroid(cell);
        let expected = [(phase % 4) as f64 / 4.0, (phase / 4) as f64 / 4.0];
        for axis in 0..2 {
            let error = (moved[axis] - still[axis] - expected[axis]).abs();
            assert!(
                error <= 1.0 / 64.0,
                "phase {phase} moves axis {axis} by {} for {}",
                moved[axis] - still[axis],
                expected[axis]
            );
        }
    }
}

#[test]
fn a_cell_past_sixty_four_pixels_declines() {
    let mut wide = BezPath::new();
    wide.move_to((-32.0, -2.0));
    wide.line_to((31.5, -2.0));
    wide.line_to((31.5, 2.0));
    wide.close_path();
    let geometry = geometry_of(&wide, IDENTITY).expect("a marker");
    let mut rig = Rig::new();
    assert!(
        rig.fill(std::slice::from_ref(&geometry)).is_none(),
        "the phase of three quarters makes the cell 65 pixels wide"
    );
    assert_eq!(rig.atlas.len(), 0);
    let narrower = geometry_of(&wide, [0.98, 0.0, 0.0, 1.0]).expect("a marker");
    assert!(rig.fill(std::slice::from_ref(&narrower)).is_some());
}

#[test]
fn a_stroke_is_drawn_at_its_device_width() {
    let mut cross = BezPath::new();
    cross.move_to((-3.0, 0.0));
    cross.line_to((3.0, 0.0));
    let geometry = geometry_of(&cross, [2.0, 0.0, 0.0, 2.0]).expect("a marker");
    let stroke = kurbo::Stroke::new(1.5).with_caps(kurbo::Cap::Round);
    let mut rig = Rig::new();
    let sheets = rig
        .glyphs
        .sheets_for(
            &mut rig.atlas,
            &mut rig.next,
            GlyphRequest {
                geometries: std::slice::from_ref(&geometry),
                style: VectorMaskStyle::Stroke(&stroke),
                scale: 2.0,
            },
        )
        .expect("sheets");
    // Three device pixels wide, so the reach is a pixel and a half past the line.
    assert_eq!(sheets.reach[0], [-8, -2, 9, 3]);
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
    assert_eq!(splits.len(), MAX_SPLITS);
    assert!(
        splits.lookup(first, IDENTITY).is_none(),
        "the oldest entry made room"
    );
}

#[test]
fn forgotten_sheets_leave_the_cache() {
    let found = split(&triangles((0..64).map(scattered)), IDENTITY).expect("one outline");
    let mut rig = Rig::new();
    let sheets = rig.fill(&found.geometries).expect("sheets");
    assert_eq!(rig.glyphs.len(), 1);
    assert!(rig.atlas.remove(sheets.keys[0]));
    rig.glyphs.forget_tiles(&sheets.keys);
    assert_eq!(rig.glyphs.len(), 0);
    let again = rig.fill(&found.geometries).expect("sheets");
    assert_ne!(again.keys, sheets.keys, "a new sheet takes a new key");
    assert_eq!(rig.glyphs.len(), 1);
}

#[test]
fn the_frame_budget_declines_the_sixty_fifth_new_sheet() {
    let mut rig = Rig::new();
    let outlines: Vec<Geometry> = (0..65)
        .map(|index| {
            let mut path = BezPath::new();
            triangle(&mut path, 0.0, 0.0, 2.0 + index as f64 * 0.125);
            geometry_of(&path, IDENTITY).expect("a marker")
        })
        .collect();
    for geometry in &outlines[..64] {
        assert!(rig.fill(std::slice::from_ref(geometry)).is_some());
    }
    assert!(
        rig.fill(std::slice::from_ref(&outlines[64])).is_none(),
        "the sixty-fifth new sheet waits for the next frame"
    );
    assert!(
        rig.fill(std::slice::from_ref(&outlines[3])).is_some(),
        "a held sheet costs nothing"
    );
    rig.frame();
    assert!(rig.fill(std::slice::from_ref(&outlines[64])).is_some());
}
