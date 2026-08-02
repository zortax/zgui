//! The side tables, and which of them are worth paying for on every node.
//!
//! A column is dense when nearly every node participates in it and sparse when most nodes do not.
//! The split is not a guess. Measured on a built document, the fixed cost of a node is two hundred
//! bytes, of which seventy-two are side tables — and a *single* dense attribute column is
//! twenty-four of those, on every node, whether or not the node has an attribute at all. Twelve
//! dense columns over a hundred thousand nodes is twenty megabytes before anything is stored in
//! them, on an index space that only ever grows.
//!
//! So the rule, per column:
//!
//! | Column | Shape | Why |
//! |---|---|---|
//! | [`attrs`](Columns::attrs) | sparse | most nodes have no attributes, and a value is a string |
//! | [`inline_style`](Columns::inline_style) | sparse | most nodes have no `style` of their own |
//! | [`custom_states`](Columns::custom_states) | sparse | almost nothing carries an author-defined state |
//! | [`text`](Columns::text) | sparse | only text nodes |
//! | [`boxes`](Columns::boxes) | sparse | absent for hidden nodes, and rebuilt rather than kept |
//! | [`semantics`](Columns::semantics) | sparse | most of a document is containers |
//! | [`listeners`](Columns::listeners) | sparse | a small fraction of nodes listen for anything |
//! | [`props`](Columns::props) | sparse | only nodes with imperative properties |
//! | [`observed`](Columns::observed) | sparse | almost nothing is watched |
//! | [`anim`](Columns::anim) | sparse | live only while a cheap animation runs |
//! | [`state_mask`](Columns::state_mask) | sparse | only nodes something has written to |
//! | [`paint_key`](Columns::paint_key) | **dense** | every styled node is compared every restyle |
//! | [`a11y_key`](Columns::a11y_key) | **dense** | the same, one projection over |
//!
//! Columns are never written while the style traversal runs, which is the same frame discipline the
//! record's cells rely on. Reads during a traversal are shared reads of memory nobody is writing.

use zgui_arena::{DomainId, PagedVec, SlotVec};

use crate::id::node_key::NodeKey;
use crate::mutate::filter::StateMaskSlot;
use crate::side::a11y_key::A11yKey;
use crate::side::anim::AnimSlot;
use crate::side::attrs::AttrMap;
use crate::side::boxes::BoxList;
use crate::side::custom_state::CustomStates;
use crate::side::inline_style::StyleBlock;
use crate::side::listeners::ListenerSet;
use crate::side::observed::ObservationSlots;
use crate::side::paint_key::PaintStyleKey;
use crate::side::props::PropMap;
use crate::side::semantics::SemanticsSlot;

/// Per-node data kept out of the hot record.
pub struct Columns {
    /// Attributes other than `id` and `class`.
    pub attrs: PagedVec<NodeKey, Option<Box<AttrMap>>>,
    /// The parsed `style` attribute, shared with the style engine under the document's lock.
    pub inline_style: PagedVec<NodeKey, Option<StyleBlock>>,
    /// The author-defined states this node carries, which `:state(name)` matches.
    pub custom_states: PagedVec<NodeKey, CustomStates>,
    /// Text content, for text nodes.
    pub text: PagedVec<NodeKey, Option<Box<str>>>,
    /// The boxes this node generated, in document order.
    pub boxes: PagedVec<NodeKey, BoxList>,
    /// What this node means, for the accessibility projection.
    pub semantics: PagedVec<NodeKey, SemanticsSlot>,
    /// Which events this node listens for, and under which identities.
    pub listeners: PagedVec<NodeKey, ListenerSet>,
    /// Imperative properties, which selectors cannot see.
    pub props: PagedVec<NodeKey, PropMap>,
    /// Which of this node's measurements are watched, and what was last delivered.
    pub observed: PagedVec<NodeKey, ObservationSlots>,
    /// A cheap animation's private values, live only while one is running.
    pub anim: PagedVec<NodeKey, AnimSlot>,
    /// Which interaction-state bits any active selector could match this node on.
    ///
    /// A cache, not a fact: absent means "not computed, or dropped because the node's identity
    /// changed". It is here rather than in the record because only nodes something writes to ever
    /// get an entry, and a document is mostly nodes nothing writes to.
    pub state_mask: PagedVec<NodeKey, StateMaskSlot>,
    /// Identity of everything this node's painted appearance depends on.
    pub paint_key: SlotVec<NodeKey, PaintStyleKey>,
    /// Identity of everything this node's accessible description depends on.
    pub a11y_key: SlotVec<NodeKey, A11yKey>,
}

impl Columns {
    /// Empty columns for one document's node arena.
    ///
    /// Every table is told the domain it belongs to, so a key from another document or another of
    /// this document's arenas is caught in debug builds rather than silently reading a row that
    /// belongs to something else.
    pub fn new(domain: DomainId) -> Self {
        Self {
            attrs: PagedVec::for_domain(domain),
            inline_style: PagedVec::for_domain(domain),
            custom_states: PagedVec::for_domain(domain),
            text: PagedVec::for_domain(domain),
            boxes: PagedVec::for_domain(domain),
            semantics: PagedVec::for_domain(domain),
            listeners: PagedVec::for_domain(domain),
            props: PagedVec::for_domain(domain),
            observed: PagedVec::for_domain(domain),
            anim: PagedVec::for_domain(domain),
            state_mask: PagedVec::for_domain(domain),
            paint_key: SlotVec::for_domain(domain),
            a11y_key: SlotVec::for_domain(domain),
        }
    }

    /// How many sparse pages are allocated across every sparse column.
    ///
    /// This is what the sparse columns actually cost beyond their page indexes, and it is the
    /// number the memory budget is written against.
    pub fn allocated_pages(&self) -> usize {
        self.attrs.pages()
            + self.inline_style.pages()
            + self.custom_states.pages()
            + self.text.pages()
            + self.boxes.pages()
            + self.semantics.pages()
            + self.listeners.pages()
            + self.props.pages()
            + self.observed.pages()
            + self.anim.pages()
            + self.state_mask.pages()
    }

    /// Drops everything stored for one node, across every column.
    ///
    /// Called as the node's record is dropped, and it has to name every column: a column left out
    /// keeps its row for the lifetime of the document, because a slot number is reused but a row is
    /// only ever overwritten by a node that happens to write to the same column. Adding a column
    /// and not adding it here is a leak that nothing else notices.
    pub fn clear(&mut self, key: NodeKey) {
        self.attrs.clear(key);
        self.inline_style.clear(key);
        self.custom_states.clear(key);
        self.text.clear(key);
        self.boxes.clear(key);
        self.semantics.clear(key);
        self.listeners.clear(key);
        self.props.clear(key);
        self.observed.clear(key);
        self.anim.clear(key);
        self.state_mask.clear(key);
        self.paint_key.remove(key);
        self.a11y_key.remove(key);
    }

    /// Drops every sparse page nothing is stored on any more.
    ///
    /// Run once per frame, beside the arena's own recycling, so that the pages a churning subtree
    /// left behind do not accumulate. The columns whose values cannot be compared are compacted by
    /// an explicit emptiness test instead, which has to be the real one: dropping a page whose
    /// entries are not all empty silently resets them.
    pub fn compact(&mut self) {
        self.attrs.compact_by(Option::is_none);
        self.inline_style.compact_by(Option::is_none);
        self.custom_states.compact_by(CustomStates::is_empty);
        self.text.compact();
        self.boxes.compact_by(BoxList::is_empty);
        self.semantics.compact_by(Option::is_none);
        self.listeners.compact_by(ListenerSet::is_empty);
        self.props.compact_by(PropMap::is_empty);
        self.observed.compact();
        self.anim.compact_by(Option::is_none);
        self.state_mask.compact_by(Option::is_none);
    }
}

#[cfg(test)]
mod tests {
    use zgui_arena::{DomainId, Generation, PAGE_LEN};

    use super::Columns;
    use crate::id::node_key::NodeKey;
    use crate::side::listeners::{Listener, ListenerId};

    fn key(index: u32) -> NodeKey {
        NodeKey::new(index, Generation::FIRST, DomainId::FIRST)
    }

    #[test]
    fn a_document_nothing_has_been_written_to_has_no_pages_at_all() {
        let columns = Columns::new(DomainId::FIRST);
        assert_eq!(columns.allocated_pages(), 0);
    }

    #[test]
    fn writing_one_row_allocates_one_page_of_one_column() {
        let mut columns = Columns::new(DomainId::FIRST);
        *columns.text.get_mut(key(5)) = Some("hello".into());
        assert_eq!(columns.allocated_pages(), 1);
        assert_eq!(
            columns.text.get(key(5)).and_then(Option::as_deref),
            Some("hello")
        );
        assert_eq!(columns.text.get(key(6)), Some(&None));
    }

    #[test]
    fn one_page_covers_a_thousand_neighbouring_rows() {
        let mut columns = Columns::new(DomainId::FIRST);
        for index in 0..PAGE_LEN as u32 {
            columns.listeners.get_mut(key(index)).add(Listener {
                kind: zgui_vocab::EventKind::Click,
                options: zgui_vocab::ListenerOptions::DEFAULT,
                id: ListenerId::new(index as u64),
            });
        }
        assert_eq!(columns.allocated_pages(), 1);
    }

    #[test]
    fn compacting_returns_an_emptied_column_to_costing_nothing() {
        let mut columns = Columns::new(DomainId::FIRST);
        *columns.attrs.get_mut(key(3)) = Some(Box::default());
        assert_eq!(columns.allocated_pages(), 1);
        columns.attrs.clear(key(3));
        columns.compact();
        assert_eq!(columns.allocated_pages(), 0);
    }
}
