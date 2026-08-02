//! Reading back what was written.
//!
//! The node-tree seam is a set of writes and one question — who is a node's parent — because that
//! is what a retained view layer needs and nothing more. Everything else about a node is a question
//! for whoever is holding the document, and these are the answers, in the view layer's own
//! vocabulary rather than the style engine's.

use zgui_interned::{AttrName, ClassName, Ident};
use zgui_view::NodeId;
use zgui_vocab::{PropValue, Semantics, UiState};

use crate::dom::DocumentDom;
use crate::id;

impl DocumentDom {
    /// The children of `node`, in order.
    pub fn children(&self, node: NodeId) -> Vec<NodeId> {
        let document = self.document();
        let index = id::resolve(&document, node);
        let mut children = Vec::new();
        let mut next = document.store().core(index).first_child();
        while let Some(child) = next {
            children.push(id::to_view(document.store().key_of(child)));
            next = document.store().core(child).next_sibling();
        }
        children
    }

    /// The whole class list of `node`, in order.
    pub fn classes(&self, node: NodeId) -> Vec<ClassName> {
        let document = self.document();
        let index = id::resolve(&document, node);
        document
            .store()
            .classes_of(index)
            .iter()
            .map(|class| ClassName::new(class))
            .collect()
    }

    /// The value of one of `node`'s attributes.
    pub fn attribute(&self, node: NodeId, name: AttrName) -> Option<String> {
        let document = self.document();
        let index = id::resolve(&document, node);
        document
            .node(index)
            .attrs()
            .find(|attr| attr.name == name)
            .map(|attr| attr.value.as_str().to_owned())
    }

    /// The interaction state of `node`.
    pub fn ui_state(&self, node: NodeId) -> UiState {
        let document = self.document();
        let index = id::resolve(&document, node);
        zgui_dom::node::element::state::from_engine(document.store().core(index).state())
    }

    /// Whether `node` carries the author-defined state `name`.
    pub fn has_custom_state(&self, node: NodeId, name: Ident) -> bool {
        let document = self.document();
        let index = id::resolve(&document, node);
        let mut found = false;
        document.node(index).each_custom_state(|held| {
            found |= held.as_ref() == name.as_str();
        });
        found
    }

    /// The value of one of `node`'s imperative properties.
    pub fn property(&self, node: NodeId, key: zgui_vocab::PropKey) -> PropValue {
        let document = self.document();
        let index = id::resolve(&document, node);
        document
            .store()
            .columns()
            .props
            .get(document.store().key_of(index))
            .and_then(|props| props.get(key))
            .cloned()
            .unwrap_or(PropValue::Unset)
    }

    /// What `node` means to an accessibility tree.
    pub fn semantics(&self, node: NodeId) -> Option<Semantics> {
        let document = self.document();
        let index = id::resolve(&document, node);
        document
            .store()
            .columns()
            .semantics
            .get(document.store().key_of(index))
            .and_then(|slot| slot.as_deref())
            .cloned()
    }

    /// Every character `node`'s subtree contributes, in order.
    pub fn text_content(&self, node: NodeId) -> String {
        let document = self.document();
        let index = id::resolve(&document, node);
        let mut text = String::new();
        collect_text(&document, index, &mut text);
        text
    }
}

/// Appends the text of `index`'s subtree to `into`.
fn collect_text(document: &zgui_dom::Document, index: zgui_dom::NodeIndex, into: &mut String) {
    if let Some(own) = zgui_dom::text::node::text_of(document.store(), index) {
        into.push_str(own);
    }
    let mut next = document.store().core(index).first_child();
    while let Some(child) = next {
        collect_text(document, child, into);
        next = document.store().core(child).next_sibling();
    }
}
