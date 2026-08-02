//! The name an element gets from the text inside it.
//!
//! An element that declared no label of its own is named by its own text, exactly as
//! `aria-labelledby` pointing at a paragraph is: the text *is* the name. Without this, relating a
//! field to the element that names it produces a field with no name at all — a consumer reading an
//! explicit relation asks the target for its label and gets nothing, because the target is a box
//! and the words are one level further down.
//!
//! Only the element's **own** text children take part. A name assembled from a whole subtree would
//! make one deep element pay for every text node beneath it, and the invalidation key this
//! projection is scheduled by hashes exactly these children and no others — so widening the rule
//! here would silently stop the name from being rebuilt when the text changed.

use accesskit::{Node, Role};
use zgui_dom::{NodeKey, NodeKind};
use zgui_vocab::Semantics;

use crate::world::World;

/// The roles whose own text is what they hold rather than what they are called.
///
/// An editable element's text *is* its value, and the only place that value exists is the document
/// — the editing model writes it there and nothing else keeps a copy. A field that declared one as
/// well would be a second answer to the same question, drifting from the first the moment somebody
/// types; so a field declares no value and this reads the real one.
const HOLDS_ITS_TEXT_AS_A_VALUE: [Role; 3] =
    [Role::TextInput, Role::MultilineTextInput, Role::SearchInput];

/// Names `into` from the element's own text, unless the element named itself.
///
/// Which property the text lands in depends on the role, and the difference is not cosmetic: a
/// consumer reads the text of a [`Role::Label`] node from its **value** and every other node's from
/// its **label**, so writing to the wrong one produces a node that is in the tree and says nothing.
/// A field is read the same way as a label and for the same reason — the words inside it are what
/// it holds — while a field whose text were written to its label would be announced by whatever the
/// user last typed instead of by what it is for.
pub fn apply(world: &World<'_>, node: NodeKey, declared: &Semantics, into: &mut Node) {
    if declared.role == Role::Label || HOLDS_ITS_TEXT_AS_A_VALUE.contains(&declared.role) {
        if declared.value.is_none()
            && let Some(text) = from_content(world, node)
        {
            into.set_value(text);
        }
        return;
    }
    if declared.label.is_none()
        && let Some(text) = from_content(world, node)
    {
        into.set_label(text);
    }
}

/// The text directly inside `node`, joined with single spaces, or nothing when there is none.
pub fn from_content(world: &World<'_>, node: NodeKey) -> Option<String> {
    let store = world.document.store();
    let index = store.index_of(node)?;
    let mut name = String::new();
    let mut child = store.core(index).first_child();
    while let Some(current) = child {
        if store.core(current).kind() == NodeKind::Text
            && let Some(text) = zgui_dom::text::node::text_of(store, current)
            && !text.is_empty()
        {
            if !name.is_empty() {
                name.push(' ');
            }
            name.push_str(text);
        }
        child = store.core(current).next_sibling();
    }
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeKind};
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;

    use super::from_content;
    use crate::world::World;

    #[test]
    fn an_element_is_named_by_the_text_directly_inside_it_and_not_by_a_subtree() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let own = document.append(root, NodeKind::Text, ElementName::new("#text"));
        let nested = document.append(root, NodeKind::Element, ElementName::new("box"));
        let deep = document.append(nested, NodeKind::Text, ElementName::new("#text"));
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_text(own, "Full name");
                edit.set_text(deep, "not part of the name");
            })
            .expect("not poisoned");

        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        };
        assert_eq!(
            from_content(&world, document.store().key_of(root)).as_deref(),
            Some("Full name")
        );
    }

    #[test]
    fn an_element_with_no_text_of_its_own_is_named_by_nothing() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
        };
        assert_eq!(from_content(&world, document.store().key_of(root)), None);
    }
}
