//! The one conversion between a document's name for a node and an accessibility tree's.

use zgui_dom::NodeKey;

/// The identity a node is addressed by in an accessibility tree.
///
/// Re-exported so that a consumer of this crate can name what it is handed without naming the
/// interchange crate itself.
pub use accesskit::NodeId;

/// The accessibility tree's name for `key`.
///
/// The two are the same sixty-four bits, so relating one node to another needs no table and no
/// second identity that could go stale. A document's name already carries a generation counter,
/// which is what makes a stale identifier detectably stale rather than a silent alias for whatever
/// took the slot over.
///
/// ```
/// use zgui_dom::{Document, NodeKind};
/// use zgui_interned::ElementName;
///
/// let mut document = Document::new();
/// let root = document.append(
///     document.document_index(),
///     NodeKind::Element,
///     ElementName::new("root"),
/// );
/// let key = document.store().key_of(root);
///
/// let id = zgui_a11y::to_a11y(key);
/// assert_eq!(zgui_a11y::to_document(id), Some(key));
/// ```
pub fn to_a11y(key: NodeKey) -> NodeId {
    NodeId(key.as_u64())
}

/// The document's name for `id`, when the bits are a name a document ever issued.
///
/// `None` for an identifier that is not one — a zero, or a number an assistive technology invented.
pub fn to_document(id: NodeId) -> Option<NodeKey> {
    NodeKey::from_u64(id.0)
}

#[cfg(test)]
mod tests {
    use accesskit::NodeId;

    use super::{to_a11y, to_document};

    #[test]
    fn an_identifier_no_document_issued_resolves_to_nothing() {
        assert_eq!(to_document(NodeId(0)), None);
    }

    #[test]
    fn every_key_round_trips() {
        let mut document = zgui_dom::Document::new();
        let root = document.append(
            document.document_index(),
            zgui_dom::NodeKind::Element,
            zgui_interned::ElementName::new("root"),
        );
        let key = document.store().key_of(root);
        assert_eq!(to_document(to_a11y(key)), Some(key));
    }
}
