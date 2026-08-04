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
/// Current inline resolutions pin their shaping. Everything else is an old version or content no
/// longer present in the document, and can be removed without invalidating layout.
///
/// # Full invalidation is reserved for an explicit reset
///
/// Budget eviction only chooses entries no current inline resolution names, so no cached
/// measurement depends on what it drops. An explicit [`forget`](Budgeted::forget), such as device
/// recovery or a font-set reset, still drops active results too; that operation marks the whole
/// layout tree dirty before any cached measurement can be served from missing shaping.
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
        let resident = self.text.shaped_held().paragraphs as u64;
        let active = self.layout.borrow().active_paragraph_count() as u64;
        CacheReport {
            resident,
            // A current resolution is both visible state and a dependency of cached measurements.
            // It is a soft-limit pin: an active document larger than the limit remains over rather
            // than being deliberately made to reshape on every frame.
            pinned: active.min(resident),
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

    /// Drops only cold entries no current inline resolution names.
    fn evict(&mut self, units: u64, _epoch: SceneEpoch) -> u64 {
        let active = self.layout.borrow().active_paragraph_keys();
        self.text.evict_inactive(&active, units as usize) as u64
    }

    fn forget(&mut self) {
        self.drop_shaping();
    }
}
