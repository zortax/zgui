//! The shaped paragraphs the text engine holds between frames.

use std::cell::RefCell;

use zgui_layout::tree::store::LayoutStore;

use crate::budget::epoch::SceneEpoch;
use crate::budget::manager::{Budgeted, Tracked};
use crate::budget::report::{CacheId, CacheReport, CacheUnit, rebuild};
use crate::text::TextEngine;

/// The window's shaped paragraphs, as the budget sees them.
///
/// # Why it is counted in paragraphs and not in bytes
///
/// The bulk of a shaped paragraph is inside the shaper's own form, which this framework carries and
/// never opens — that is what the seam is for. A byte figure would therefore be the parts around
/// that form added up and presented as the whole, which understates it by however much the shaper
/// allocates, and a level set against an understated figure is a level nobody could defend. What
/// can be counted exactly is how many results are held, so that is the unit and the level is stated
/// in it.
///
/// # Why eviction is all of them
///
/// [`ParagraphCache`](zgui_text::ParagraphCache) records no per-entry last use and the layout store
/// cannot say which boxes were measured from which paragraph, so there is no coldest few to take
/// and no way to invalidate exactly what dropping them invalidated. Dropping all of it and marking
/// the whole tree dirty is the operation that exists, it is correct, and it is expensive — which is
/// the reason the level is set high rather than the reason to pretend a finer one exists.
///
/// # The invalidation is not optional and is done here
///
/// Every measurement taken from a dropped paragraph goes with it: a box's cached size, its
/// baselines and the lines an inline formatting context resolved were all computed from shaped runs
/// that no longer exist, and a layout served from that cache asks for none of them again. Dropping
/// the shaping without marking the tree is how a document silently lays itself out with no glyphs
/// in it, on that frame and on every frame after. So the adapter holds the layout store as well as
/// the engine, and neither [`evict`](Budgeted::evict) nor [`forget`](Budgeted::forget) can be
/// reached without it.
pub struct ParagraphShapingBudget<'a> {
    /// What holds the shaped results.
    text: &'a mut dyn TextEngine,
    /// What has to be marked dirty when they go.
    layout: &'a RefCell<LayoutStore>,
    /// How many results may be held.
    limit: usize,
    /// This cache's own history.
    tracked: &'a mut Tracked,
}

impl<'a> ParagraphShapingBudget<'a> {
    /// The adapter over one window's text engine.
    pub fn new(
        text: &'a mut dyn TextEngine,
        layout: &'a RefCell<LayoutStore>,
        limit: usize,
        tracked: &'a mut Tracked,
    ) -> Self {
        Self {
            text,
            layout,
            limit,
            tracked,
        }
    }

    /// Drops every shaped result and marks everything measured from one as stale.
    ///
    /// Returns how many results went. Nothing dropped is nothing stale, which is the whole of a
    /// window that holds no text: the tree is not touched at all in that case.
    fn drop_shaping(&mut self) -> usize {
        let dropped = self.text.forget_shaped();
        if dropped > 0 {
            zgui_layout::tree::dirty::mark_all_dirty(&mut self.layout.borrow_mut());
        }
        dropped
    }
}

impl Budgeted for ParagraphShapingBudget<'_> {
    fn id(&self) -> CacheId {
        CacheId::ParagraphShaping
    }

    fn limit(&self) -> Option<u64> {
        Some(self.limit as u64)
    }

    fn report(&self) -> CacheReport {
        CacheReport {
            resident: self.text.shaped_held().paragraphs as u64,
            // Nothing. A shaped paragraph that is on the screen right now is not lost by being
            // dropped — the next layout pass shapes it again — so unlike a retained atlas tile
            // there is no set here that eviction must not touch. What dropping one costs is the
            // reshape and the relayout, and that is the rebuild cost rather than a pin.
            pinned: 0,
            last_used: self.tracked.last_used(),
            rebuild_cost: rebuild::RESHAPED,
            speculative: 0,
            unit: CacheUnit::Entries,
        }
    }

    fn observe(&mut self, epoch: SceneEpoch) {
        self.tracked
            .note(epoch, self.text.shaped_held().hits, false);
    }

    /// Drops every shaped paragraph, whatever `units` asked for.
    ///
    /// All or nothing, for the reason on the type: there is no coldest few to take. What comes back
    /// is therefore everything that was held, which is at least what was asked for.
    fn evict(&mut self, _units: u64, _epoch: SceneEpoch) -> u64 {
        self.drop_shaping() as u64
    }

    fn forget(&mut self) {
        self.drop_shaping();
    }
}
