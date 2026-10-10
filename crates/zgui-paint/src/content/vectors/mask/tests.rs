//! What the mask cache keeps, declines and gives back as shapes change between frames.

use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasLimits};
use zgui_geom::{Point, Rect, Size};
use zgui_scene::VectorId;
use zgui_scene::kurbo::BezPath;
use zgui_scene::peniko;

use super::{COLD_SEGMENTS, VectorMask, VectorMaskCache, VectorMaskRequest, VectorMaskStyle};

/// The owner most cases ask as.
const OWNER: VectorId = VectorId(7);

/// The raster bounds every request here uses.
const SIDE: i32 = 16;

/// A triangle inside the bounds, moved by a fraction of a pixel so each `shift` is new geometry.
fn triangle(shift: f64) -> Arc<BezPath> {
    let mut path = BezPath::new();
    path.move_to((1.0 + shift, 1.0));
    path.line_to((14.0, 2.0 + shift));
    path.line_to((3.0, 14.0));
    path.close_path();
    Arc::new(path)
}

/// A zig-zag inside the bounds with `segments` lines.
fn zigzag(segments: usize, shift: f64) -> Arc<BezPath> {
    let mut path = BezPath::new();
    path.move_to((1.0 + shift, 1.0));
    for index in 0..segments {
        let x = 1.0 + (index % 14) as f64;
        let y = if index % 2 == 0 { 14.0 } else { 1.0 + shift };
        path.line_to((x, y));
    }
    path.close_path();
    Arc::new(path)
}

/// A mask cache, an atlas, and every path a case has asked with, kept alive so that each new
/// path is a new allocation.
struct Rig {
    cache: VectorMaskCache,
    atlas: Atlas,
    paths: Vec<Arc<BezPath>>,
}

impl Rig {
    fn new(ready: bool) -> Self {
        let mut cache = VectorMaskCache::default();
        cache.set_raster_ready(ready);
        Self {
            cache,
            atlas: Atlas::new(AtlasLimits::default()),
            paths: Vec::new(),
        }
    }

    /// Starts a frame, as the content cache does.
    fn begin(&mut self) {
        self.atlas.begin_frame();
        self.cache.begin_frame();
    }

    /// Ends a frame, as the content cache does.
    fn end(&mut self) {
        self.cache.end_frame();
    }

    /// Asks for `path`'s fill as `owner`.
    fn ask(&mut self, owner: VectorId, path: &Arc<BezPath>) -> Option<VectorMask> {
        self.paths.push(Arc::clone(path));
        self.cache.tile_for(
            &mut self.atlas,
            VectorMaskRequest {
                owner,
                path,
                style: VectorMaskStyle::Fill(peniko::Fill::NonZero),
                density: [1.0, 1.0],
                scale: 1.0,
                bounds: Rect::new(Point::new(0, 0), Size::new(SIDE, SIDE)),
            },
        )
    }

    /// One frame in which `owner` asks for `path`.
    fn frame(&mut self, owner: VectorId, path: &Arc<BezPath>) -> Option<VectorMask> {
        self.begin();
        let mask = self.ask(owner, path);
        self.end();
        mask
    }

    /// Whether `owner` is volatile now.
    fn volatile(&self, owner: VectorId) -> bool {
        self.cache.histories[&owner].volatile
    }
}

#[test]
fn an_owner_that_changes_three_times_in_four_frames_is_volatile() {
    let mut rig = Rig::new(false);
    // The first request sets the stamp. Each later one with a new path is a change.
    for frame in 0..3 {
        rig.frame(OWNER, &triangle(f64::from(frame) * 0.1));
        assert!(!rig.volatile(OWNER), "{frame} changes are not volatile");
    }
    rig.frame(OWNER, &triangle(0.3));
    assert!(rig.volatile(OWNER));
}

#[test]
fn a_volatile_owner_recovers_after_four_stable_frames() {
    let mut rig = Rig::new(false);
    for frame in 0..4 {
        rig.frame(OWNER, &triangle(f64::from(frame) * 0.1));
    }
    assert!(rig.volatile(OWNER));
    let still = triangle(0.5);
    rig.frame(OWNER, &still);
    for frame in 0..3 {
        rig.frame(OWNER, &still);
        assert!(rig.volatile(OWNER), "stable for {frame} frames");
    }
    rig.frame(OWNER, &still);
    assert!(!rig.volatile(OWNER), "stable for four frames");
}

#[test]
fn an_unchanged_owner_across_frames_is_never_volatile() {
    let mut rig = Rig::new(true);
    let icon = triangle(0.0);
    for _ in 0..20 {
        assert!(rig.frame(OWNER, &icon).is_some());
        assert!(!rig.volatile(OWNER));
    }
}

#[test]
fn a_volatile_owner_declines_while_vector_raster_is_ready() {
    let mut rig = Rig::new(true);
    let masks: Vec<bool> = (0..6)
        .map(|frame| rig.frame(OWNER, &triangle(f64::from(frame) * 0.1)).is_some())
        .collect();
    assert_eq!(masks, [true, true, true, false, false, false]);
}

#[test]
fn a_small_volatile_owner_keeps_the_mask_while_vector_raster_is_cold() {
    let mut rig = Rig::new(false);
    for frame in 0..8 {
        assert!(rig.frame(OWNER, &triangle(f64::from(frame) * 0.1)).is_some());
    }
    assert!(rig.volatile(OWNER));
}

#[test]
fn a_volatile_owner_over_4096_segments_declines_while_cold() {
    let mut rig = Rig::new(false);
    let masks: Vec<bool> = (0..6)
        .map(|frame| {
            rig.frame(OWNER, &zigzag(COLD_SEGMENTS + 1, f64::from(frame) * 0.1))
                .is_some()
        })
        .collect();
    assert_eq!(masks, [true, true, true, false, false, false]);
}

#[test]
fn an_owner_untouched_for_eight_frames_is_swept() {
    let mut rig = Rig::new(false);
    rig.frame(OWNER, &triangle(0.0));
    for _ in 0..7 {
        rig.begin();
        rig.end();
    }
    assert!(rig.cache.histories.contains_key(&OWNER), "seven frames");
    rig.begin();
    rig.end();
    assert!(!rig.cache.histories.contains_key(&OWNER), "eight frames");
}
