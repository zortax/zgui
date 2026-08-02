//! The structural selectors: emptiness and root-ness.
//!
//! `:nth-child`, `:first-child` and their relatives are counted by the matcher itself, by stepping
//! along the element-only sibling chain, so nothing here has to number anything. What the matcher
//! cannot derive is the two facts below.
//!
//! **`:empty` counts children of every kind, and weighs them differently.** An element is empty
//! when it has no element child and no text child holding anything — so a marker or a text node
//! holding the empty string leaves it empty, and one character of text does not. That asymmetry is
//! the whole point of the selector: a placeholder that appears when a container has nothing in it
//! must disappear the moment text is put there, and reappear when the text is emptied again, and
//! neither of those moves an element child.
//!
//! **Root-ness is a fact about the parent, not about a flag.** An element is the root when its
//! parent is the document node itself. Reading it from the link rather than from a stored bit is
//! what keeps it correct across a reparent, which changes the answer without touching the element.
//!
//! Unreachable on a node that is not an element, and the assertions say so.

use crate::node::handle::Node;
use crate::node::kind::NodeKind;
use crate::stylo::selectors::expect_element;

impl Node<'_> {
    /// Whether this element has no element child and no text child of non-zero length.
    ///
    /// A marker holds a place in the child list and is neither, so it leaves an element empty; a
    /// text node holding the empty string does too, which is what an interface writes when it
    /// clears a label.
    pub fn matches_empty(self) -> bool {
        expect_element(self);
        let store = self.store();
        let mut current = self.record().first_child();
        while let Some(index) = current {
            let child = store.core(index);
            match child.kind() {
                NodeKind::Element => return false,
                NodeKind::Text
                    if !crate::text::node::text_of(store, index).is_none_or(str::is_empty) =>
                {
                    return false;
                }
                _ => {}
            }
            current = child.next_sibling();
        }
        true
    }

    /// Whether this element is its document's root element.
    pub fn matches_root(self) -> bool {
        expect_element(self);
        self.parent_node_handle()
            .is_some_and(|parent| parent.kind() == NodeKind::Document)
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn text_makes_an_element_non_empty_but_only_once_it_holds_something() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let holder = document.append(root, NodeKind::Element, ElementName::new("box"));
        assert!(document.node(holder).matches_empty());

        let text = document.append(holder, NodeKind::Text, ElementName::new("#text"));
        assert!(
            document.node(holder).matches_empty(),
            "a text child holding nothing is what an interface leaves behind when it clears a \
             label, and it must not keep the placeholder away"
        );

        crate::text::node::set_text(document.store_mut(), text, "Saved");
        assert!(
            !document.node(holder).matches_empty(),
            "a placeholder rule keyed on `:empty` has to stop applying once there is text"
        );

        crate::text::node::set_text(document.store_mut(), text, "");
        assert!(document.node(holder).matches_empty());
    }

    #[test]
    fn a_marker_holds_a_place_without_filling_the_element() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let holder = document.append(root, NodeKind::Element, ElementName::new("box"));
        document.append(holder, NodeKind::Marker, ElementName::new("#marker"));
        assert!(document.node(holder).matches_empty());

        document.append(holder, NodeKind::Element, ElementName::new("box"));
        assert!(!document.node(holder).matches_empty());
    }

    #[test]
    fn only_the_document_nodes_own_element_child_is_the_root() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let child = document.append(root, NodeKind::Element, ElementName::new("box"));
        assert!(document.node(root).matches_root());
        assert!(!document.node(child).matches_root());
    }
}
