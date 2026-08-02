//! States the author named, and the invalidation they have to bring with them.
//!
//! An interaction state is one bit of a word the style engine records across a change and compares
//! afterwards, which is what makes `:hover` invalidate exactly what it should. An author-defined
//! state gets none of that: the engine's record of an element reports unconditionally that it has
//! no author-defined states, and the whole arm that would invalidate them is gated on that report.
//! A state written here and left to the engine would therefore match once, at first match, and
//! never be re-examined — the old style would stay on the screen with nothing to notice it by.
//!
//! So the invalidation is supplied here instead, and it is deliberately wider than a snapshot
//! would be:
//!
//! * the element itself and everything below it are re-matched, which covers `:state(open)` and
//!   `:state(open) .panel`;
//! * when a sibling combinator could reach past the element — which its parent's own selector
//!   flags answer in one atomic load, and almost always answer with "no" — its later siblings are
//!   re-matched too, which covers `:state(open) + .panel` and `:state(open) ~ .panel`.
//!
//! That is correct and not minimal. It costs a subtree re-match per toggle, which is why the
//! closed set of interaction states exists and is what a control's own state should use.

use selectors::matching::ElementSelectorFlags;
use style::invalidation::element::restyle_hints::RestyleHint;
use style::values::AtomIdent;
use zgui_interned::Ident;

use crate::id::node_key::NodeIndex;
use crate::mutate::edit::Edit;

impl Edit<'_> {
    /// Turns the author-defined state `name` on or off for `node`.
    ///
    /// `name` is matched by `:state(name)` in a selector. Nothing happens if the state was already
    /// in the requested position.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn set_custom_state(&mut self, node: NodeIndex, name: Ident, on: bool) {
        let atom = AtomIdent::from(name.as_str());
        let store = self.store();
        let key = store.key_of(node);
        if !store
            .columns_mut()
            .custom_states
            .get_mut(key)
            .set(&atom, on)
        {
            return;
        }
        let (store, batch) = self.parts();
        batch
            .hints
            .record(store, node, RestyleHint::restyle_subtree());

        let Some(parent) = store.core(node).parent() else {
            return;
        };
        if !store
            .core(parent)
            .selector_flags()
            .contains(ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS)
        {
            return;
        }
        let mut sibling = store.core(node).next_element();
        while let Some(next) = sibling {
            let (store, batch) = self.parts();
            batch
                .hints
                .record(store, next, RestyleHint::restyle_subtree());
            sibling = self.store().core(next).next_element();
        }
    }
}

#[cfg(test)]
mod tests {
    use selectors::matching::ElementSelectorFlags;
    use style::invalidation::element::restyle_hints::RestyleHint;
    use style::values::AtomIdent;
    use zgui_interned::{ElementName, Ident};

    use crate::arena::document::Document;
    use crate::id::node_key::NodeIndex;
    use crate::mutate::filter::EverythingMatters;
    use crate::node::kind::NodeKind;

    /// A parent with two element children, all three styled so a hint has somewhere to land.
    fn family() -> (Document, NodeIndex, NodeIndex, NodeIndex) {
        let mut document = Document::new();
        let parent = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("row"),
        );
        let first = document.append(parent, NodeKind::Element, ElementName::new("box"));
        let second = document.append(parent, NodeKind::Element, ElementName::new("box"));
        for node in [parent, first, second] {
            document.node(node).establish_style_data();
        }
        (document, parent, first, second)
    }

    /// The hint `node` is carrying.
    fn hint(document: &Document, node: NodeIndex) -> RestyleHint {
        document
            .node(node)
            .borrow_style_data()
            .expect("the element has data")
            .hint
    }

    #[test]
    fn a_state_is_visible_to_matching_and_a_second_write_of_it_changes_nothing() {
        let (document, _, first, _) = family();
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_custom_state(first, Ident::new("peeking"), true);
            })
            .expect("not poisoned");
        assert!(
            document
                .node(first)
                .has_custom_state_named(&AtomIdent::from("peeking"))
        );
        assert!(hint(&document, first).contains(RestyleHint::restyle_subtree()));

        let mut seen = Vec::new();
        document
            .node(first)
            .each_custom_state(|name| seen.push(name.to_string()));
        assert_eq!(seen, vec!["peeking".to_owned()]);

        document
            .edit(&EverythingMatters, |edit| {
                edit.set_custom_state(first, Ident::new("peeking"), false);
            })
            .expect("not poisoned");
        assert!(
            !document
                .node(first)
                .has_custom_state_named(&AtomIdent::from("peeking"))
        );
    }

    #[test]
    fn a_later_sibling_is_restyled_only_when_a_sibling_combinator_could_reach_it() {
        let (document, _, first, second) = family();
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_custom_state(first, Ident::new("peeking"), true);
            })
            .expect("not poisoned");
        assert!(
            hint(&document, second).is_empty(),
            "no rule set has claimed a sibling combinator, so nothing later can have changed"
        );

        document
            .node(first)
            .record_selector_flags(ElementSelectorFlags::HAS_SLOW_SELECTOR_LATER_SIBLINGS);
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_custom_state(first, Ident::new("peeking"), false);
            })
            .expect("not poisoned");
        assert!(
            hint(&document, second).contains(RestyleHint::restyle_subtree()),
            "`:state(x) ~ .y` has to invalidate `.y`, and no snapshot will do it"
        );
    }
}
