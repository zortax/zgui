//! The one conversion between what a view holds and what the document holds.

use zgui_dom::{Document, NodeIndex, NodeKey};
use zgui_view::{DocumentId, NodeId};

/// The handle a view holds for `key`.
///
/// The two are the same sixty-four bits. A document's generation-checked name packs a slot number,
/// an occupancy counter and the document it belongs to; a view's handle packs the document it
/// belongs to and fifty-two bits the backend chooses. The document sits in the top twelve of both,
/// so the conversion is the identity and no table is needed anywhere to undo it.
pub fn to_view(key: NodeKey) -> NodeId {
    NodeId::from_u64(key.as_u64()).expect("a node key is never zero")
}

/// The document's name for `node`.
///
/// `None` when the bits are not a name any document ever issued — a retired slot, or a handle from
/// something that is not this backend.
pub fn to_document(node: NodeId) -> Option<NodeKey> {
    NodeKey::from_u64(node.as_u64())
}

/// The document `node` belongs to, as the view layer numbers documents.
pub fn document_of(node: NodeId) -> DocumentId {
    node.document()
}

/// Whether `node` still names a live node of `document`.
pub fn is_live(document: &Document, node: NodeId) -> bool {
    to_document(node).is_some_and(|key| document.store().index_of(key).is_some())
}

/// Resolves `node` to a live slot of `document`, panicking if it names nothing.
///
/// # Panics
///
/// Panics if `node` is not a live node of `document` — a handle from another window, or one whose
/// node has been dropped. Both are programming errors in the layer above, and a silent no-op would
/// turn either into an interface that stops updating with nothing to point at.
pub fn resolve(document: &Document, node: NodeId) -> NodeIndex {
    let key = to_document(node).expect("a view's handle always carries a document's own name");
    document
        .store()
        .index_of(key)
        .expect("the node is still in the document")
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeIndex};
    use zgui_interned::ElementName;

    use super::{document_of, resolve, to_document, to_view};

    /// An element called `name` under `parent`, built the one way this crate builds anything.
    fn element(document: &Document, parent: NodeIndex, name: &str) -> NodeIndex {
        document
            .edit(&EverythingMatters, |edit| {
                let node = edit.create_element(ElementName::new(name));
                edit.insert_before(parent, node, None);
                node
            })
            .expect("a fresh document is not poisoned")
    }

    #[test]
    fn a_key_and_a_handle_are_the_same_bits_and_name_the_same_node() {
        let document = Document::new();
        let root = element(&document, document.document_index(), "root");
        let key = document.store().key_of(root);
        let handle = to_view(key);

        assert_eq!(to_document(handle), Some(key));
        assert_eq!(resolve(&document, handle), root);
        assert_eq!(document_of(handle).get(), key.document().get());
    }

    #[test]
    fn a_handle_whose_node_is_gone_resolves_to_nothing_rather_than_to_its_replacement() {
        let mut document = Document::new();
        let root = element(&document, document.document_index(), "root");
        let doomed = element(&document, root, "box");
        let handle = to_view(document.store().key_of(doomed));

        document
            .edit(&EverythingMatters, |edit| edit.remove(doomed))
            .expect("not poisoned");
        zgui_dom::arena::end_frame(&mut document);

        let key = to_document(handle).expect("still well formed bits");
        assert_eq!(document.store().index_of(key), None);
    }
}
