//! The transient view of a document that the layout algorithms are driven over.

pub mod cache;
pub mod dirty;
mod executor;
pub mod gate;
pub mod parallel;
pub mod partial;
pub mod print;
pub mod store;
pub mod traverse;
pub(crate) mod view;

use taffy::{AvailableSpace, Size};
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx};
use zgui_profile::{Counter, counter};

use crate::inline::atomic::AtomicMemo;
use crate::inline::content::styles::TextStyles;
use crate::key::to_node_id;
use crate::measure::MeasureContent;
use crate::style::{DeviceStyle, MeasuredSizes, StyleRef};
use crate::tree::store::LayoutStore;

/// A document's boxes, borrowed for the duration of one layout pass.
///
/// Layout never owns the document. What it owns is the box records and their results, and what it
/// borrows for a pass is those plus whatever can say how big a piece of content is.
///
/// The `calc()` arena lives here rather than in the store because it is a property of *one pass*:
/// its handles stop meaning anything when the pass ends.
///
/// The intrinsic measurements used to live here too, on the reasoning that a measurement is taken
/// against a containing block the next pass may change. That reasoning does not survive reading the
/// probe that takes them: [`prepass`](crate::intrinsic::prepass) asks with no known dimensions, no
/// parent size and an available space pinned to min-content or max-content, so no containing block
/// enters the answer and there is nothing about it for a new pass to invalidate. They now live on
/// the box, beside the two cache storeys they are emptied with, and the store's
/// `BoxLayout::intrinsic` carries the argument in full. Moving them is what stops a
/// `width: fit-content` button re-measuring its whole subtree on every frame that lays anything
/// out.
pub struct LayoutTree<'a, C> {
    /// The boxes and their results, as this pass may reach them.
    store: view::StoreView<'a>,
    /// Whoever can say how big a leaf's content is.
    content: &'a mut C,
    /// The two numbers no style carries.
    device: DeviceStyle,
    /// What each atomic inline's nested layout came out at, per constraint.
    atomic: AtomicMemo,
    /// The text properties of each distinct style this pass has met.
    text: TextStyles,
    /// Whoever lays custom elements out, when a window has any.
    custom: &'a dyn crate::custom::CustomLayoutSource,
    /// Whether a custom source was installed, which is what keeps batches off.
    ///
    /// A custom source carries no `Sync` bound, so a batch would hand workers something the type
    /// system cannot vouch for. Tracked as a flag beside the reference because the installed
    /// source replaces the inert default and the two cannot be told apart through the trait.
    has_custom: bool,
    /// The pool parallel batches run on, when the application installed one.
    parallel: Option<&'a parallel::LayoutPool>,
}

impl<'a, C: MeasureContent> LayoutTree<'a, C> {
    /// Borrows a store for one pass.
    ///
    /// Lowers whatever styles the frame's writes left owing before any algorithm reads one.
    pub fn new(store: &'a mut LayoutStore, content: &'a mut C, device: DeviceStyle) -> Self {
        store.ensure_lowered_styles(device);
        Self {
            store: view::StoreView::Exclusive(store),
            content,
            device,
            atomic: AtomicMemo::default(),
            text: TextStyles::default(),
            custom: &crate::custom::NoCustomLayout,
            has_custom: false,
            parallel: None,
        }
    }

    /// Lays the whole document out into a viewport of the given size.
    ///
    /// Returns whether there was a root to lay out.
    ///
    /// No device-pixel rounding happens here. Rounding needs each box's cumulative absolute origin,
    /// and so does composing the fragment tree, so the two are one walk and it is the fragment
    /// pass — running it here as well would compute every origin twice, over every box, on every
    /// frame that laid anything out at all.
    pub fn layout_root(&mut self, viewport: Size<f32>) -> bool {
        let Some(root) = self.store.get().root() else {
            return false;
        };
        counter::bump(Counter::LayoutReachedRoot);
        for pass in 0..crate::scroll_region::auto::MAX_PASSES {
            crate::intrinsic::prepass::run(self, root);
            taffy::compute_root_layout(
                self,
                to_node_id(root),
                Size {
                    width: AvailableSpace::Definite(viewport.width),
                    height: AvailableSpace::Definite(viewport.height),
                },
            );
            // The second pass exists only to revise a gutter, so a document with no undecided
            // gutter in it never enters one — and asking is a test against a list rather than a
            // walk of the tree.
            if pass + 1 == crate::scroll_region::auto::MAX_PASSES
                || self.store.get().no_undecided_overflow()
                || !crate::scroll_region::auto::revise(self, root)
            {
                break;
            }
        }
        self.store.get_mut().record_root_layout(viewport);
        true
    }

    /// The same pass, skipped when the results already held are the ones it would produce.
    ///
    /// The pass is the largest thing a frame does and most frames do not need one: a colour that
    /// moved, a caret that blinked and an animation that only repaints all leave every box where
    /// the previous pass put it. [`gate`] states exactly what is compared, and the answer is
    /// derived from the same invalidation the algorithms themselves read, so a document that would
    /// lay out to different numbers can never be held.
    ///
    /// [`LayoutTree::layout_root`] is the ungated pass, and stays so: it is what a caller driving
    /// its own fixpoint wants, and what a test comparing a held frame against a fresh one compares
    /// it to.
    pub fn relayout_root(&mut self, viewport: Size<f32>) -> gate::Relayout {
        if self.store.get().root().is_none() {
            return gate::Relayout::NoRoot;
        }
        if gate::stands(self.store.get(), viewport) {
            counter::bump(Counter::LayoutsHeld);
            return gate::Relayout::Held;
        }
        self.layout_root(viewport);
        gate::Relayout::Ran
    }

    /// The same pass, into a viewport `width` by `height` device pixels across.
    ///
    /// [`LayoutTree::layout_root`] names the layout algorithms' own size type, which only this
    /// crate may name. This is the same call spelled in numbers, so a caller outside the crate can
    /// lay a document out without reaching for that type.
    pub fn layout_viewport(&mut self, width: f32, height: f32) -> bool {
        self.layout_root(Size { width, height })
    }
}

impl<'a, C> LayoutTree<'a, C> {
    /// The same pass, laying custom elements out through `custom`.
    #[must_use]
    pub fn with_custom(mut self, custom: &'a dyn crate::custom::CustomLayoutSource) -> Self {
        self.custom = custom;
        self.has_custom = true;
        self
    }

    /// The same pass, running independent measurement batches on `pool`'s workers.
    ///
    /// The caller must have pre-flattened every dirty inline context — the frame's pre-shape
    /// prepass does — because a batch worker may claim no brush slot. A pass without that
    /// guarantee stays serial by not installing the pool.
    #[must_use]
    pub fn with_parallel(mut self, pool: &'a parallel::LayoutPool) -> Self {
        self.parallel = Some(pool);
        self
    }

    /// Whoever lays custom elements out.
    pub(crate) fn custom(&self) -> &'a dyn crate::custom::CustomLayoutSource {
        self.custom
    }

    /// The boxes and their results.
    pub fn store(&self) -> &LayoutStore {
        self.store.get()
    }

    /// The boxes and their results, for modification.
    pub fn store_mut(&mut self) -> &mut LayoutStore {
        self.store.get_mut()
    }

    /// One box's layout state, as this pass may see it.
    ///
    /// A worker's own writes are visible here and through nothing else; see
    /// [`view::StoreView`] for the discipline.
    pub(crate) fn state(&self, key: BoxKey) -> Option<&crate::tree::store::state::BoxLayout> {
        self.store.state(key)
    }

    /// The same, for writing.
    pub(crate) fn state_mut(&mut self, key: BoxKey) -> &mut crate::tree::store::state::BoxLayout {
        self.store.state_mut(key)
    }

    /// The flattened form one box is holding.
    pub(crate) fn flattened_of(
        &self,
        key: BoxKey,
    ) -> Option<&crate::inline::content::memo::Flattened> {
        self.store.flattened(key)
    }

    /// Holds a box's flattened form.
    pub(crate) fn hold_flattened(
        &mut self,
        key: BoxKey,
        flattened: crate::inline::content::memo::Flattened,
    ) {
        self.store.hold_flattened(key, flattened);
    }

    /// The lines one box resolved to.
    pub(crate) fn inline_resolution_of(
        &self,
        key: BoxKey,
    ) -> Option<&crate::inline::resolved::InlineResolution> {
        self.store.inline_resolution(key)
    }

    /// Records what one inline formatting context resolved to.
    pub(crate) fn set_inline_resolution(
        &mut self,
        key: BoxKey,
        resolution: crate::inline::resolved::InlineResolution,
    ) {
        self.store.set_inline_resolution(key, resolution);
    }

    /// The identifier a shaped paragraph is carried by.
    pub(crate) fn intern_paragraph(
        &mut self,
        key: zgui_text::ParagraphKey,
    ) -> crate::fragment::ParagraphId {
        self.store.intern_paragraph(key)
    }

    /// The two numbers no style carries.
    pub fn device(&self) -> DeviceStyle {
        self.device
    }

    /// Whoever can say how big a leaf's content is.
    pub(crate) fn content(&mut self) -> &mut C {
        self.content
    }

    /// The text properties of each distinct style met so far.
    pub(crate) fn text_styles_mut(&mut self) -> &mut TextStyles {
        &mut self.text
    }

    /// How many distinct text styles this pass has lowered.
    pub fn lowered_text_styles(&self) -> usize {
        self.text.len()
    }

    /// The three things flattening a context needs at once: the boxes to read, the measurer to ask
    /// for brush slots, and the lowered styles to reuse.
    ///
    /// Handed out together because they are three fields of one borrow and flattening needs all
    /// three; taking them one at a time would borrow the whole tree three times over.
    pub(crate) fn content_parts(&mut self) -> (&LayoutStore, &mut C, &mut TextStyles) {
        (self.store.get(), self.content, &mut self.text)
    }

    /// The shrink-to-fit answers held for atomic inlines.
    pub fn atomic_memo(&self) -> &AtomicMemo {
        &self.atomic
    }

    /// The same, for modification.
    pub(crate) fn atomic_memo_mut(&mut self) -> &mut AtomicMemo {
        &mut self.atomic
    }

    /// How many `calc()` expressions the lowerings hold.
    pub fn interned_calcs(&self) -> usize {
        self.store.get().interned_calcs()
    }

    /// A view over one box's style.
    pub(crate) fn style_of(&self, key: BoxKey) -> StyleRef<'_> {
        let structure = self.store.structure();
        let node = structure.node(key);
        let measured = MeasuredSizes {
            horizontal: self.store.intrinsic(key, crate::axis::Axis::Horizontal),
            vertical: self.store.intrinsic(key, crate::axis::Axis::Vertical),
        };
        let natural_ratio = structure.replaced(key).and_then(|content| content.ratio);
        let lowered = structure.lowered_style(key);
        StyleRef::new(node, lowered, self.device, measured, natural_ratio)
            .with_reserved_gutter(self.store.reserved_gutter(key))
    }

    /// The structural half of the store, readable in both modes.
    pub(crate) fn structure(&self) -> crate::tree::store::Structure<'_> {
        self.store.structure()
    }

    /// Resolves a `calc()` handle the layout algorithms hand back.
    pub(crate) fn resolve_calc(&self, value: *const (), basis: f32) -> f32 {
        self.store.structure().resolve_calc(value, basis)
    }
}

/// The size of a viewport, as the layout algorithms want it.
pub fn viewport(width: DevicePx, height: DevicePx) -> Size<f32> {
    Size {
        width: width.0,
        height: height.0,
    }
}

/// The size of a viewport measured in this framework's own geometry.
pub fn viewport_of(size: zgui_geom::Size<DevicePx, Device>) -> Size<f32> {
    viewport(size.width, size.height)
}
