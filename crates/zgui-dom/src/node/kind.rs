//! What a node is.

use crate::plain_data;

/// What a node is.
///
/// Only [`NodeKind::Element`] takes part in selector matching. The other three exist so that the
/// element-only sibling chain has something to skip, and skipping them is not an optimisation:
/// a text node between two elements must not shift either one's position among its element
/// siblings, or `:nth-child` and `+` both answer wrongly.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum NodeKind {
    /// The document node, parent of the root element and of nothing else.
    Document,
    /// An element: the only kind selectors match against.
    Element,
    /// A run of text.
    Text,
    /// A positional insertion point with no presence of its own.
    ///
    /// A marker holds a place in the child list so that content which appears and disappears can
    /// be re-inserted where it was. It is deliberately **not** an element and deliberately not
    /// described as a hidden one: `display` does not affect selector matching, so an element-like
    /// marker would shift `:nth-child` for every site in the document that shows content
    /// conditionally.
    Marker,
}

impl NodeKind {
    /// Whether a node of this kind takes part in selector matching.
    pub const fn is_element(self) -> bool {
        matches!(self, Self::Element)
    }

    /// Whether a node of this kind appears in the element-only sibling chain.
    ///
    /// The same answer as [`NodeKind::is_element`], written separately because the two questions
    /// are asked for different reasons and only stay the same answer by design.
    pub const fn in_element_chain(self) -> bool {
        matches!(self, Self::Element)
    }
}

plain_data!(NodeKind);

#[cfg(test)]
mod tests {
    use super::NodeKind;

    #[test]
    fn only_elements_match_selectors_and_only_elements_are_siblings() {
        for kind in [NodeKind::Document, NodeKind::Text, NodeKind::Marker] {
            assert!(!kind.is_element());
            assert!(!kind.in_element_chain());
        }
        assert!(NodeKind::Element.is_element());
        assert!(NodeKind::Element.in_element_chain());
    }

    #[test]
    fn the_kind_is_one_byte() {
        assert_eq!(size_of::<NodeKind>(), 1);
    }
}
