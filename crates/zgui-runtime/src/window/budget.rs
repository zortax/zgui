//! Which of a window's fields are budgeted caches, and what the frame does about them.
//!
//! The registry is a visitor rather than a list, and this is why: the caches are fields of the
//! window and each of them needs a different second half of the window to do its part — the atlas
//! needs the renderer's texture sink, the shaping cache needs the layout store, the target pool is
//! only reachable through the renderer. Visiting them one at a time is what lets each borrow what
//! it needs for exactly as long as it is being visited, without any of them being moved out of the
//! window to be budgeted.
//!
//! **Adding a cache is one edit, here.** Every assertion about the registry walks it, so a cache
//! visited here is covered by them without anyone remembering to go and add it to a test.

use crate::budget::caches::{
    DecodedImagesBudget, DeviceMemoryBudget, GlyphAtlasBudget, ParagraphShapingBudget,
    RenderTargetsBudget, VectorResourcesBudget,
};
use crate::budget::manager::{self, Budgeted, CacheRegistry};
use crate::budget::report::{BudgetReport, CacheId};
use crate::budget::{CacheLimits, SceneEpoch};
use crate::window::Window;

impl CacheRegistry for Window {
    fn for_each(&mut self, visit: &mut dyn FnMut(&mut dyn Budgeted)) {
        visit(&mut GlyphAtlasBudget::new(
            &mut self.content,
            self.budgets.tracked(CacheId::GlyphAtlas),
        ));
        visit(&mut DecodedImagesBudget::new(
            &mut self.content,
            self.budgets.tracked(CacheId::DecodedImages),
        ));
        let shaped = self.budgets.limits().shaped_paragraphs;
        visit(&mut ParagraphShapingBudget::new(
            &mut *self.text,
            &self.layout,
            shaped,
            self.budgets.tracked(CacheId::ParagraphShaping),
        ));
        let drawings = self.budgets.limits().placed_drawings;
        visit(&mut VectorResourcesBudget::new(
            &mut self.vectors,
            drawings,
            self.budgets.tracked(CacheId::VectorResources),
        ));
        visit(&mut RenderTargetsBudget::new(
            &mut *self.renderer,
            self.budgets.tracked(CacheId::RenderTargets),
        ));
        // Last, and not a cache: everything else the renderer holds on the device. It frees
        // nothing and states no level; what it does is make the registry's total the process's
        // total rather than the small part of it that happens to be reproducible.
        visit(&mut DeviceMemoryBudget::new(
            &mut *self.renderer,
            self.budgets.tracked(CacheId::DeviceMemory),
        ));
    }
}

impl Window {
    /// What every budgeted cache is holding, read now.
    ///
    /// Not quite free: it sums every picture attached to the window. Something that wants to show
    /// the figures rather than act on them wants [`Window::last_budget_report`].
    pub fn budget_report(&mut self) -> BudgetReport {
        manager::report(self)
    }

    /// What every budgeted cache was holding when the budget was last enforced.
    ///
    /// The frame's own budget step reads this and acts on it, so handing it out again costs
    /// nothing. It is what an inspector shows: a panel that recomputed it would make the window
    /// take a figure it had already taken, once more per frame.
    pub fn last_budget_report(&self) -> BudgetReport {
        self.budgets.last_report()
    }

    /// The frame the budget is on.
    pub fn budget_epoch(&self) -> SceneEpoch {
        self.budgets.epoch()
    }

    /// The levels this window's entry-counted caches are held to.
    pub fn cache_limits(&self) -> CacheLimits {
        self.budgets.limits()
    }

    /// Changes the levels this window's entry-counted caches are held to.
    ///
    /// Nothing is freed here; the new levels take effect at the end of the next frame. The atlas's
    /// own level is not among them — it is a byte figure held by the atlas and set through
    /// [`ContentCache::set_soft_bytes`](zgui_paint::ContentCache::set_soft_bytes).
    pub fn set_cache_limits(&mut self, limits: CacheLimits) {
        self.budgets.set_limits(limits);
    }

    /// Records what every cache did this frame, then frees whatever is over its level.
    ///
    /// Called once per frame, at the end: after the emit walk, so that what is measured is this
    /// frame's working set and not the previous one's, and after the uploads have been flushed, so
    /// that nothing being freed is a tile this frame is about to draw from.
    ///
    /// The two halves are in that order and cannot be swapped. Observing is what stamps each cache
    /// with the frame it was last read in, and eviction order is decided from those stamps — a
    /// budget enforced before the frame was observed would be ordering the caches by what they were
    /// doing one frame ago.
    pub(crate) fn enforce_budgets(&mut self) {
        self.budgets.begin_frame();
        let epoch = self.budgets.epoch();
        manager::observe(self, epoch);
        let report = manager::report(self);
        manager::enforce(self, &report, epoch);
        self.budgets.record(report);
    }

    /// Drops everything every budgeted cache holds.
    ///
    /// The window keeps its document, its layout and its display list; what goes is everything that
    /// was retained only because it had been produced once already. The next frame produces all of
    /// it again, which is what makes this the operation a cold window is built with.
    ///
    /// It is not a memory-pressure step. Decoded images go with the rest, and nothing in this
    /// process can produce those again — see
    /// [`DecodedImagesBudget`].
    ///
    /// It asks for a frame and redraws the whole surface. Both are owed: the display list replays
    /// ranges that name rasters which no longer exist, and the tree has been marked as needing
    /// measurement again — a window left in that state and never woken would go on presenting the
    /// last frame it drew while holding nothing any of it came from.
    pub fn forget_caches(&mut self) {
        manager::forget_all(self);
        self.damage = zgui_bits::DamageSet::full();
        self.request_frame();
    }
}
