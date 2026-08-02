//! Which element a text node takes its style from.
//!
//! A text node has no style of its own. It is not an element, no selector matches it, and the
//! cascade never visits it — yet the font, the colour and the line height it is drawn with all have
//! to come from somewhere. They come from the nearest element at or above it, which is what CSS
//! means by an anonymous box inheriting from its parent.
//!
//! The walk skips anything that is not an element, so a text node inside a positional marker still
//! finds the element that encloses both.

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;

/// The nearest element at or above `node`, which is the element its style is inherited from.
///
/// Returns [`None`] only for a node with no element above it at all, which is the document node and
/// anything hanging directly off it.
///
/// # Panics
///
/// Panics if `node` names no live node of `store`.
pub fn inherits_from(store: &DocumentStore, node: NodeIndex) -> Option<NodeIndex> {
    let mut current = Some(node);
    while let Some(index) = current {
        let record = store.core(index);
        if record.kind().is_element() {
            return Some(index);
        }
        current = record.parent();
    }
    None
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use super::inherits_from;
    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn a_text_node_inherits_from_the_element_that_contains_it() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let text = document.append(root, NodeKind::Text, ElementName::new("#text"));
        assert_eq!(inherits_from(document.store(), text), Some(root));
    }

    #[test]
    fn the_walk_skips_a_marker_on_the_way_up() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let marker = document.append(root, NodeKind::Marker, ElementName::new("#marker"));
        let text = document.append(marker, NodeKind::Text, ElementName::new("#text"));
        assert_eq!(inherits_from(document.store(), text), Some(root));
    }

    #[test]
    fn an_element_inherits_from_itself() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        assert_eq!(inherits_from(document.store(), root), Some(root));
    }

    #[test]
    fn nothing_above_the_document_node_is_an_element() {
        let document = Document::new();
        assert_eq!(
            inherits_from(document.store(), document.document_index()),
            None
        );
    }
}
