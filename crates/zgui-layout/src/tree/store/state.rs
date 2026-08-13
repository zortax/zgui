//! One box's layout-time state, and the accessors a pass reaches it through.

use zgui_dom::side::BoxKey;

use crate::axis::Axis;
use crate::fragment::FragList;
use crate::inline::content::memo::Flattened;
use crate::inline::resolved::InlineResolution;
use crate::style::convert::length::IntrinsicSizes;
use crate::tree::store::LayoutStore;
use crate::tree::store::full::FullLayout;
use crate::tree::store::measured::Measured;

/// One box's layout-time state: what the engine cached, and what it produced.
#[derive(Clone, Debug, Default)]
pub(crate) struct BoxLayout {
    /// The one full-layout answer a box may safely replay.
    pub(crate) full: FullLayout,
    /// Size-only measurements, keyed by the complete question.
    pub(crate) measured: Measured,
    /// What this box measured at its narrowest and its widest, per axis.
    ///
    /// The third storey of the cache, and the one thing here that survives a pass. It is emptied by
    /// the same call as the other two and for the same reason: it is an answer about this box's own
    /// content, and every change to that content invalidates this box by construction.
    ///
    /// # Why it may be held across frames at all
    ///
    /// Because it does not depend on the containing block, which is the property a measurement
    /// taken during layout does not have.
    /// [`prepass::probe`](crate::intrinsic::prepass) asks with `known_dimensions: NONE` and
    /// `parent_size: NONE` and an available space pinned to min-content or max-content, so a
    /// percentage anywhere inside the subtree resolves against nothing — exactly as it would on any
    /// other frame, at any other viewport. The answer is a function of the subtree's styles and
    /// text, the device scale and the gutters reserved inside it, and of nothing above the box.
    ///
    /// Each of those clears this box's cache when it moves: a style or a text rewrite marks the box
    /// and its ancestors through [`mark_dirty`](crate::tree::dirty::mark_dirty), a scale change goes
    /// through [`mark_all_dirty`](crate::tree::dirty::mark_all_dirty), and a gutter decision marks
    /// the box it was taken for. `mark_dirty` reaches this box from anywhere below it despite its
    /// early stop, because dirtiness is upward-closed — it stops only at an ancestor that is
    /// already holding nothing, and that ancestor cleared its own ancestors when it was marked.
    pub(crate) intrinsic: [Option<IntrinsicSizes>; 2],
    /// Which axes of this box are written as a content keyword.
    ///
    /// The authoritative half of the content-keyword roster; see
    /// [`roster`](crate::tree::store::roster) for why the list beside it is only a hint.
    pub(crate) content_axes: [bool; 2],
    /// Whether this box's overflow is undecided on each axis.
    ///
    /// The authoritative half of the undecided-overflow roster.
    pub(crate) undecided_overflow: (bool, bool),
    /// The engine's result before device-pixel snapping.
    pub(crate) unrounded: taffy::Layout,
    /// The engine's result the fragments this box currently holds were composed from.
    ///
    /// A fragment's absolute geometry is a function of two things and no others: where the box
    /// above it was placed, and this result. So a box whose parent did not move and whose result
    /// still equals this one has fragments that are already right, and the pass that composes them
    /// may leave it and everything below it alone.
    ///
    /// It is a separate field from [`BoxLayout::unrounded`] because the two answer different
    /// questions. The engine writes `unrounded` whenever it lays a box out, including when it
    /// arrives at the same numbers, and it leaves a box it never revisited holding numbers from
    /// several frames ago. Neither of those says whether the *fragments* agree with it. This does,
    /// and it is what stops a box being skipped because it is itself unchanged while an earlier
    /// sibling grew and pushed it sideways.
    pub(crate) composed: taffy::Layout,
    /// The result after snapping.
    pub(crate) snapped: taffy::Layout,
    /// The scroll and sticky offsets this box's children were last composed against.
    ///
    /// Everything a descendant's absolute position depends on is either its own layout result, the
    /// rounded origin above it — both of which [`BoxLayout::composed`] speaks for — or this. So the
    /// difference between this and the shift a new pass arrives at is exactly how far a clean
    /// subtree has moved, which is what lets the pass offset the subtree instead of composing it
    /// again.
    pub(crate) composed_shift: (f32, f32),
    /// The first baseline this box reported, measured down from its border-box top.
    pub(crate) first_baseline: Option<f32>,
    /// The last baseline it reported.
    pub(crate) last_baseline: Option<f32>,
    /// The lines this box resolved to, when it establishes an inline formatting context.
    pub(crate) inline: Option<Box<InlineResolution>>,
    /// The flattened form of the context this box establishes, when it establishes one.
    ///
    /// Beside the box rather than beside the pass, because flattening depends on the content and
    /// not on the width it is asked about: a paragraph probed at twenty widths, and a document laid
    /// out again because something elsewhere moved, both reuse it.
    pub(crate) flattened: Option<Box<Flattened>>,
    /// The shrink-to-fit answers this box holds, when it is an atomic inline that was measured.
    ///
    /// The fourth cache storey, emptied with the other three. See
    /// [`AtomicAnswers`](crate::inline::atomic::AtomicAnswers) for why the constraint key lets it
    /// survive a pass.
    pub(crate) atomic: Option<Box<crate::inline::atomic::AtomicAnswers>>,
    /// The fragments this box produced.
    pub(crate) fragments: FragList,
    /// Which axes of an `overflow: auto` box were decided to scroll.
    ///
    /// Kept between frames so that the next layout starts from the previous answer instead of
    /// from "reserves nothing", which is what stops a scrollport's gutter appearing and
    /// disappearing while its content is edited.
    pub(crate) auto_scroll: (bool, bool),
    /// The gutter this box keeps reserved while it is locked, if it is.
    pub(crate) scroll_lock: Option<(bool, bool)>,
}

impl BoxLayout {
    /// Throws away every answer this box is holding about its own size.
    ///
    /// All three storeys of the cache go at once, and that is the whole of why this exists rather
    /// than each caller clearing what it happens to know about. An answer kept in one while the
    /// others were emptied is a measurement from before whatever invalidated the box, served in
    /// preference to taking it again — which is a document laid out to the sizes its content used
    /// to have, with nothing anywhere reporting that anything was skipped.
    pub(crate) fn forget_layout(&mut self) {
        self.forget_cached_sizes();
        self.intrinsic = [None, None];
        if let Some(answers) = self.atomic.as_deref_mut() {
            answers.clear();
        }
    }

    /// Throws away the two cache storeys, keeping the intrinsic answer.
    ///
    /// The intrinsic pre-pass alone may call this, and only immediately after computing that
    /// answer. What it is clearing is the entries its own probes stored on the way to the answer,
    /// which were taken while the box's keyword still read as `auto` and would otherwise be served
    /// back during the real layout, when the keyword means a length. Keeping the answer it has just
    /// computed is the point of the call.
    ///
    /// Nothing else may use it. Every other caller is invalidating the box because something it
    /// measured has changed, and for those the intrinsic answer is exactly as stale as the rest.
    pub(crate) fn forget_cached_sizes(&mut self) {
        self.full.clear();
        self.measured.clear();
    }

    /// Whether this box is holding no answer about its own size.
    pub(crate) fn holds_no_layout(&self) -> bool {
        self.full.is_empty() && self.measured.is_empty()
    }
}

impl LayoutStore {
    /// What one box measured on one axis, if it is still holding the answer.
    pub(crate) fn intrinsic(&self, key: BoxKey, axis: Axis) -> Option<IntrinsicSizes> {
        self.state(key)?.intrinsic[axis.index()]
    }

    /// Records what one box measured on one axis.
    pub(crate) fn set_intrinsic(&mut self, key: BoxKey, axis: Axis, sizes: IntrinsicSizes) {
        self.state_mut(key).intrinsic[axis.index()] = Some(sizes);
    }
}

impl LayoutStore {
    /// One box's layout-time state.
    pub(crate) fn state(&self, key: BoxKey) -> Option<&BoxLayout> {
        self.layout.get(key)?.as_ref()
    }

    /// One box's layout-time state, for modification.
    pub(crate) fn state_mut(&mut self, key: BoxKey) -> &mut BoxLayout {
        self.layout
            .get_mut(key)
            .as_mut()
            .expect("every live box has a layout entry")
    }
}
