//! The tree the cascade walks, and the names an element is known by.
//!
//! The traversal walks the *plain* child chain: text nodes are handed to it too, because a text
//! node inherits from the element above it and something has to visit it to say so. That is the
//! difference from the chain selector matching walks, which skips them.

use crate::node::element::name::ElementName;
use crate::node::handle::Node;

/// The children the style traversal descends into, in document order.
///
/// Every child of any kind, because the traversal's job is to visit the tree and not to match
/// against it.
pub struct TraversalChildren<'doc> {
    /// The next child, if there is one.
    next: Option<Node<'doc>>,
}

impl<'doc> TraversalChildren<'doc> {
    /// The children of `parent`.
    pub fn of(parent: Node<'doc>) -> Self {
        Self {
            next: parent.first_child_node(),
        }
    }
}

impl<'doc> Iterator for TraversalChildren<'doc> {
    type Item = Node<'doc>;

    fn next(&mut self) -> Option<Node<'doc>> {
        let current = self.next?;
        self.next = current.next_sibling_node();
        Some(current)
    }
}

impl<'doc> Node<'doc> {
    /// This node's parent, of any kind.
    pub fn parent_node_handle(self) -> Option<Node<'doc>> {
        self.record().parent().map(|index| self.sibling(index))
    }

    /// This node's first child of any kind.
    pub fn first_child_node(self) -> Option<Node<'doc>> {
        self.record().first_child().map(|index| self.sibling(index))
    }

    /// This node's next sibling of any kind.
    pub fn next_sibling_node(self) -> Option<Node<'doc>> {
        self.record()
            .next_sibling()
            .map(|index| self.sibling(index))
    }

    /// This node's previous element sibling, skipping text and markers.
    pub fn prev_element_sibling(self) -> Option<Node<'doc>> {
        self.record()
            .prev_element()
            .map(|index| self.sibling(index))
    }

    /// This node's next element sibling, skipping text and markers.
    pub fn next_element_sibling(self) -> Option<Node<'doc>> {
        self.record()
            .next_element()
            .map(|index| self.sibling(index))
    }

    /// This node's first element child, skipping text and markers.
    pub fn first_element_child_handle(self) -> Option<Node<'doc>> {
        self.record()
            .first_element_child()
            .map(|index| self.sibling(index))
    }

    /// This element's tag name, borrowed from the record rather than rebuilt.
    ///
    /// Selector matching compares the name once per candidate, so handing back a reference into the
    /// record is the difference between a pointer comparison and constructing an interned string on
    /// every test.
    pub fn tag_name(self) -> &'doc ElementName {
        self.record().local_name()
    }

    /// This element's namespace.
    pub fn namespace_uri(self) -> &'doc web_atoms::Namespace {
        self.store().namespace(self.record().namespace_id())
    }
}
