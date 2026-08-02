//! Text content, and where it is kept.

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeIndex;
use crate::node::kind::NodeKind;

/// The text `index` holds, or [`None`] if it is not a text node or holds nothing.
///
/// # Panics
///
/// Panics if `index` names no live node of `store`.
pub fn text_of(store: &DocumentStore, index: NodeIndex) -> Option<&str> {
    store
        .columns()
        .text
        .get(store.key_of(index))
        .and_then(Option::as_deref)
}

/// Replaces the text `index` holds.
///
/// # Panics
///
/// Panics if `index` names no live node of the store, or if it is not a text node — text on an
/// element would be content the layout stage has no box to put and the accessibility projection no
/// place to read from, so it is a caller error rather than a silently ignored write.
pub fn set_text(store: &mut DocumentStore, index: NodeIndex, text: &str) {
    assert_eq!(
        store.core(index).kind(),
        NodeKind::Text,
        "only a text node holds text"
    );
    let key = store.key_of(index);
    *store.columns_mut().text.get_mut(key) = Some(text.into());
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;

    use super::{set_text, text_of};
    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn text_round_trips_through_the_column() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let text = document.append(root, NodeKind::Text, ElementName::new("#text"));
        assert_eq!(text_of(document.store(), text), None);

        set_text(document.store_mut(), text, "hello");
        assert_eq!(text_of(document.store(), text), Some("hello"));
        set_text(document.store_mut(), text, "goodbye");
        assert_eq!(text_of(document.store(), text), Some("goodbye"));
    }

    #[test]
    #[should_panic(expected = "only a text node holds text")]
    fn an_element_cannot_be_given_text() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        set_text(document.store_mut(), root, "hello");
    }
}
