//! What the mask cache keeps, declines and gives back as shapes change between frames.

use std::sync::Arc;

use zgui_atlas::{Atlas, AtlasKey, AtlasLimits, TextureKind};
use zgui_geom::{Point, Rect, Size};
use zgui_scene::VectorId;
use zgui_scene::kurbo::BezPath;
use zgui_scene::peniko;

use super::{
    COLD_BUDGET_TILES, COLD_SEGMENTS, VectorMask, VectorMaskCache, VectorMaskRequest,
    VectorMaskStyle,
};

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
        self.ask_placed(owner, path, zgui_scene::kurbo::Affine::IDENTITY)
    }

    /// Asks for `path`'s fill under `placement` as `owner`.
    fn ask_placed(
        &mut self,
        owner: VectorId,
        path: &Arc<BezPath>,
        placement: zgui_scene::kurbo::Affine,
    ) -> Option<VectorMask> {
        self.paths.push(Arc::clone(path));
        self.cache.tile_for(
            &mut self.atlas,
            VectorMaskRequest {
                owner,
                path,
                placement,
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

/// A source path through a placement is the same raster as the placed path, so the two share one
/// tile.
#[test]
fn a_mask_through_a_placement_matches_the_placed_path() {
    let mut rig = Rig::new(false);
    let fit =
        zgui_scene::kurbo::Affine::translate((3.0, 2.0)) * zgui_scene::kurbo::Affine::scale(1.5);
    let source = triangle(0.0);
    let shape = zgui_svg::Shape {
        path: Arc::clone(&source),
        fill: None,
        stroke: None,
        clips: Vec::new(),
    };
    let placed = zgui_svg::document::place::shape(&shape, fit).path;
    rig.begin();
    let through = rig
        .ask_placed(OWNER, &source, fit)
        .expect("a mask through the fit");
    let direct = rig
        .ask(VectorId(OWNER.0 + 1), &placed)
        .expect("a mask of the placed path");
    rig.end();
    assert_eq!(through.key, direct.key, "one raster, one atlas entry");
    assert_eq!(through.tile, direct.tile);
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
        .map(|frame| {
            rig.frame(OWNER, &triangle(f64::from(frame) * 0.1))
                .is_some()
        })
        .collect();
    assert_eq!(masks, [true, true, true, false, false, false]);
}

#[test]
fn a_small_volatile_owner_keeps_the_mask_while_vector_raster_is_cold() {
    let mut rig = Rig::new(false);
    for frame in 0..8 {
        assert!(
            rig.frame(OWNER, &triangle(f64::from(frame) * 0.1))
                .is_some()
        );
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
        assert!(
            rig.frame(OWNER, &triangle(f64::from(frame) * 0.01))
                .is_some()
        );
        assert!(
            rig.atlas.len() <= 2,
            "frame {frame}: {} tiles",
            rig.atlas.len()
        );
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
        drawn.push(
            rig.frame(OWNER, &triangle(f64::from(frame) * 0.1))
                .expect("a mask")
                .key,
        );
    }
    assert!(
        rig.frame(OWNER, &triangle(0.3)).is_none(),
        "volatile and declined"
    );
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

#[test]
fn the_cold_budget_declines_volatile_misses_past_its_limit() {
    let mut rig = Rig::new(false);
    let owners = COLD_BUDGET_TILES + 1;
    // Every owner asks with new geometry in each frame. The fourth frame makes them all volatile.
    let frame = |rig: &mut Rig, frame: u32| {
        rig.begin();
        let masks: Vec<bool> = (0..owners)
            .map(|owner| {
                let shift = f64::from(frame) * 0.1 + f64::from(owner) * 1e-4;
                rig.ask(VectorId(100 + owner), &triangle(shift)).is_some()
            })
            .collect();
        rig.end();
        masks
    };
    for index in 0..3 {
        assert!(frame(&mut rig, index).into_iter().all(|mask| mask));
    }
    let masks = frame(&mut rig, 3);
    assert!(masks[..COLD_BUDGET_TILES as usize].iter().all(|mask| *mask));
    assert!(!masks[COLD_BUDGET_TILES as usize]);
}

#[test]
fn a_shape_placed_again_on_whole_pixels_is_not_a_change() {
    let mut rig = Rig::new(true);
    // A new allocation every frame, as a scrolled drawing is placed again, on the same tile.
    let masks: Vec<_> = (0..8)
        .map(|_| rig.frame(OWNER, &triangle(0.0)).map(|mask| mask.key))
        .collect();
    assert!(masks.iter().all(|key| *key == masks[0] && key.is_some()));
    assert!(!rig.volatile(OWNER));
    assert_eq!(rig.cache.histories[&OWNER].changes, 0);
}

#[test]
fn an_analytic_decline_is_remembered_for_eight_frames() {
    let mut rig = Rig::new(false);
    let other = VectorId(8);
    rig.begin();
    assert!(rig.cache.analytic_allowed(OWNER), "nothing declined yet");
    rig.cache.note_analytic_declined(OWNER);
    assert!(!rig.cache.analytic_allowed(OWNER));
    assert!(
        rig.cache.analytic_allowed(other),
        "a decline names one owner"
    );
    rig.end();
    for frame in 1..8 {
        rig.begin();
        assert!(!rig.cache.analytic_allowed(OWNER), "frame {frame}");
        rig.end();
    }
    rig.begin();
    assert!(rig.cache.analytic_allowed(OWNER), "frame 8 tries again");
    rig.end();
}

#[test]
fn an_analytic_decline_survives_the_sweep_while_it_counts() {
    let mut rig = Rig::new(false);
    // The owner's mask history is seven frames old when the decline arrives.
    rig.frame(OWNER, &triangle(0.0));
    for _ in 0..6 {
        rig.begin();
        rig.end();
    }
    rig.begin();
    rig.cache.note_analytic_declined(OWNER);
    rig.end();
    for frame in 1..8 {
        rig.begin();
        rig.end();
        assert!(
            rig.cache.histories.contains_key(&OWNER),
            "the decline counts in frame {frame}, so the sweep keeps it"
        );
        assert!(!rig.cache.analytic_allowed(OWNER), "frame {frame}");
    }
    rig.begin();
    rig.end();
    assert!(
        !rig.cache.histories.contains_key(&OWNER),
        "swept once it stops counting"
    );
    assert!(rig.cache.analytic_allowed(OWNER));
}

/// A source that keeps recognitions and declines every mask.
#[derive(Default)]
struct Remembering(core::cell::RefCell<crate::content::vectors::Recognitions>);

impl crate::content::vectors::VectorMaskSource for Remembering {
    fn vector_mask(&self, _request: VectorMaskRequest<'_>) -> Option<VectorMask> {
        None
    }

    fn recognitions(
        &self,
    ) -> Option<core::cell::RefMut<'_, crate::content::vectors::Recognitions>> {
        Some(self.0.borrow_mut())
    }
}

/// A filled circle as a shape in its own space.
fn circle_shape() -> zgui_svg::Shape {
    use zgui_scene::kurbo::Shape as _;

    zgui_svg::Shape {
        path: Arc::new(zgui_scene::kurbo::Circle::new((20.0, 20.0), 8.0).to_path(0.01)),
        fill: Some(zgui_svg::Fill {
            paint: zgui_svg::Paint::Solid(zgui_svg::Ink::Inherited { alpha: 1.0 }),
            rule: peniko::Fill::NonZero,
        }),
        stroke: None,
        clips: Vec::new(),
    }
}

/// What the circle's fill is found to be, at `tau`.
fn recognised_at(
    source: &Remembering,
    shape: &zgui_svg::Shape,
    tau: f64,
) -> Option<Arc<crate::emit::vector::recognise::Decomposition>> {
    use crate::emit::vector::ShapeSource;
    use crate::emit::vector::recognised::{Outline, PartOf, recognised};

    recognised(
        &ShapeSource::placed_shape(shape),
        Outline::Path,
        PartOf::Fill(peniko::Fill::NonZero),
        tau,
        1 << 22,
        source,
    )
}

#[test]
fn a_recognition_is_reused_by_path_identity_and_swept_after_eight_frames() {
    let source = Remembering::default();
    let shape = circle_shape();
    source.0.borrow_mut().begin_frame();
    let first = recognised_at(&source, &shape, 1.0 / 16.0).expect("a circle");
    let again = recognised_at(&source, &shape, 1.0 / 16.0).expect("a circle");
    assert!(
        Arc::ptr_eq(&first, &again),
        "the second lookup is the first result"
    );
    // Another allocation of the same geometry is another path.
    let copy = zgui_svg::Shape {
        path: Arc::new(shape.path.as_ref().clone()),
        ..shape.clone()
    };
    let other = recognised_at(&source, &copy, 1.0 / 16.0).expect("a circle");
    assert!(!Arc::ptr_eq(&first, &other));
    let other_again = recognised_at(&source, &copy, 1.0 / 16.0).expect("a circle");
    assert!(Arc::ptr_eq(&other, &other_again));
    // A result something else holds is kept; these are let go.
    drop((first, again, other, other_again));
    source.0.borrow_mut().end_frame();
    for frame in 1..8 {
        source.0.borrow_mut().begin_frame();
        source.0.borrow_mut().end_frame();
        assert_eq!(source.0.borrow().len(), 2, "frame {frame} keeps both");
    }
    source.0.borrow_mut().begin_frame();
    source.0.borrow_mut().end_frame();
    assert_eq!(
        source.0.borrow().len(),
        0,
        "eight untouched frames sweep them"
    );
}

#[test]
fn a_finer_tau_class_misses_the_cache() {
    let source = Remembering::default();
    let shape = circle_shape();
    let coarse = recognised_at(&source, &shape, 1.0 / 16.0).expect("a circle");
    let same_class = recognised_at(&source, &shape, 1.0 / 12.0).expect("a circle");
    assert!(
        Arc::ptr_eq(&coarse, &same_class),
        "one class serves every tolerance within a factor of two"
    );
    let finer = recognised_at(&source, &shape, 1.0 / 64.0).expect("a circle");
    assert!(
        !Arc::ptr_eq(&coarse, &finer),
        "a finer class is measured again"
    );
}

#[test]
fn a_marks_decline_is_remembered_for_eight_frames() {
    let mut rig = Rig::new(false);
    rig.begin();
    assert!(rig.cache.marks_allowed(OWNER), "nothing declined yet");
    rig.cache.note_marks_declined(OWNER);
    assert!(!rig.cache.marks_allowed(OWNER));
    assert!(
        rig.cache.analytic_allowed(OWNER),
        "a marks decline leaves the analytic route alone"
    );
    rig.end();
    for frame in 1..8 {
        rig.begin();
        assert!(!rig.cache.marks_allowed(OWNER), "frame {frame}");
        rig.end();
    }
    rig.begin();
    assert!(rig.cache.marks_allowed(OWNER), "frame 8 tries again");
    rig.end();
}

#[test]
fn a_recognition_nothing_finds_again_lives_one_frame() {
    let source = Remembering::default();
    let shape = circle_shape();
    source.0.borrow_mut().begin_frame();
    recognised_at(&source, &shape, 1.0 / 16.0).expect("a circle");
    source.0.borrow_mut().end_frame();
    assert_eq!(
        source.0.borrow().len(),
        1,
        "kept for the next frame to find"
    );
    source.0.borrow_mut().begin_frame();
    source.0.borrow_mut().end_frame();
    assert_eq!(
        source.0.borrow().len(),
        0,
        "nothing found it, so it is gone"
    );
}

#[test]
fn a_frame_that_finds_nothing_it_held_stops_keeping_until_a_probe() {
    let source = Remembering::default();
    let fresh = || zgui_svg::Shape {
        path: Arc::new(circle_shape().path.as_ref().clone()),
        ..circle_shape()
    };
    // Every frame draws new allocations, so no lookup finds anything.
    let mut kept = Vec::new();
    for _ in 0..16 {
        source.0.borrow_mut().begin_frame();
        let shapes: Vec<_> = (0..20).map(|_| fresh()).collect();
        let before = source.0.borrow().len();
        for shape in &shapes {
            recognised_at(&source, shape, 1.0 / 16.0);
        }
        kept.push(source.0.borrow().len() - before);
        source.0.borrow_mut().end_frame();
    }
    assert!(
        kept.iter().filter(|&&held| held == 0).count() >= 12,
        "an idle cache keeps entries only on probe frames: {kept:?}"
    );
    assert!(
        kept.iter().any(|&held| held > 0),
        "a probe keeps them: {kept:?}"
    );
}
