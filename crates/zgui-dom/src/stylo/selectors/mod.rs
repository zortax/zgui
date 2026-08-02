//! The matcher's view of an element.
//!
//! Every method here runs on a worker thread, against an element some other worker may be looking at
//! simultaneously, which is why the node record is built the way it is. A trait implementation
//! cannot be split across files, so what is here is the surface — every method one line of
//! delegation — and the answers live beside their reasons in the modules below.
//!
//! | Module | Answers |
//! |---|---|
//! | [`tree`] | identity, and the chain combinators step along |
//! | [`simple`] | names, classes, identifiers and the state pseudo-classes |
//! | [`attrs`] | attribute selectors |
//! | [`nth`] | emptiness and root-ness |
//! | [`flags`] | the one method that writes, and writes on the parent too |
//!
//! Every method that answers a *simple selector* — as opposed to stepping to another candidate —
//! passes its answer through the module that counts them, so a frame's selector-matching cost is
//! readable from the frame counters.

pub mod attrs;
pub mod flags;
pub mod nth;
pub mod simple;
mod tally;
pub mod tree;

use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::matching::{ElementSelectorFlags, MatchingContext};
use selectors::{Element as SelectorsElement, OpaqueElement};
use style::selector_parser::{NonTSPseudoClass, PseudoElement, SelectorImpl};
use style::values::{AtomIdent, AtomString};

use crate::node::handle::Node;

/// States that a handle reached through the matcher is an element.
///
/// The matcher reaches an element only through the parent link, the element-only sibling chain or
/// the matching context's roots, and each of those already filters to elements — so this can never
/// fire today. It exists so that a future change widening one of those entry points fails loudly in
/// a debug build instead of matching a text node against a class selector and quietly answering no.
pub(crate) fn expect_element(node: Node<'_>) {
    debug_assert!(
        node.kind().is_element(),
        "selector matching reached a node that is not an element: {node:?}"
    );
}

impl<'doc> SelectorsElement for Node<'doc> {
    type Impl = SelectorImpl;

    fn opaque(&self) -> OpaqueElement {
        self.opaque_identity()
    }

    fn parent_element(&self) -> Option<Self> {
        self.parent_element_handle()
    }

    /// Never: this document has no shadow trees, so no node's parent is a shadow root.
    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }

    /// Never, for the same reason.
    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }

    fn is_pseudo_element(&self) -> bool {
        false
    }

    fn prev_sibling_element(&self) -> Option<Self> {
        self.prev_element_sibling()
    }

    fn next_sibling_element(&self) -> Option<Self> {
        self.next_element_sibling()
    }

    fn first_element_child(&self) -> Option<Self> {
        self.first_element_child_handle()
    }

    /// Never: names match case-sensitively here, because there is no document language to impose a
    /// folding rule.
    fn is_html_element_in_html_document(&self) -> bool {
        false
    }

    fn has_local_name(&self, local_name: &web_atoms::LocalName) -> bool {
        tally::tested(self.matches_local_name(local_name))
    }

    fn has_namespace(&self, namespace: &web_atoms::Namespace) -> bool {
        tally::tested(self.matches_namespace(namespace))
    }

    fn is_same_type(&self, other: &Self) -> bool {
        tally::tested(self.matches_same_type(*other))
    }

    fn attr_matches(
        &self,
        namespace: &NamespaceConstraint<&style::Namespace>,
        local_name: &style::LocalName,
        operation: &AttrSelectorOperation<&AtomString>,
    ) -> bool {
        tally::tested(self.matches_attr(namespace, local_name, operation))
    }

    fn match_non_ts_pseudo_class(
        &self,
        class: &NonTSPseudoClass,
        context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        tally::tested(self.matches_pseudo_class(class, context))
    }

    fn match_pseudo_element(
        &self,
        pseudo: &PseudoElement,
        context: &mut MatchingContext<Self::Impl>,
    ) -> bool {
        tally::tested(self.matches_pseudo_element(pseudo, context))
    }

    fn apply_selector_flags(&self, flags: ElementSelectorFlags) {
        self.record_selector_flags(flags);
    }

    fn is_link(&self) -> bool {
        tally::tested(self.matches_link())
    }

    /// Never: slots belong to shadow trees, and this document has none.
    fn is_html_slot_element(&self) -> bool {
        false
    }

    fn has_id(&self, id: &AtomIdent, case_sensitivity: CaseSensitivity) -> bool {
        tally::tested(self.matches_id(id, case_sensitivity))
    }

    fn has_class(&self, name: &AtomIdent, case_sensitivity: CaseSensitivity) -> bool {
        tally::tested(self.matches_class(name, case_sensitivity))
    }

    fn has_custom_state(&self, name: &AtomIdent) -> bool {
        tally::tested(self.has_custom_state_named(name))
    }

    /// Never: parts are exported across a shadow boundary, and there are none.
    fn imported_part(&self, _name: &AtomIdent) -> Option<AtomIdent> {
        None
    }

    /// Never, for the same reason.
    fn is_part(&self, _name: &AtomIdent) -> bool {
        tally::tested(false)
    }

    fn is_empty(&self) -> bool {
        tally::tested(self.matches_empty())
    }

    fn is_root(&self) -> bool {
        tally::tested(self.matches_root())
    }

    fn add_element_unique_hashes(&self, filter: &mut BloomFilter) -> bool {
        self.add_bloom_hashes(filter)
    }
}
