//! What the mask cache keeps, declines and gives back as shapes change between frames.

use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, TextureKind};
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
        self.cache.end_frame(&mut self.atlas);
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

#[test]
fn a_churning_owner_keeps_at_most_two_live_tiles() {
    let mut rig = Rig::new(false);
    for frame in 0..20 {
        assert!(rig.frame(OWNER, &triangle(f64::from(frame) * 0.01)).is_some());
        assert!(rig.atlas.len() <= 2, "frame {frame}: {} tiles", rig.atlas.len());
        assert!(rig.cache.entries.len() <= 2, "frame {frame}");
    }
}

#[test]
fn reclaim_spares_a_tile_a_record_holds() {
    let mut rig = Rig::new(false);
    let first = rig.frame(OWNER, &triangle(0.0)).expect("a mask").key;
    // A recorded painting that draws the first tile holds it.
    rig.atlas.retain(first);
    for frame in 1..6 {
        rig.frame(OWNER, &triangle(f64::from(frame) * 0.1));
    }
    assert!(rig.atlas.contains(first), "a held tile stays");
    assert!(
        rig.cache.entries.values().any(|key| *key == first),
        "and so does its entry"
    );
}

#[test]
fn reclaim_never_touches_a_glyph_tile() {
    let mut rig = Rig::new(false);
    let glyph = AtlasKey::new(42, TextureKind::Mono);
    rig.begin();
    rig.atlas
        .get_or_insert(glyph, Size::new(8, 8), || vec![0; 64])
        .expect("room for a glyph");
    rig.end();
    for frame in 0..20 {
        rig.frame(OWNER, &triangle(f64::from(frame) * 0.01));
    }
    assert!(rig.atlas.contains(glyph));
}

#[test]
fn a_one_off_change_leaves_the_old_tile_for_eviction() {
    let mut rig = Rig::new(false);
    let first = triangle(0.0);
    let old = rig.frame(OWNER, &first).expect("a mask").key;
    let second = triangle(0.5);
    for _ in 0..3 {
        rig.frame(OWNER, &second);
    }
    assert!(rig.atlas.contains(old), "one change removes nothing");
    let tiles = rig.atlas.len();
    let back = rig.frame(OWNER, &first).expect("a mask").key;
    assert_eq!(back, old, "swapping back hits the old tile");
    assert_eq!(rig.atlas.len(), tiles);
}

#[test]
fn a_declining_volatile_owner_gives_back_its_tiles() {
    let mut rig = Rig::new(true);
    let mut drawn = Vec::new();
    for frame in 0..3 {
        drawn.push(rig.frame(OWNER, &triangle(f64::from(frame) * 0.1)).expect("a mask").key);
    }
    assert!(rig.frame(OWNER, &triangle(0.3)).is_none(), "volatile and declined");
    for key in drawn {
        assert!(!rig.atlas.contains(key), "{key:?} was given back");
    }
    assert!(rig.cache.entries.is_empty());
}

#[test]
fn reclaim_spares_a_tile_another_owner_drew_this_frame() {
    let mut rig = Rig::new(false);
    let shared = triangle(0.0);
    let first = rig.frame(OWNER, &shared).expect("a mask").key;
    rig.frame(OWNER, &triangle(0.1));
    rig.begin();
    // The owner drops the first tile in this frame, and another owner draws it with no hold.
    rig.ask(OWNER, &triangle(0.2));
    assert_eq!(rig.ask(VectorId(8), &shared).expect("a mask").key, first);
    rig.end();
    assert!(rig.atlas.contains(first));
}

/// Asks for `count` new geometries in the current frame, one owner each from `first`, and reports
/// which were masks.
fn new_masks(rig: &mut Rig, first: u32, count: u32) -> Vec<bool> {
    (first..first + count)
        .map(|index| {
            rig.ask(VectorId(100 + index), &triangle(f64::from(index) * 0.01))
                .is_some()
        })
        .collect()
}

#[test]
fn the_new_mask_budget_declines_the_33rd_miss_while_ready() {
    let mut rig = Rig::new(true);
    rig.begin();
    let masks = new_masks(&mut rig, 0, 33);
    rig.end();
    assert!(masks[..32].iter().all(|mask| *mask));
    assert!(!masks[32]);
    // The budget is per frame.
    rig.begin();
    assert_eq!(new_masks(&mut rig, 32, 1), [true]);
    rig.end();
}

#[test]
fn a_cache_hit_spends_no_budget() {
    let mut rig = Rig::new(true);
    rig.begin();
    assert!(new_masks(&mut rig, 0, 32).into_iter().all(|mask| mask));
    rig.end();
    rig.begin();
    // The same 32 again are hits, and leave the whole budget for 32 new ones.
    assert!(new_masks(&mut rig, 0, 64).into_iter().all(|mask| mask));
    assert_eq!(new_masks(&mut rig, 64, 1), [false]);
    rig.end();
}

#[test]
fn the_budget_does_not_apply_while_vector_raster_is_cold() {
    let mut rig = Rig::new(false);
    rig.begin();
    assert!(new_masks(&mut rig, 0, 64).into_iter().all(|mask| mask));
    rig.end();
}
