//! What an element looked like before it was changed.
//!
//! The style engine works out what a change can affect by comparing the element as it is now with a
//! record of the element as it was. Without such a record it has nothing to compare against, so it
//! re-matches the changed element and *only* the changed element — and every selector that reached
//! the element sideways, through a sibling combinator or a descendant combinator, keeps the answer
//! it had before. The symptom is that `.item:hover + .label` never lights up, and nothing anywhere
//! reports a problem.
//!
//! # Two records, not one, and why the cheaper one is the default
//!
//! A state change — hover, focus, checked — is invalidated from the state word alone: the engine
//! needs to know which bits differ, and the element's classes have not moved. So the record a state
//! write takes holds the previous state and no attributes at all, which is what keeps hovering a
//! row of a large table from copying that row's class list.
//!
//! A class, identifier or attribute change needs the previous *values*, so the record is widened to
//! carry them. Widening happens inside the write that needs it, and it is correct to do it late: a
//! record taken earlier in the same batch by a state write has not touched the attributes, so the
//! values still in the document at the moment of widening are still the pre-batch ones.
//!
//! # One record per element per batch
//!
//! The record describes the element as the last restyle left it, so the *first* change in a batch
//! is the one worth recording and every later change to the same element must leave it alone.
//! Whether a record already exists is asked of the element's own bookkeeping word rather than of
//! the map, because the answer is needed on every mutation and the map is keyed by an opaque
//! identity the engine chooses.

use style::attr::{AttrIdentifier, AttrValue};
use style::dom::TNode;
use style::selector_parser::SnapshotMap;
use style::servo::selector_parser::ServoElementSnapshot;
use stylo_atoms::Atom;

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;
use crate::node::atomics;

/// The pre-mutation records the next restyle will consume.
///
/// Empty between restyles: the traversal reads every record it needs and the tail of the restyle
/// clears the set, because a record that outlives the change it describes would make the *next*
/// change compare against the wrong past.
pub struct SnapshotStore {
    /// The records, keyed the way the style engine keys them.
    map: SnapshotMap,
    /// Which elements carry a bookkeeping bit this set has to clear when it is emptied.
    flagged: Vec<NodeIndex>,
}

impl Default for SnapshotStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotStore {
    /// An empty set.
    pub fn new() -> Self {
        Self {
            map: SnapshotMap::new(),
            flagged: Vec::new(),
        }
    }

    /// The records, as the style engine's traversal context wants them.
    pub fn map(&self) -> &SnapshotMap {
        &self.map
    }

    /// The record taken for `index`, if one was.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of `store`.
    pub fn of(&self, store: &DocumentStore, index: NodeIndex) -> Option<&ServoElementSnapshot> {
        self.map
            .get(&crate::node::handle::Node::new(store.core(index)))
    }

    /// How many elements have a record.
    pub fn len(&self) -> usize {
        self.flagged.len()
    }

    /// Whether nothing has been recorded.
    pub fn is_empty(&self) -> bool {
        self.flagged.is_empty()
    }

    /// Drops the records of elements that are about to be dropped themselves.
    ///
    /// A record is keyed by the element's *address*, and an arena slot that is returned is handed
    /// out again at the address it always had — so a record left behind by an element that is gone
    /// is found for whichever element moves into its place, and that element is then invalidated
    /// against a past that is not its own. Nothing about that fails loudly: the wrong element is
    /// compared against the wrong previous state and keeps a style that no rule justifies.
    ///
    /// Elements in `gone` that carry no record cost nothing, and an element named twice is handled
    /// once.
    pub(crate) fn forget(&mut self, store: &DocumentStore, gone: &[NodeIndex]) {
        if self.flagged.is_empty() {
            return;
        }
        let gone: rustc_hash::FxHashSet<NodeIndex> = gone.iter().copied().collect();
        let mut flagged = core::mem::take(&mut self.flagged);
        flagged.retain(|index| {
            if !gone.contains(index) {
                return true;
            }
            if let Some(record) = store.try_core(*index) {
                self.map
                    .remove(&TNode::opaque(&crate::node::handle::Node::new(record)));
            }
            false
        });
        self.flagged = flagged;
    }

    /// Drops every record and clears the bookkeeping bit on every element that carried one.
    ///
    /// Owed by whatever ran the restyle that consumed them, immediately after it ran. An element
    /// whose node has been dropped since is skipped rather than resolved.
    pub fn clear(&mut self, store: &DocumentStore) {
        for index in self.flagged.drain(..) {
            if let Some(record) = store.try_core(index) {
                record.clear_atomic(atomics::HAS_SNAPSHOT | atomics::SNAPSHOT_HANDLED);
            }
        }
        self.map.clear();
    }

    /// Records `index`'s interaction state, if it has no record yet.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of `store`.
    pub(crate) fn record_state(&mut self, store: &DocumentStore, index: NodeIndex) {
        self.entry(store, index);
    }

    /// Records `index`'s state and, additionally, its attribute values as they are now.
    ///
    /// The values are read at the moment of the call rather than when the record was created, which
    /// is what lets a state write earlier in the same batch take the cheap record and a class write
    /// later in it still see the right previous classes.
    ///
    /// # Panics
    ///
    /// Panics if `index` names no live node of `store`.
    pub(crate) fn record_attributes(&mut self, store: &DocumentStore, index: NodeIndex) {
        let identity = self.entry(store, index);
        let attrs = collect_attributes(store, index);
        let record = self
            .map
            .get_mut(&identity)
            .expect("the record was just created or already present");
        if record.attrs.is_none() {
            record.attrs = Some(attrs);
        }
    }

    /// Notes that `index`'s class list is what changed.
    pub(crate) fn note_class_changed(&mut self, store: &DocumentStore, index: NodeIndex) {
        self.note(store, index, |record| {
            record.class_changed = true;
            push_changed(record, local_name("class"));
        });
    }

    /// Notes that `index`'s identifier is what changed.
    pub(crate) fn note_id_changed(&mut self, store: &DocumentStore, index: NodeIndex) {
        self.note(store, index, |record| {
            record.id_changed = true;
            push_changed(record, local_name("id"));
        });
    }

    /// Notes that the attribute called `name` on `index` is what changed.
    pub(crate) fn note_attr_changed(
        &mut self,
        store: &DocumentStore,
        index: NodeIndex,
        name: &str,
    ) {
        self.note(store, index, |record| {
            record.other_attributes_changed = true;
            push_changed(record, local_name(name));
        });
    }

    /// Applies `edit` to `index`'s record.
    fn note(
        &mut self,
        store: &DocumentStore,
        index: NodeIndex,
        edit: impl FnOnce(&mut ServoElementSnapshot),
    ) {
        let identity = identity_of(store, index);
        if let Some(record) = self.map.get_mut(&identity) {
            edit(record);
        }
    }

    /// The record for `index`, creating one if there is none, and returns its key.
    ///
    /// The record always carries the element's attributes, even when only its state changed.
    /// Installing a style sheet makes the engine ask an existing record for its classes, and it
    /// reads them straight out of this list without asking whether the list is there — so a record
    /// with no attributes ends that call in a panic. Which is to say: an application that installs
    /// a component's sheet while a pointer rests on anything would stop, and the fault would look
    /// like it belonged to whichever component happened to be first.
    fn entry(&mut self, store: &DocumentStore, index: NodeIndex) -> style::dom::OpaqueNode {
        let identity = identity_of(store, index);
        let record = store.core(index);
        if record.has_atomic(atomics::HAS_SNAPSHOT) {
            return identity;
        }
        let attrs = collect_attributes(store, index);
        self.map.insert(
            identity,
            ServoElementSnapshot {
                state: Some(record.state()),
                attrs: Some(attrs),
                changed_attrs: Vec::new(),
                class_changed: false,
                id_changed: false,
                other_attributes_changed: false,
            },
        );
        record.set_atomic(atomics::HAS_SNAPSHOT);
        self.flagged.push(index);
        identity
    }
}

/// The identity the style engine keys a record by.
fn identity_of(store: &DocumentStore, index: NodeIndex) -> style::dom::OpaqueNode {
    TNode::opaque(&crate::node::handle::Node::new(store.core(index)))
}

/// A name in no namespace, as the style engine spells one.
fn local_name(name: &str) -> style::LocalName {
    style::values::GenericAtomIdent(web_atoms::LocalName::from(name))
}

/// Records that `name` is one of the attributes that changed, without repeating it.
fn push_changed(record: &mut ServoElementSnapshot, name: style::LocalName) {
    if !record.changed_attrs.contains(&name) {
        record.changed_attrs.push(name);
    }
}

/// One attribute of the record, in no namespace and with no prefix.
fn attribute(name: &str, value: AttrValue) -> (AttrIdentifier, AttrValue) {
    (
        AttrIdentifier {
            local_name: local_name(name),
            name: local_name(name),
            namespace: style::values::GenericAtomIdent(web_atoms::Namespace::from("")),
            prefix: None,
        },
        value,
    )
}

/// Every attribute of `index` in the form a record holds them, `class` and `id` included.
///
/// `class` and `id` are stored in the node record rather than in the attribute table, so they are
/// rebuilt here: the style engine asks a record for them by name, and a record that omitted them
/// would report that an element with classes had none.
fn collect_attributes(store: &DocumentStore, index: NodeIndex) -> Vec<(AttrIdentifier, AttrValue)> {
    let record = store.core(index);
    let mut attrs = Vec::new();
    let classes: Vec<Atom> = store
        .classes_of(index)
        .iter()
        .map(|class| class.0.clone())
        .collect();
    attrs.push(attribute("class", AttrValue::from(classes)));
    if let Some(id) = record.id_attr()
        && let Some(atom) = store.idents().resolve(id)
    {
        attrs.push(attribute("id", AttrValue::Atom(atom.clone())));
    }
    for attr in crate::node::handle::Node::new(record).attrs() {
        attrs.push(attribute(
            attr.name.as_str(),
            AttrValue::String(attr.value.as_str().to_owned()),
        ));
    }
    attrs
}

#[cfg(test)]
mod tests {
    use style::invalidation::element::element_wrapper::ElementSnapshot;
    use stylo_dom::ElementState;
    use zgui_interned::{ClassName, ElementName, Ident};

    use super::SnapshotStore;
    use crate::arena::document::Document;
    use crate::node::atomics;
    use crate::node::kind::NodeKind;

    /// A document with one classed element, and that element.
    fn one_element() -> (Document, crate::id::node_key::NodeIndex) {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        document.set_classes(root, &[ClassName::new("btn")]);
        document.set_id(root, Some(Ident::new("save")));
        (document, root)
    }

    /// A state-only record still carries the attributes, and the copy is the price of not crashing.
    ///
    /// This record used to hold state alone, so that a hover cost no class list. It cannot: the
    /// engine reads an existing record's classes straight out of the list, without asking whether
    /// the list is there, so a record taken while a pointer rested on something and read when a
    /// sheet was installed ended that call in a panic.
    #[test]
    fn a_state_record_carries_the_elements_attributes() {
        let (document, root) = one_element();
        document.store().core(root).set_state(ElementState::HOVER);
        let mut snapshots = SnapshotStore::new();
        snapshots.record_state(document.store(), root);

        let record = snapshots
            .of(document.store(), root)
            .expect("a record was taken");
        assert_eq!(record.state(), Some(ElementState::HOVER));
        assert!(
            record.has_attrs(),
            "a record the engine may read classes from has to carry them"
        );
        assert_eq!(
            record.id_attr().map(ToString::to_string),
            Some("save".to_owned()),
            "the attributes carried are the element's own"
        );
    }

    #[test]
    fn widening_after_a_state_record_still_sees_the_pre_change_classes() {
        let (mut document, root) = one_element();
        let mut snapshots = SnapshotStore::new();
        snapshots.record_state(document.store(), root);

        // The class write's own snapshot step, taken before the classes are replaced.
        snapshots.record_attributes(document.store(), root);
        snapshots.note_class_changed(document.store(), root);
        document.set_classes(root, &[ClassName::new("btn"), ClassName::new("primary")]);

        let record = snapshots
            .of(document.store(), root)
            .expect("a record was taken");
        let mut classes = Vec::new();
        record.each_class(|class| classes.push(class.to_string()));
        assert_eq!(classes, vec!["btn".to_string()]);
        assert!(record.class_changed());
        assert_eq!(
            record.id_attr().map(ToString::to_string).as_deref(),
            Some("save")
        );
    }

    #[test]
    fn a_second_change_to_the_same_element_does_not_replace_the_record() {
        let (document, root) = one_element();
        document.store().core(root).set_state(ElementState::empty());
        let mut snapshots = SnapshotStore::new();
        snapshots.record_state(document.store(), root);
        document.store().core(root).set_state(ElementState::HOVER);
        snapshots.record_state(document.store(), root);

        let record = snapshots
            .of(document.store(), root)
            .expect("a record was taken");
        assert_eq!(
            record.state(),
            Some(ElementState::empty()),
            "the record describes the element as the last restyle left it, not as the last write \
             left it"
        );
        assert_eq!(snapshots.len(), 1);
    }

    #[test]
    fn clearing_drops_the_records_and_the_bit_that_says_one_exists() {
        let (document, root) = one_element();
        let mut snapshots = SnapshotStore::new();
        snapshots.record_state(document.store(), root);
        assert!(
            document
                .store()
                .core(root)
                .has_atomic(atomics::HAS_SNAPSHOT)
        );

        snapshots.clear(document.store());
        assert!(snapshots.is_empty());
        assert!(
            !document
                .store()
                .core(root)
                .has_atomic(atomics::HAS_SNAPSHOT)
        );
    }
}
