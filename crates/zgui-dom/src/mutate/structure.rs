//! Which siblings a change to a child list can have changed the match of.
//!
//! Inserting or removing a child changes what `:nth-child`, `:last-of-type`, `:empty` and every
//! `+` and `~` combinator match on the *other* children, and none of that is expressible as a
//! record of the changed node: the node that changed is not the node whose style changed. The style
//! engine records the dependency while it matches — it sets a flag on the parent saying "a selector
//! here cares about this child list" — but consumes none of it, so the expansion is entirely this
//! crate's obligation, and a document that skips it looks like one whose sibling selectors simply
//! do not work.
//!
//! # The entry records what is observable and compares nothing
//!
//! Every structural change under one parent, in one batch, folds into one entry. The entry holds
//! the parent's selector flags, the pair of edge children as they were before the batch, and up to
//! four *anchors* — the earliest child each individual change could affect. Nothing is compared
//! while the batch is open, and that is the entire point: a change invalidates the positions of
//! every element sibling under that parent, so asking "which anchor is earliest" per change would
//! renumber the child list per change. Deferring the comparison to the close of the batch pays for
//! one renumber however many changes there were, which is the difference between a thousand-row
//! reorder being linear and being quadratic.
//!
//! # Why the pre-batch edge pair is stored rather than recomputed
//!
//! Prepending under `.group > :first-child` leaves the element that *was* first neither the new
//! first nor the last, so the pair "first and last as they are now" does not reach it, and the
//! inserted node's own restyle does not either. The style engine sets the edge-child flag
//! exclusively of the two whole-child-list flags, so no other arm covers it. Its mirror needs no
//! stored value — a removed first child has left the document, and a moved one is restyled where it
//! landed — which is why every field is filtered by "is this still a child of this parent" rather
//! than assumed to name something live.

use selectors::matching::ElementSelectorFlags;
use smallvec::SmallVec;

use crate::arena::store::DocumentStore;
use crate::id::node_key::{NodeIndex, OptIndex};
use crate::mutate::hints::HintLog;
use crate::mutate::ordinals;

/// How many anchors an entry names exactly before it gives up and says "every child".
pub(crate) const ANCHORS: usize = 4;

/// Everything one batch's changes to one parent's child list recorded.
struct Entry {
    /// The parent whose child list changed.
    parent: NodeIndex,
    /// The union of the selector flags read off the parent at each change.
    flags: ElementSelectorFlags,
    /// The parent's first and last element child as they were before the batch, once a structural
    /// change has written them.
    edge_before: Option<(OptIndex, OptIndex)>,
    /// The earliest child each change can affect, in the order they were recorded.
    anchors: SmallVec<[NodeIndex; ANCHORS]>,
    /// Set once a fifth anchor arrives, after which the entry names every child instead.
    all_children: bool,
}

impl Entry {
    /// An entry for `parent` with nothing recorded yet.
    fn new(parent: NodeIndex) -> Self {
        Self {
            parent,
            flags: ElementSelectorFlags::empty(),
            edge_before: None,
            anchors: SmallVec::new(),
            all_children: false,
        }
    }

    /// Adds `anchor`, degrading to "every child" on the fifth distinct one.
    fn push_anchor(&mut self, anchor: NodeIndex) {
        if self.all_children || self.anchors.contains(&anchor) {
            return;
        }
        if self.anchors.len() == ANCHORS {
            self.all_children = true;
            self.anchors.clear();
            return;
        }
        self.anchors.push(anchor);
    }
}

/// Every parent whose child list changed while one batch was open.
#[derive(Default)]
pub struct StructureLog {
    /// One entry per parent, in the order the parents were first touched. A batch touches a
    /// handful of parents, so a scan beats a hash.
    entries: Vec<Entry>,
}

impl StructureLog {
    /// A log with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many parents have an entry.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry for `parent`, creating an empty one if this is its first record this batch.
    fn entry(&mut self, parent: NodeIndex) -> &mut Entry {
        if let Some(position) = self.entries.iter().position(|entry| entry.parent == parent) {
            return &mut self.entries[position];
        }
        self.entries.push(Entry::new(parent));
        self.entries.last_mut().expect("an entry was just pushed")
    }

    /// Records a change to `parent`'s child list, with `anchor` the earliest child it can affect.
    ///
    /// `anchor` is captured *before* the links are rewritten: an insertion's anchor is the node
    /// being inserted, and a removal's is the element that followed the node being removed —
    /// nothing at all if it was the last, because there is then no later sibling to restyle.
    ///
    /// An anchor must be an **element**. Positions among siblings, sibling combinators and the edge
    /// selectors are all counted over element children alone, so nothing else has a position to be
    /// compared by or a later sibling to expand into — and a non-element handed in here would
    /// compare as the earliest child of every parent and then expand to nothing at all, silently
    /// swallowing the suffix a real anchor recorded beside it. A change to a child list that moves
    /// no element records its emptiness through [`StructureLog::record_emptiness_change`] instead.
    ///
    /// Returns whether anything was recorded. Nothing is when no selector in the document depends
    /// on this parent's child list, which is the common case and costs one atomic load.
    ///
    /// # Panics
    ///
    /// Panics if `parent` names no live node of `store`.
    pub(crate) fn record_change(
        &mut self,
        store: &DocumentStore,
        parent: NodeIndex,
        anchor: Option<NodeIndex>,
    ) -> bool {
        let flags = store.core(parent).selector_flags();
        if flags.is_empty() {
            return false;
        }
        debug_assert!(
            anchor.is_none_or(|anchor| store.core(anchor).kind().in_element_chain()),
            "an anchor is compared by its position among element siblings and expanded along the \
             element chain, so only an element can be one"
        );
        let entry = self.entry(parent);
        entry.flags |= flags;
        if let Some(anchor) = anchor {
            entry.push_anchor(anchor);
        }
        // Written by the first *structural* record only, and read only then, so a batch of a
        // thousand changes under one parent walks the child list's tail once. An entry created by a
        // text edit leaves the field unset on purpose: a text edit moves no element child, so the
        // pair is still the pre-batch one when the first real structural change arrives.
        if entry.edge_before.is_none() {
            let edge = (
                OptIndex::from_option(store.core(parent).first_element_child()),
                OptIndex::from_option(crate::node::links::last_element_child(store, parent)),
            );
            self.entry(parent).edge_before = Some(edge);
        }
        true
    }

    /// Records that `parent`'s own `:empty` match may have changed, without any child moving.
    ///
    /// This is the whole of a text edit's structural obligation: a text node is not an element, so
    /// no positional selector, no sibling combinator and no edge selector can see it, and the only
    /// flag that can matter is the one the parent carries about itself. No anchor is pushed, no
    /// edge pair is written and no position is invalidated.
    ///
    /// Returns whether anything was recorded.
    ///
    /// # Panics
    ///
    /// Panics if `parent` names no live node of `store`.
    pub(crate) fn record_emptiness_change(
        &mut self,
        store: &DocumentStore,
        parent: NodeIndex,
    ) -> bool {
        let flags = store.core(parent).selector_flags() & ElementSelectorFlags::HAS_EMPTY_SELECTOR;
        if flags.is_empty() {
            return false;
        }
        self.entry(parent).flags |= flags;
        true
    }

    /// Expands every entry into the restyles it implies, and empties the log.
    ///
    /// Run once, when the outermost batch closes, because that is the first moment at which two
    /// anchors recorded by two different changes are comparable at all.
    pub(crate) fn close(&mut self, store: &mut DocumentStore, hints: &mut HintLog) {
        for entry in core::mem::take(&mut self.entries) {
            expand(store, hints, &entry);
        }
    }
}

/// Turns one parent's entry into restyles.
fn expand(store: &mut DocumentStore, hints: &mut HintLog, entry: &Entry) {
    let parent = entry.parent;
    if store.try_core(parent).is_none() {
        return;
    }
    let flags = entry.flags;

    if flags.contains(ElementSelectorFlags::HAS_EMPTY_SELECTOR) {
        hints.restyle_self(store, parent);
    }

    if flags.contains(ElementSelectorFlags::HAS_SLOW_SELECTOR) {
        restyle_every_child(store, hints, parent);
    } else if flags.contains(ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS) {
        // Whichever surviving anchor is earliest names the earliest suffix any single change could
        // have produced, and the union of suffixes is the earliest of them. Losing every anchor to
        // the liveness filter means the change that recorded it was undone or moved by a later one
        // in the same batch, and the conservative answer is the whole child list.
        match ordinals::earliest_surviving(store, parent, &entry.anchors) {
            Some(anchor) => restyle_from(store, hints, anchor),
            None => restyle_every_child(store, hints, parent),
        }
    }

    if flags.contains(ElementSelectorFlags::HAS_EDGE_CHILD_SELECTOR) {
        if let Some((first, last)) = entry.edge_before {
            for edge in [first.get(), last.get()].into_iter().flatten() {
                if store
                    .try_core(edge)
                    .is_some_and(|record| record.parent() == Some(parent))
                {
                    hints.restyle_self(store, edge);
                }
            }
        }
        let now = [
            store.core(parent).first_element_child(),
            crate::node::links::last_element_child(store, parent),
        ];
        for edge in now.into_iter().flatten() {
            hints.restyle_self(store, edge);
        }
    }
}

/// Restyles every element child of `parent`.
fn restyle_every_child(store: &mut DocumentStore, hints: &mut HintLog, parent: NodeIndex) {
    let mut current = store.core(parent).first_element_child();
    while let Some(child) = current {
        current = store.core(child).next_element();
        hints.restyle_self(store, child);
    }
}

/// Restyles `from` and every element sibling after it.
fn restyle_from(store: &mut DocumentStore, hints: &mut HintLog, from: NodeIndex) {
    let mut current = Some(from);
    while let Some(child) = current {
        current = store.core(child).next_element();
        hints.restyle_self(store, child);
    }
}

#[cfg(test)]
mod tests {
    use selectors::matching::ElementSelectorFlags;
    use zgui_bits::Dirty;
    use zgui_interned::ElementName;

    use super::StructureLog;
    use crate::arena::document::Document;
    use crate::id::node_key::NodeIndex;
    use crate::mutate::hints::HintLog;
    use crate::node::kind::NodeKind;

    /// A parent with `width` element children, and the flags the matcher left on it.
    fn row(width: usize, flags: ElementSelectorFlags) -> (Document, NodeIndex, Vec<NodeIndex>) {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let children = (0..width)
            .map(|_| document.append(root, NodeKind::Element, ElementName::new("row")))
            .collect();
        document.store().core(root).insert_selector_flags(flags);
        (document, root, children)
    }

    /// Which nodes the log's expansion marked for restyle.
    fn restyled(document: &Document, candidates: &[NodeIndex]) -> Vec<NodeIndex> {
        candidates
            .iter()
            .copied()
            .filter(|index| {
                document
                    .store()
                    .core(*index)
                    .dirty()
                    .own()
                    .contains(Dirty::RESTYLE)
            })
            .collect()
    }

    #[test]
    fn a_parent_no_selector_depends_on_records_nothing_at_all() {
        let (document, root, children) = row(4, ElementSelectorFlags::empty());
        let mut log = StructureLog::new();
        assert!(!log.record_change(document.store(), root, Some(children[0])));
        assert!(log.is_empty());
    }

    #[test]
    fn the_later_siblings_flag_restyles_the_suffix_from_the_earliest_anchor() {
        let (mut document, root, children) =
            row(6, ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS);
        let mut log = StructureLog::new();
        let mut hints = HintLog::new();
        log.record_change(document.store(), root, Some(children[4]));
        log.record_change(document.store(), root, Some(children[2]));
        log.close(document.store_mut(), &mut hints);

        assert_eq!(restyled(&document, &children), children[2..].to_vec());
    }

    #[test]
    fn an_anchor_whose_node_left_the_parent_is_filtered_out_before_it_is_asked_about() {
        let (mut document, root, children) =
            row(6, ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS);
        let mut log = StructureLog::new();
        let mut hints = HintLog::new();
        log.record_change(document.store(), root, Some(children[3]));
        log.record_change(document.store(), root, Some(children[1]));
        // The second change unlinks exactly what the first recorded, which is what two adjacent
        // removals in one batch do.
        crate::node::links::unlink(document.store(), children[1]);
        log.close(document.store_mut(), &mut hints);

        assert_eq!(restyled(&document, &children), children[3..].to_vec());
    }

    #[test]
    fn a_fifth_anchor_gives_up_and_takes_the_whole_child_list() {
        let (mut document, root, children) =
            row(8, ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS);
        let mut log = StructureLog::new();
        let mut hints = HintLog::new();
        for index in [7, 6, 5, 4, 3] {
            log.record_change(document.store(), root, Some(children[index]));
        }
        log.close(document.store_mut(), &mut hints);
        assert_eq!(restyled(&document, &children), children);
    }

    #[test]
    fn the_edge_flag_reaches_the_child_that_was_first_before_the_batch() {
        let (mut document, root, children) = row(3, ElementSelectorFlags::HAS_EDGE_CHILD_SELECTOR);
        let mut log = StructureLog::new();
        let mut hints = HintLog::new();

        let fresh = document.detached(NodeKind::Element, ElementName::new("row"));
        log.record_change(document.store(), root, Some(fresh));
        crate::node::links::link_before(document.store(), root, fresh, Some(children[0]));
        log.close(document.store_mut(), &mut hints);

        assert!(
            restyled(&document, &children).contains(&children[0]),
            "the old first child is neither the new first nor the last, so only the stored \
             pre-batch pair reaches it"
        );
    }

    #[test]
    fn a_text_edit_records_only_the_parents_own_emptiness() {
        let (mut document, root, children) = row(
            2,
            ElementSelectorFlags::HAS_EMPTY_SELECTOR
                | ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS,
        );
        let mut log = StructureLog::new();
        let mut hints = HintLog::new();
        assert!(log.record_emptiness_change(document.store(), root));
        log.close(document.store_mut(), &mut hints);

        assert!(
            document
                .store()
                .core(root)
                .dirty()
                .own()
                .contains(Dirty::RESTYLE)
        );
        assert!(
            restyled(&document, &children).is_empty(),
            "a text node moves no element sibling, so no positional selector can have changed"
        );
    }
}
