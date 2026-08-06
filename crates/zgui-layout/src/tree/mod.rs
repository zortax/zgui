//! The transient view of a document that the layout algorithms are driven over.

pub mod cache;
pub mod dirty;
pub mod gate;
pub mod partial;
pub mod print;
pub mod store;
pub mod traverse;

use core::cell::RefCell;

use taffy::{AvailableSpace, Size};
use zgui_dom::side::BoxKey;
use zgui_geom::{Device, DevicePx};
use zgui_profile::{Counter, counter};

use crate::inline::atomic::AtomicMemo;
use crate::inline::content::styles::TextStyles;
use crate::key::to_node_id;
use crate::measure::MeasureContent;
use crate::style::calc::CalcArena;
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
/// the box, beside the two cache storeys they are emptied with — see
/// [`BoxLayout::intrinsic`](crate::tree::store::state::BoxLayout::intrinsic), which carries the
/// argument in full. Moving them is what stops a `width: fit-content` button re-measuring its whole
/// subtree on every frame that lays anything out.
pub struct LayoutTree<'a, C> {
    /// The boxes and their results.
    store: &'a mut LayoutStore,
    /// Whoever can say how big a leaf's content is.
    content: &'a mut C,
    /// The two numbers no style carries.
    device: DeviceStyle,
    /// Where this pass interns `calc()`.
    calc: RefCell<CalcArena>,
    /// What each atomic inline's nested layout came out at, per constraint.
    atomic: AtomicMemo,
    /// The text properties of each distinct style this pass has met.
    text: TextStyles,
}

impl<'a, C: MeasureContent> LayoutTree<'a, C> {
    /// Borrows a store for one pass.
    pub fn new(store: &'a mut LayoutStore, content: &'a mut C, device: DeviceStyle) -> Self {
        Self {
            store,
            content,
            device,
            calc: RefCell::new(CalcArena::new(device.scale)),
            atomic: AtomicMemo::default(),
            text: TextStyles::default(),
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
        let Some(root) = self.store.root() else {
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
                || self.store.no_undecided_overflow()
                || !crate::scroll_region::auto::revise(self, root)
            {
                break;
            }
        }
        self.store.record_root_layout(viewport);
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
        if self.store.root().is_none() {
            return gate::Relayout::NoRoot;
        }
        if gate::stands(self.store, viewport) {
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

impl<C> LayoutTree<'_, C> {
    /// The boxes and their results.
    pub fn store(&self) -> &LayoutStore {
        self.store
    }

    /// The boxes and their results, for modification.
    pub fn store_mut(&mut self) -> &mut LayoutStore {
        self.store
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
        (self.store, self.content, &mut self.text)
    }

    /// The shrink-to-fit answers held for atomic inlines.
    pub fn atomic_memo(&self) -> &AtomicMemo {
        &self.atomic
    }

    /// The same, for modification.
    pub(crate) fn atomic_memo_mut(&mut self) -> &mut AtomicMemo {
        &mut self.atomic
    }

    /// How many `calc()` expressions this pass has interned.
    pub fn interned_calcs(&self) -> usize {
        self.calc.borrow().len()
    }

    /// A view over one box's style.
    pub(crate) fn style_of(&self, key: BoxKey) -> StyleRef<'_> {
        let node = self.store.node(key);
        let measured = MeasuredSizes {
            horizontal: self.store.intrinsic(key, crate::axis::Axis::Horizontal),
            vertical: self.store.intrinsic(key, crate::axis::Axis::Vertical),
        };
        StyleRef::new(node, &self.calc, self.device, measured, node.natural_ratio)
            .with_reserved_gutter(self.store.reserved_gutter(key))
    }

    /// The `calc()` arena, for resolving a handle the layout algorithms hand back.
    pub(crate) fn calc_arena(&self) -> &RefCell<CalcArena> {
        &self.calc
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
