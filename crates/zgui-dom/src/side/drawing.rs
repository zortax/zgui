//! The outlines an element draws, read off the properties they are carried in.
//!
//! There is no column of its own for these. A drawing arrives as two imperative properties, and a
//! second copy kept beside them would be a second opinion about what an element draws — so this
//! module is a reader over the property map rather than storage, and the three stages that need to
//! know (the box tree, the paint stage and anything measuring a drawing) all ask the same question
//! of the same bytes.

use zgui_vocab::{PropKey, PropValue, prop::drawing};

use crate::arena::store::DocumentStore;
use crate::id::node_key::NodeKey;

/// The outlines `node` draws, in path notation, one per line.
///
/// Nothing for an element that carries none, and nothing for one carrying an empty string — an
/// empty drawing and no drawing are the same drawing, and treating them differently would make a
/// view that cleared its paths produce a fragment that draws nothing rather than no fragment.
pub fn path_data(store: &DocumentStore, node: NodeKey) -> Option<&str> {
    match store
        .columns()
        .props
        .get(node)?
        .get(PropKey::new(drawing::PATHS))
    {
        Some(PropValue::Text(data)) if !data.trim().is_empty() => Some(data.as_str()),
        _ => None,
    }
}

/// The vector document `node` draws, as its source text.
///
/// Nothing for an element carrying none, and nothing for one carrying only whitespace — an empty
/// document and no document are the same document.
pub fn document(store: &DocumentStore, node: NodeKey) -> Option<&str> {
    match store
        .columns()
        .props
        .get(node)?
        .get(PropKey::new(drawing::DOCUMENT))
    {
        Some(PropValue::Text(source)) if !source.trim().is_empty() => Some(source.as_str()),
        _ => None,
    }
}

/// The space `node`'s outlines are written in, as minimum x, minimum y, width and height.
///
/// Nothing for an element drawing in its own box's coordinates, and nothing for one whose view box
/// does not read as four finite numbers with a positive extent.
pub fn view_box(store: &DocumentStore, node: NodeKey) -> Option<[f32; 4]> {
    match store
        .columns()
        .props
        .get(node)?
        .get(PropKey::new(drawing::VIEW_BOX))
    {
        Some(PropValue::Text(text)) => drawing::view_box(text),
        _ => None,
    }
}

/// Whether `node` draws any outlines at all.
///
/// This is what decides that an element's box produces a drawing rather than a plain box, so it is
/// deliberately the same test [`path_data`] and [`document`] answer between them: a box built from
/// one answer and painted from the other would be a piece of geometry nothing draws into.
pub fn draws(store: &DocumentStore, node: NodeKey) -> bool {
    path_data(store, node).is_some() || document(store, node).is_some()
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;
    use zgui_vocab::{PropKey, PropValue, prop::drawing};

    use crate::arena::document::Document;
    use crate::mutate::filter::EverythingMatters;
    use crate::node::kind::NodeKind;

    use super::{draws, path_data, view_box};

    /// A document with one element carrying the given properties.
    fn one(properties: &[(&str, &str)]) -> (Document, crate::id::node_key::NodeKey) {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("vector"),
        );
        document
            .edit(&EverythingMatters, |edit| {
                for (name, value) in properties {
                    edit.set_property(root, PropKey::new(name), Some(PropValue::from(*value)));
                }
            })
            .expect("not poisoned");
        let key = document.store().key_of(root);
        (document, key)
    }

    #[test]
    fn an_element_with_outlines_draws_and_one_without_does_not() {
        let (document, node) = one(&[(drawing::PATHS, "M0 0 L8 0 L8 8 Z")]);
        assert_eq!(path_data(document.store(), node), Some("M0 0 L8 0 L8 8 Z"));
        assert!(draws(document.store(), node));

        let (document, node) = one(&[]);
        assert_eq!(path_data(document.store(), node), None);
        assert!(!draws(document.store(), node));
    }

    /// A view that cleared its paths must stop producing a drawing rather than produce an empty one.
    #[test]
    fn an_empty_string_is_no_drawing_rather_than_an_empty_one() {
        let (document, node) = one(&[(drawing::PATHS, "   ")]);
        assert!(!draws(document.store(), node));
    }

    #[test]
    fn a_view_box_is_read_only_when_it_is_four_numbers() {
        let (document, node) = one(&[
            (drawing::PATHS, "M0 0 L8 0"),
            (drawing::VIEW_BOX, "0 0 24 24"),
        ]);
        assert_eq!(
            view_box(document.store(), node),
            Some([0.0, 0.0, 24.0, 24.0])
        );

        let (document, node) = one(&[
            (drawing::PATHS, "M0 0 L8 0"),
            (drawing::VIEW_BOX, "nonsense"),
        ]);
        assert_eq!(view_box(document.store(), node), None);
    }
}
