//! One box's layout-time state, and the accessors a pass reaches it through.

use zgui_dom::side::BoxKey;

use crate::fragment::FragList;
use crate::inline::content::memo::Flattened;
use crate::inline::resolved::InlineResolution;
use crate::tree::store::LayoutStore;
use crate::tree::store::measured::Measured;

/// One box's layout-time state: what the engine cached, and what it produced.
#[derive(Debug, Default)]
pub(crate) struct BoxLayout {
    /// The engine's own per-node cache.
    pub(crate) cache: taffy::Cache,
    /// The size-only measurements the engine's own cache had no room to keep.
    ///
    /// The two are one cache with two storeys and are always emptied together — see
    /// [`BoxLayout::forget_layout`].
    pub(crate) measured: Measured,
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
    /// Both storeys of the cache go at once, and that is the whole of why this exists rather than
    /// each caller clearing what it happens to know about. An answer kept in one while the other
    /// was emptied is a measurement from before whatever invalidated the box, served in preference
    /// to taking it again — which is a document laid out to the sizes its content used to have,
    /// with nothing anywhere reporting that anything was skipped.
    pub(crate) fn forget_layout(&mut self) {
        self.cache.clear();
        self.measured.clear();
    }

    /// Whether this box is holding no answer about its own size.
    pub(crate) fn holds_no_layout(&self) -> bool {
        self.cache.is_empty() && self.measured.is_empty()
    }
}

impl LayoutStore {
    /// One box's layout-time state.
    pub(crate) fn state(&self, key: BoxKey) -> Option<&BoxLayout> {
        self.layout.get(key)
    }

    /// One box's layout-time state, for modification.
    pub(crate) fn state_mut(&mut self, key: BoxKey) -> &mut BoxLayout {
        self.layout
            .get_mut(key)
            .expect("every live box has a layout entry")
    }
}
