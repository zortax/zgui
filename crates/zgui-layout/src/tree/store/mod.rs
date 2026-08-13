//! Where boxes, their results and their fragments live.
//!
//! | Module | Contents |
//! |---|---|
//! | `resolved` | one box's geometry, restated in this framework's own units |
//! | `state` | one box's layout-time state: the engine's cache, and what it produced |
//! | `fragments` | the pieces boxes were painted as |
//! | `inline` | what an inline formatting context resolved to |
//! | `scroll` | which axes reserve a scrollbar gutter |
//! | `laid_out` | the viewport the results now held were produced for |
//! | `measured` | the box's complete, bounded size-only answers |
//! | `roster` | the boxes whose style puts them in a class some pass has to visit |

pub(crate) mod content;
mod fragments;
mod full;
mod inline;
pub mod laid_out;
pub(crate) mod measured;
mod resolved;
pub(crate) mod roster;
mod scroll;
pub(crate) mod state;
pub(crate) mod styles;

#[cfg(test)]
mod tests;

use rustc_hash::FxHashMap;
use zgui_arena::{ArenaKind, ChunkArena, DocumentId, DomainId, PagedVec};
use zgui_css::ComputedStyle;
use zgui_dom::NodeKey;
use zgui_dom::side::{BoxKey, BoxList};

use crate::fragment::{FragKey, FragList, Fragment};
use crate::key::{named, slot};
use crate::node::box_node::BoxNode;
use crate::tree::store::content::{BoxContent, CustomBox, ReplacedBox};
use crate::tree::store::roster::{Roster, Rosters};
use crate::tree::store::state::BoxLayout;
use crate::tree::store::styles::{StyleSlot, StyleTable};

pub use crate::tree::store::resolved::ResolvedLayout;

/// Which of a document's arenas holds its boxes.
///
/// Every arena of a document indexes independently, so the same slot number occurs in all of them
/// at once; naming each arena once is what keeps a handle from resolving inside a key space it does
/// not belong to.
pub const BOX_ARENA: ArenaKind = match ArenaKind::new(1) {
    Some(kind) => kind,
    None => panic!("one is a valid arena kind"),
};

/// Which of a document's arenas holds its fragments.
pub const FRAGMENT_ARENA: ArenaKind = match ArenaKind::new(2) {
    Some(kind) => kind,
    None => panic!("two is a valid arena kind"),
};

/// Every box of one document, every box's result, and every fragment those results produced.
///
/// The store outlives a layout pass. What a pass borrows is the store plus whatever it needs to
/// measure content with; the boxes, the caches and the fragments stay put between frames, which is
/// what makes an incremental pass cheaper than a fresh one.
#[derive(Debug)]
pub struct LayoutStore {
    /// The box records.
    boxes: ChunkArena<BoxNode>,
    /// One entry per box, holding the engine's cache and the box's result.
    layout: PagedVec<BoxKey, Option<BoxLayout>, 64>,
    /// Intrinsics and identifiers held only by replaced boxes.
    replaced: PagedVec<BoxKey, Option<ReplacedBox>, 64>,
    /// Registry references held only by custom boxes.
    custom: PagedVec<BoxKey, Option<CustomBox>, 64>,
    /// The boxes whose style puts them in a class some pass has to visit.
    rosters: Rosters,
    /// Every distinct computed style a live box holds, interned once each.
    styles: StyleTable,
    /// Which interned style each box holds.
    style_slots: PagedVec<BoxKey, Option<StyleSlot>, 64>,
    /// The box the document is laid out from.
    root: Option<BoxKey>,
    /// Which boxes each element generated, in document order.
    boxes_of_node: PagedVec<NodeKey, BoxList>,
    /// The fragment records.
    fragments: ChunkArena<Fragment>,
    /// Which fragments each element's boxes produced.
    fragments_of_node: PagedVec<NodeKey, FragList>,
    /// The shaped paragraphs the document's inline formatting contexts resolved to.
    ///
    /// A fragment names a paragraph by its position here rather than by the key its glyphs are
    /// cached under, so that the identifier a fragment carries stays small and stays this store's
    /// to issue — there is one paragraph identity in the document, and it is this one.
    paragraphs: Vec<Option<ParagraphRecord>>,
    /// Shaping key to its compact document-local identifier.
    paragraph_index: FxHashMap<zgui_text::ParagraphKey, crate::fragment::ParagraphId>,
    /// Paragraph slots safe to reuse on a later frame.
    free_paragraphs: Vec<u32>,
    /// Paragraphs whose last inline resolution was released since the last fragment diff.
    unused_paragraphs: Vec<crate::fragment::ParagraphId>,
    /// Unique paragraph keys named by at least one current inline resolution.
    active_paragraphs: usize,
    /// The fragments destroyed since the last pass drained this list.
    ///
    /// A fragment ceases to exist for two quite different reasons — a box produced fewer pieces
    /// than last time, or the box itself was taken out of the tree — and only the first is
    /// something the walk that composes fragments ever sees. Recording both here, at the one place
    /// a fragment is actually destroyed, is what lets that walk take the names out of the hit index
    /// whether or not it ever visited the box they belonged to.
    retired: Vec<FragKey>,
    /// The fragments destroyed since the paint stage drained this list.
    ///
    /// The same names as [`LayoutStore::retired`], recorded twice because each drained list may
    /// have exactly one consumer: the fragment diff drains `retired` into the hit index, and the
    /// runtime drains this one into the paint cache, whose records live until the fragment does.
    /// The two run at different times — the diff inside every layout pass, the paint drain once
    /// before the emit walk — so a shared list would hand each name to whichever ran first.
    retired_paint: Vec<FragKey>,
    /// The boxes taken out of the tree since the last pass drained this list.
    ///
    /// Recorded for the same reason the fragments are, and read by whoever holds something *named
    /// after* a box: a coordinate system is named after the box that establishes it, and a walk that
    /// composes only what changed never visits a box that is no longer there to be told it has gone.
    /// Without this the names of deleted boxes are never given back, and the dense buffer they are
    /// the index of grows for the life of the process.
    retired_boxes: Vec<BoxKey>,
    /// Every fragment whose filter chain reads pixels outside every rectangle it writes.
    ///
    /// Sparse: empty in almost every document, a handful of entries in one with a blurred dialog
    /// or a frosted header. It is maintained as fragments gain, lose or are destroyed with such an
    /// extent rather than rebuilt, because a fragment that did not change this frame is never
    /// revisited and rebuilding would lose it exactly when the content beneath it animates.
    read_extents: Vec<FragKey>,
    /// How many inline formatting contexts have been flattened into a shaper's string.
    flattenings: u64,
    /// The viewport the last root layout that ran to completion was asked for, by its bits.
    ///
    /// See [`laid_out`](crate::tree::store::laid_out) for what it is compared against and why it is
    /// held here rather than beside the pass.
    laid_out: Option<(u32, u32)>,
}

#[derive(Clone, Copy, Debug)]
struct ParagraphRecord {
    key: zgui_text::ParagraphKey,
    users: u32,
}

impl LayoutStore {
    /// An empty store for one document.
    pub fn new(document: DocumentId) -> Self {
        let box_domain = DomainId::new(document, BOX_ARENA);
        let fragment_domain = DomainId::new(document, FRAGMENT_ARENA);
        Self {
            boxes: ChunkArena::new(box_domain),
            layout: PagedVec::for_domain(box_domain),
            replaced: PagedVec::for_domain(box_domain),
            custom: PagedVec::for_domain(box_domain),
            rosters: Rosters::default(),
            styles: StyleTable::default(),
            style_slots: PagedVec::for_domain(box_domain),
            root: None,
            boxes_of_node: PagedVec::for_domain(zgui_dom::id::document_id::node_domain(document)),
            fragments: ChunkArena::new(fragment_domain),
            fragments_of_node: PagedVec::for_domain(zgui_dom::id::document_id::node_domain(
                document,
            )),
            paragraphs: Vec::new(),
            paragraph_index: FxHashMap::default(),
            free_paragraphs: Vec::new(),
            unused_paragraphs: Vec::new(),
            active_paragraphs: 0,
            retired: Vec::new(),
            retired_paint: Vec::new(),
            retired_boxes: Vec::new(),
            read_extents: Vec::new(),
            flattenings: 0,
            laid_out: None,
        }
    }

    /// The arena boxes are named in.
    pub fn box_domain(&self) -> DomainId {
        self.boxes.domain()
    }

    /// How many boxes are live.
    pub fn len(&self) -> u32 {
        self.boxes.len()
    }

    /// Whether the document generated no boxes.
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }

    /// How many box slots the arena is holding, live and awaiting recycling.
    ///
    /// The high-water mark rather than the live count, which is what makes it the figure to watch:
    /// [`LayoutStore::remove`] keeps a box readable until the frame is recycled, so a document that
    /// rebuilds its whole box tree holds two documents' worth of slots until that happens. A leak
    /// here — a removal never recycled — is invisible in [`LayoutStore::len`] and shows up only
    /// as this number climbing frame after frame.
    pub fn box_capacity(&self) -> u32 {
        self.boxes.capacity()
    }

    /// The box the document is laid out from, if the tree has been built.
    pub fn root(&self) -> Option<BoxKey> {
        self.root
    }

    /// Makes `key` the box the document is laid out from.
    pub fn set_root(&mut self, key: BoxKey) {
        self.root = Some(key);
    }

    /// Adds a box, and gives it an empty result to be laid out into.
    pub fn insert(&mut self, node: BoxNode) -> BoxKey {
        self.insert_with_content(node, BoxContent::default())
    }

    /// Adds a box together with the rare content payloads established while it was built.
    pub(crate) fn insert_with_content(&mut self, mut node: BoxNode, content: BoxContent) -> BoxKey {
        let source = node.source;
        let style = node.style.clone();
        node.painted = if content.custom.is_some() {
            crate::node::kind::PaintedContent::Custom
        } else if content.draws_vector {
            crate::node::kind::PaintedContent::Vector
        } else if content.replaced.is_some() {
            crate::node::kind::PaintedContent::Replaced
        } else {
            crate::node::kind::PaintedContent::Box
        };
        let key = named(self.boxes.insert(node));
        let slot = self.styles.intern(&style);
        self.style_slots.replace(key, Some(slot));
        self.layout.replace(key, Some(BoxLayout::default()));
        if let Some(replaced) = content.replaced {
            self.replaced.replace(key, Some(replaced));
        }
        if let Some(custom) = content.custom {
            self.custom.replace(key, Some(custom));
        }
        self.classify(key, &style);
        if let Some(source) = source {
            self.boxes_of_node.get_mut(source).push(key);
        }
        key
    }

    /// Puts a new computed style on a live box, and reports whether that changed the allocation.
    ///
    /// The only way a box's style is ever replaced, and one of the two places it is ever
    /// established. Assigning `node.style` directly would leave the rosters describing the style
    /// the box used to have: a button restyled from `width: auto` to `width: fit-content` would
    /// never be measured, and one restyled the other way would be measured for ever.
    ///
    /// The test is allocation identity rather than value equality, which is what the caller wants
    /// and is documented where it is relied on — see
    /// [`patch::style`](crate::boxtree::patch::style).
    pub fn set_style(&mut self, key: BoxKey, style: &ComputedStyle) -> bool {
        let Some(node) = self.boxes.get_mut(slot(key)) else {
            return false;
        };
        if crate::style::same_cascade(&node.style, style) {
            return false;
        }
        node.style = style.clone();
        let slot = self.styles.intern(style);
        if let Some(held) = self.style_slots.replace(key, Some(slot)) {
            self.styles.release(held);
        }
        self.classify(key, style);
        true
    }

    /// The style a box's interned slot names, if the box holds a slot.
    pub(crate) fn interned_style(&self, key: BoxKey) -> Option<&ComputedStyle> {
        let slot = self.style_slots.get(key).copied().flatten()?;
        Some(self.styles.style(slot))
    }

    /// Lowers every interned style that owes a lowering for `device`.
    pub(crate) fn ensure_lowered_styles(&mut self, device: crate::style::DeviceStyle) {
        self.styles.ensure_lowered(device);
    }

    /// The read-only half of the store, beside the layout column borrowed for writing.
    ///
    /// This split is what a parallel batch runs on: the workers' exclusive borrows are carved out
    /// of the returned column, and everything structural — boxes, styles, replaced content — is
    /// shared through [`Structure`], which the borrow checker can see touches no layout state.
    pub(crate) fn split_for_batch(
        &mut self,
    ) -> (&mut PagedVec<BoxKey, Option<BoxLayout>, 64>, Structure<'_>) {
        (
            &mut self.layout,
            Structure {
                boxes: &self.boxes,
                styles: &self.styles,
                style_slots: &self.style_slots,
                replaced: &self.replaced,
                root: self.root,
            },
        )
    }

    /// The same read surface over a store that is not split, for the serial pass.
    pub(crate) fn structure(&self) -> Structure<'_> {
        Structure {
            boxes: &self.boxes,
            styles: &self.styles,
            style_slots: &self.style_slots,
            replaced: &self.replaced,
            root: self.root,
        }
    }

    /// How many `calc()` expressions the lowerings hold.
    pub fn interned_calcs(&self) -> usize {
        self.styles.interned_calcs()
    }

    /// How many distinct computed styles are interned.
    pub fn interned_styles(&self) -> usize {
        self.styles.live()
    }

    /// Records which style-defined classes a box belongs to, and enrols it in the ones it has
    /// joined.
    ///
    /// Called from the two places a box's style is established: [`LayoutStore::insert`] and
    /// [`LayoutStore::set_style`]. The membership bits are written unconditionally so that a box
    /// leaving a class is recorded at once; the list is only appended to, because a box that is
    /// already a member is already in it and the entries for boxes that have left are compacted
    /// away by whoever next walks the list.
    fn classify(&mut self, key: BoxKey, style: &ComputedStyle) {
        let content = crate::intrinsic::keywords::axes_of(style);
        let overflow = crate::style::convert::overflow::undecided_axes(style);
        let Some(state) = self.layout.get_mut(key).as_mut() else {
            return;
        };
        let joined_content = content != [false, false] && state.content_axes == [false, false];
        let joined_overflow =
            overflow != (false, false) && state.undecided_overflow == (false, false);
        state.content_axes = content;
        state.undecided_overflow = overflow;
        if joined_content {
            self.rosters.content.push(key);
        }
        if joined_overflow {
            self.rosters.overflow.push(key);
        }
    }

    /// Whether no box in the document is written with a content keyword.
    pub(crate) fn no_content_keywords(&self) -> bool {
        self.rosters.content.is_empty()
    }

    /// Whether no box in the document has an undecided gutter.
    pub(crate) fn no_undecided_overflow(&self) -> bool {
        self.rosters.overflow.is_empty()
    }

    /// Takes the content-keyword roster for the duration of one pass over it.
    pub(crate) fn take_content_roster(&mut self) -> Roster {
        Roster::take(&mut self.rosters.content)
    }

    /// Puts it back, compacted.
    pub(crate) fn restore_content_roster(&mut self, roster: Roster) {
        roster.restore(&mut self.rosters.content);
    }

    /// Takes the undecided-overflow roster for the duration of one pass over it.
    pub(crate) fn take_overflow_roster(&mut self) -> Roster {
        Roster::take(&mut self.rosters.overflow)
    }

    /// Puts it back, compacted.
    pub(crate) fn restore_overflow_roster(&mut self, roster: Roster) {
        roster.restore(&mut self.rosters.overflow);
    }

    /// Which axes of a box are written with a content keyword, as the roster recorded it.
    pub(crate) fn content_axes(&self, key: BoxKey) -> [bool; 2] {
        self.state(key).map_or([false, false], |it| it.content_axes)
    }

    /// Which axes of a box have an undecided gutter, as the roster recorded it.
    pub(crate) fn undecided_overflow(&self, key: BoxKey) -> (bool, bool) {
        self.state(key)
            .map_or((false, false), |it| it.undecided_overflow)
    }

    /// The record for one box.
    pub fn get(&self, key: BoxKey) -> Option<&BoxNode> {
        self.boxes.get(slot(key))
    }

    /// The record for one box, for modification.
    pub fn get_mut(&mut self, key: BoxKey) -> Option<&mut BoxNode> {
        self.boxes.get_mut(slot(key))
    }

    /// The record for one box, which must exist.
    ///
    /// # Panics
    ///
    /// If the key names no live box.
    pub fn node(&self, key: BoxKey) -> &BoxNode {
        self.get(key).expect("a live box key")
    }

    /// Whether a key names a live box.
    pub fn contains(&self, key: BoxKey) -> bool {
        self.boxes.contains_key(slot(key))
    }

    /// Removes one box. Its record stays readable until the frame is recycled.
    pub fn remove(&mut self, key: BoxKey) -> bool {
        self.retired_boxes.push(key);
        // The pieces this box was painted as cease to exist with it, and everything that named
        // them — the read-extent registry, the element's own list — has to be told.
        self.clear_fragments(key);
        if let Some(source) = self.get(key).and_then(|node| node.source) {
            self.boxes_of_node
                .get_mut(source)
                .retain(|&mut it| it != key);
        }
        self.take_inline_resolution(key);
        if let Some(slot) = self.style_slots.replace(key, None) {
            self.styles.release(slot);
        }
        self.layout.clear(key);
        self.replaced.clear(key);
        self.custom.clear(key);
        if self.root == Some(key) {
            self.root = None;
        }
        self.boxes.remove(slot(key))
    }

    /// Takes the boxes removed since the last call, leaving the list empty.
    ///
    /// Drained rather than read so that each removed box is handed out exactly once: a second
    /// reader would give back a name a third party has since been issued.
    pub fn drain_retired_boxes(&mut self) -> Vec<BoxKey> {
        core::mem::take(&mut self.retired_boxes)
    }

    /// Drops this frame's removed boxes and fragments and returns their slots to the allocator.
    pub fn recycle(&mut self) {
        self.boxes.recycle();
        self.fragments.recycle();
        self.layout.compact_by(Option::is_none);
        self.replaced.compact_by(Option::is_none);
        self.custom.compact_by(Option::is_none);
        self.style_slots.compact_by(Option::is_none);
    }

    /// The boxes one element generated, in document order.
    pub fn boxes_of(&self, node: NodeKey) -> &[BoxKey] {
        self.boxes_of_node.get(node).map_or(&[], |list| &list[..])
    }

    /// Walks every live box, in no particular order.
    pub fn keys(&self) -> Vec<BoxKey> {
        self.structure().keys()
    }
}

/// The store without its layout column: what a batch worker may read while the column is carved.
///
/// A view of shared borrows, so it is [`Copy`] and crosses into worker closures freely. Every
/// method here reads structure alone; per-box layout state goes through the worker's own
/// exclusive borrows, which is the whole point of the split.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Structure<'a> {
    /// The box records.
    boxes: &'a ChunkArena<BoxNode>,
    /// The interned styles and their lowerings.
    styles: &'a StyleTable,
    /// Which interned style each box holds.
    style_slots: &'a PagedVec<BoxKey, Option<StyleSlot>, 64>,
    /// Intrinsics and identifiers held only by replaced boxes.
    replaced: &'a PagedVec<BoxKey, Option<ReplacedBox>, 64>,
    /// The box the document is laid out from.
    root: Option<BoxKey>,
}

impl<'a> Structure<'a> {
    /// The record for one box.
    pub(crate) fn get(&self, key: BoxKey) -> Option<&'a BoxNode> {
        self.boxes.get(slot(key))
    }

    /// The record for one box, which must exist.
    ///
    /// # Panics
    ///
    /// If the key names no live box.
    pub(crate) fn node(&self, key: BoxKey) -> &'a BoxNode {
        self.get(key).expect("a live box key")
    }

    /// A live box's style in the layout algorithms' vocabulary.
    ///
    /// # Panics
    ///
    /// If the key names no live box, or no pass has lowered the styles yet.
    pub(crate) fn lowered_style(&self, key: BoxKey) -> &'a crate::style::lowered::LayoutStyle {
        let slot = self
            .style_slots
            .get(key)
            .copied()
            .flatten()
            .expect("a live box holds a style slot");
        self.styles.lowered(slot)
    }

    /// Resolves a `calc()` handle a lowering embedded.
    pub(crate) fn resolve_calc(&self, value: *const (), basis: f32) -> f32 {
        self.styles.resolve_calc(value, basis)
    }

    /// The replaced-content record one box holds, if it is a replaced box.
    pub(crate) fn replaced(&self, key: BoxKey) -> Option<&'a ReplacedBox> {
        self.replaced.get(key)?.as_ref()
    }

    /// Walks every live box, in no particular order.
    pub(crate) fn keys(&self) -> Vec<BoxKey> {
        let mut keys = Vec::new();
        if let Some(root) = self.root {
            let mut stack = vec![root];
            while let Some(key) = stack.pop() {
                keys.push(key);
                stack.extend(self.node(key).children.iter().copied());
            }
        }
        keys
    }
}
