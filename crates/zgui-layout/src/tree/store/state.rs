//! One box's layout-time state, and the accessors a pass reaches it through.

use zgui_dom::side::BoxKey;

use crate::axis::Axis;
use crate::fragment::FragList;
use crate::inline::content::memo::Flattened;
use crate::inline::resolved::InlineResolution;
use crate::style::convert::length::IntrinsicSizes;
use crate::tree::store::LayoutStore;

/// One box's layout-time state: what the engine cached, and what it produced.
#[derive(Clone, Debug, Default)]
pub(crate) struct BoxLayout {
    /// The engine's cache: one final answer and the size-only answers, keyed by the complete
    /// question and tagged with the generation that last read them.
    pub(crate) cache: cephal::NodeCache,
    /// The scheduler's bits for this box (low nibble), see [`cephal::schedule`].
    pub(crate) flags: u8,
    /// The pass that last recorded this box's previous result on the touched list.
    touched_generation: u32,
    /// Whether the engine style the box holds makes it a scroll container.
    ///
    /// The engine counts a scroll container's own end padding into its scrollable overflow, and
    /// a content size stated from the content-box origin has to take it off again.
    pub(crate) scroll_container: bool,
    /// What the last size probe over this box's lines computed, for the kept pass to adopt.
    ///
    /// See [`ProbeMemo`](crate::inline::measure::ProbeMemo). Dropped with the cache: a box marked
    /// dirty may have new content, and the memo is only ever trusted within one pass anyway.
    pub(crate) probe_memo: Option<Box<crate::inline::measure::ProbeMemo>>,
    /// Whether a size probe over this box's lines is on [`LayoutStore::probed`].
    pub(crate) probed: bool,
    /// What the engine recorded about this box's absolutely positioned children, so that one of
    /// them can be placed again without the box running its own algorithm.
    pub(crate) absolute: Option<Box<cephal::tree::AbsoluteContext>>,
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
    pub(crate) unrounded: cephal::Layout,
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
    pub(crate) composed: cephal::Layout,
    /// The result after snapping.
    pub(crate) snapped: cephal::Layout,
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
    /// The range of hit-order keys this box's subtree spans, see
    /// [`LayoutStore::subtree_order`](crate::LayoutStore::subtree_order).
    pub(crate) subtree_order: SubtreeOrder,
    /// Which axes of an `overflow: auto` box were decided to scroll.
    ///
    /// Kept between frames so that the next layout starts from the previous answer instead of
    /// from "reserves nothing", which is what stops a scrollport's gutter appearing and
    /// disappearing while its content is edited.
    pub(crate) auto_scroll: (bool, bool),
    /// The gutter this box keeps reserved while it is locked, if it is.
    pub(crate) scroll_lock: Option<(bool, bool)>,
}

/// The range of hit-order keys one box's subtree spans, as `(first, last)`.
///
/// Held per box, beside the box's fragments: every piece of a box spans the same range. Empty,
/// `(MAX, 0)`, until the fragment pass or the hit index folds it, and again when the box loses
/// its last fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SubtreeOrder(pub(crate) (u64, u64));

impl SubtreeOrder {
    /// The range of a subtree with no indexed fragment, which folds away.
    pub(crate) const EMPTY: Self = Self((u64::MAX, 0));
}

impl Default for SubtreeOrder {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl BoxLayout {
    /// The pass that last noted this box as touched.
    pub(crate) fn cache_generation(&self) -> u32 {
        self.touched_generation
    }

    /// Notes that `generation` recorded this box's previous result.
    pub(crate) fn note_touched(&mut self, generation: u32) {
        self.touched_generation = generation;
    }

    /// Throws away every answer this box is holding about its own size.
    ///
    /// All three storeys of the cache go at once, and that is the whole of why this exists rather
    /// than each caller clearing what it happens to know about. An answer kept in one while the
    /// others were emptied is a measurement from before whatever invalidated the box, served in
    /// preference to taking it again — which is a document laid out to the sizes its content used
    /// to have, with nothing anywhere reporting that anything was skipped.
    pub(crate) fn forget_layout(&mut self) {
        self.forget_cached_sizes();
        self.probe_memo = None;
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
        self.cache.clear();
    }

    /// An estimate of the heap bytes this box's layout state holds, one level deep.
    ///
    /// Counts the boxed records and the capacity of the vectors directly inside them. Content that
    /// is shared through an `Arc`, and the engine's own cache records, are left out.
    pub(crate) fn heap_bytes(&self) -> usize {
        use core::mem::size_of;
        let probe = self.probe_memo.as_deref().map_or(0, |memo| {
            size_of::<crate::inline::measure::ProbeMemo>() + memo.heap_bytes()
        });
        let absolute = self
            .absolute
            .as_ref()
            .map_or(0, |_| size_of::<cephal::tree::AbsoluteContext>());
        let inline = self.inline.as_deref().map_or(0, |inline| {
            size_of::<InlineResolution>() + inline.heap_bytes()
        });
        let flattened = self.flattened.as_deref().map_or(0, |flattened| {
            size_of::<Flattened>() + flattened.heap_bytes()
        });
        let atomic = self
            .atomic
            .as_ref()
            .map_or(0, |_| size_of::<crate::inline::atomic::AtomicAnswers>());
        let fragments = if self.fragments.spilled() {
            self.fragments.capacity() * size_of::<crate::fragment::FragKey>()
        } else {
            0
        };
        probe + absolute + inline + flattened + atomic + fragments
    }

    /// Whether this box is holding no answer about its own size.
    pub(crate) fn holds_no_layout(&self) -> bool {
        self.cache.is_empty()
    }

    /// The scrollable extent of this result, measured from the content-box corner.
    ///
    /// The engine measures overflow from the padding-box corner and counts a scroll container's
    /// end padding into it; every reader of a content size in this workspace compares it with the
    /// content box, so both paddings come off.
    pub(crate) fn content_size_of(
        layout: &cephal::Layout,
        scroll_container: bool,
    ) -> cephal::Size<f32> {
        let end = if scroll_container {
            cephal::Size {
                width: layout.padding.right,
                height: layout.padding.bottom,
            }
        } else {
            cephal::Size::ZERO
        };
        cephal::Size {
            width: (layout.scrollable_overflow_rect.right - layout.padding.left - end.width)
                .max(0.0),
            height: (layout.scrollable_overflow_rect.bottom - layout.padding.top - end.height)
                .max(0.0),
        }
    }

    /// The overflow rectangle of a leaf whose content fills its content box exactly.
    pub(crate) fn overflow_of_content_box(&self) -> cephal::Rect<f32> {
        let layout = &self.unrounded;
        let content = layout.content_box_size();
        let end = if self.scroll_container {
            cephal::Size {
                width: layout.padding.right,
                height: layout.padding.bottom,
            }
        } else {
            cephal::Size::ZERO
        };
        cephal::Rect {
            left: 0.0,
            top: 0.0,
            right: layout.padding.left + content.width.max(0.0) + end.width,
            bottom: layout.padding.top + content.height.max(0.0) + end.height,
        }
    }
}

impl LayoutStore {
    /// The sum of `BoxLayout::heap_bytes` over every box, for a memory report.
    #[doc(hidden)]
    pub fn layout_heap_bytes(&self) -> usize {
        self.keys()
            .into_iter()
            .filter_map(|key| self.state(key))
            .map(BoxLayout::heap_bytes)
            .sum()
    }

    /// The size of one box's layout state record, for a memory report.
    #[doc(hidden)]
    pub const BOX_LAYOUT_SIZE: usize = core::mem::size_of::<BoxLayout>();

    /// What one box measured on one axis, if it is still holding the answer.
    pub(crate) fn intrinsic(&self, key: BoxKey, axis: Axis) -> Option<IntrinsicSizes> {
        self.state(key)?.intrinsic[axis.index()]
    }

    /// Records what one box measured on one axis; the box's engine style is owed a refresh.
    pub(crate) fn set_intrinsic(&mut self, key: BoxKey, axis: Axis, sizes: IntrinsicSizes) {
        self.state_mut(key).intrinsic[axis.index()] = Some(sizes);
        self.touch_engine_style(key);
    }
}

impl LayoutStore {
    /// One box's layout-time state.
    pub(crate) fn state(&self, key: BoxKey) -> Option<&BoxLayout> {
        self.layout.get(key)?.as_ref()
    }

    /// One box's layout-time state, for modification, if the box has any.
    pub(crate) fn state_opt_mut(&mut self, key: BoxKey) -> Option<&mut BoxLayout> {
        self.layout.get(key)?.as_ref()?;
        self.layout.get_mut(key).as_mut()
    }

    /// One box's layout-time state, for modification.
    pub(crate) fn state_mut(&mut self, key: BoxKey) -> &mut BoxLayout {
        self.layout
            .get_mut(key)
            .as_mut()
            .expect("every live box has a layout entry")
    }
}
