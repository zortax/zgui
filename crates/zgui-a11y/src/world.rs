//! Everything a projection reads, in one borrow.
//!
//! An accessibility node is derived from four places at once — what the document says a node
//! means, where layout put it, where the coordinate systems that geometry is expressed in ended up,
//! and which node holds focus — and none of them can be looked up from the others. Passing them as
//! one value is what keeps the projection a function of the frame rather than of whatever happened
//! to be reachable.

use zgui_dom::{Document, NodeKey, NodeKind};
use zgui_layout::tree::store::LayoutStore;
use zgui_scene::Placements;
use zgui_vocab::Semantics;

/// The document, its geometry and its focus, as one frame's projection reads them.
pub struct World<'a> {
    /// The tree.
    pub document: &'a Document,
    /// Where its boxes ended up.
    pub layout: &'a LayoutStore,
    /// What each coordinate system those boxes are measured in resolved to.
    ///
    /// A fragment's rectangle is in its own space and the name of that space says nothing about
    /// where the space is, so geometry cannot be read from the layout store alone. These are the
    /// answers for the frame that was drawn, because a consumer is told where a control is on the
    /// screen it is looking at.
    pub placements: &'a Placements,
    /// How many device pixels there are to a CSS pixel.
    ///
    /// Bounds are reported in CSS pixels and the root node carries the scale, because a consumer
    /// that had to be told the scale separately would be told it one frame late on every display
    /// change.
    pub scale: f32,
    /// Which node holds keyboard focus, if any does.
    pub focus: Option<NodeKey>,
}

impl World<'_> {
    /// The node every other node hangs below.
    pub fn root(&self) -> NodeKey {
        self.document.store().key_of(self.document.document_index())
    }

    /// What `node` declares about itself, when it declares anything.
    pub fn semantics(&self, node: NodeKey) -> Option<&Semantics> {
        self.document
            .store()
            .columns()
            .semantics
            .get(node)
            .and_then(|slot| slot.as_deref())
    }

    /// Whether `node` declares anything beyond being a box on the screen.
    ///
    /// The same test the fragment pass applies before recording that a moved element owes a
    /// rectangle — see
    /// [`FrameDirty::is_semantic`](zgui_layout::fragment::diff::FrameDirty::is_semantic) — and it
    /// is here for the same reason: a moved *layout box* changes nothing an assistive technology
    /// was told, and a document is mostly layout boxes.
    pub fn declares_semantics(&self, node: NodeKey) -> bool {
        self.semantics(node)
            .is_some_and(|semantics| !semantics.is_trivial())
    }

    /// The text `node` holds, when it is a text node holding any.
    pub fn text(&self, node: NodeKey) -> Option<&str> {
        let index = self.document.store().index_of(node)?;
        zgui_dom::text::node::text_of(self.document.store(), index)
    }

    /// Whether `node` appears in the projected tree at all.
    ///
    /// This is the existence test every relation is filtered through, and it is deliberately much
    /// stricter than "names a live node". An identifier naming a marker resolves to nothing,
    /// because a marker holds a place and announces nothing. So does an identifier naming a node
    /// that has been taken out of the document: a removed node is still in the arena until the
    /// frame ends, and a projection that took "still allocated" for "still in the tree" would keep
    /// writing relations into a subtree the consumer has already dropped — which a consumer
    /// resolves with an unchecked lookup, on a thread this process does not own.
    pub fn is_projected(&self, node: NodeKey) -> bool {
        let store = self.document.store();
        let Some(index) = store.index_of(node) else {
            return false;
        };
        if !matches!(
            store.core(index).kind(),
            NodeKind::Document | NodeKind::Element
        ) {
            return false;
        }
        // Walked rather than read off a flag: attachment is recorded on the node an insertion or a
        // removal names, and a subtree taken out in one call leaves every node below it saying it
        // is attached. The walk is the only answer that is true of a descendant.
        let document = self.document.document_index();
        let mut current = index;
        loop {
            if current == document {
                return true;
            }
            match store.core(current).parent() {
                Some(parent) => current = parent,
                None => return false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;

    use super::World;

    #[test]
    fn a_marker_is_never_part_of_the_projected_tree() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let marker = document.append(root, NodeKind::Marker, ElementName::new("#marker"));
        let text = document.append(root, NodeKind::Text, ElementName::new("#text"));
        let layout = LayoutStore::new(document.store().document());
        let placements = zgui_scene::Placements::new();
        let world = World {
            document: &document,
            layout: &layout,
            placements: &placements,
            scale: 1.0,
            focus: None,
        };

        let store = document.store();
        assert!(world.is_projected(store.key_of(root)));
        assert!(!world.is_projected(store.key_of(marker)));
        assert!(
            !world.is_projected(store.key_of(text)),
            "the characters inside an element are that element's name, not a node of their own"
        );
    }

    #[test]
    fn a_subtree_taken_out_this_frame_is_out_of_the_tree_before_the_arena_recycles_it() {
        // The window between a removal and the end of the frame, which is exactly when the
        // accessibility update for that frame is built. Every node below the one that was removed
        // is still allocated, still an element, and still says it is in the document — and none of
        // them is in the tree any assistive technology is holding.
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let holder = document.append(root, NodeKind::Element, ElementName::new("box"));
        let deep = document.append(holder, NodeKind::Element, ElementName::new("label"));
        let keys = (
            document.store().key_of(holder),
            document.store().key_of(deep),
        );

        document
            .edit(&zgui_dom::EverythingMatters, |edit| edit.remove(holder))
            .expect("not poisoned");

        let layout = LayoutStore::new(document.store().document());
        let placements = zgui_scene::Placements::new();
        let world = World {
            document: &document,
            layout: &layout,
            placements: &placements,
            scale: 1.0,
            focus: None,
        };
        assert!(!world.is_projected(keys.0));
        assert!(
            !world.is_projected(keys.1),
            "a node below the one that was removed is still allocated and still an element, and a \
             relation written into it is one the consumer resolves against nothing"
        );
        assert!(world.is_projected(document.store().key_of(root)));
    }
}
