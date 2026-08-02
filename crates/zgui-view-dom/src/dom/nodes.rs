//! Creating nodes, moving them, and taking them out again.

use zgui_dom::NodeKind;
use zgui_interned::ElementName;
use zgui_view::NodeId;

use crate::dom::DocumentDom;
use crate::id;

impl DocumentDom {
    /// Creates a node of `kind` called `name`, outside the document.
    pub(crate) fn create(&self, kind: NodeKind, name: ElementName) -> NodeId {
        let index = self.edit(|edit| match kind {
            NodeKind::Text => edit.create_text(""),
            NodeKind::Marker => edit.create_marker(),
            _ => edit.create_element(name),
        });
        id::to_view(self.document().store().key_of(index))
    }

    /// Puts `child` under `parent`, immediately before `before`.
    pub(crate) fn link(&self, parent: NodeId, child: NodeId, before: Option<NodeId>) {
        let parent = self.index_of(parent);
        let child = self.index_of(child);
        let before = before.map(|node| self.index_of(node));
        self.edit(|edit| edit.insert_before(parent, child, before));
    }

    /// Takes `node` out of the document.
    ///
    /// The record stays readable for the rest of the frame, and putting the node back before the
    /// frame ends keeps it — which is exactly what a list that moves a row does. What is still out
    /// when the frame ends is what the frame's end drops.
    ///
    /// A node that is already gone is left gone. A teardown can reach the same element twice —
    /// once from its own view and once from an ancestor whose whole subtree went — and the second
    /// arrival has nothing left to remove; unwinding over it would poison the document mid-batch.
    pub(crate) fn unlink(&self, node: NodeId) {
        let Some(node) = self.live_index_of(node) else {
            return;
        };
        self.edit(|edit| edit.remove(node));
    }

    /// What `node` currently sits under, or nothing when it sits under the document itself.
    pub(crate) fn parent_of(&self, node: NodeId) -> Option<NodeId> {
        let document = self.document();
        let index = id::resolve(&document, node);
        let parent = document.store().core(index).parent()?;
        if parent == document.document_index() {
            return None;
        }
        Some(id::to_view(document.store().key_of(parent)))
    }

    /// Replaces the text `node` holds.
    ///
    /// A text binding is a writer like any attribute's and goes stale the same way — its effect
    /// can fire one frame after the node went — so a node that is gone is quietly left alone:
    /// there is no text left to replace, and unwinding would poison the document.
    pub(crate) fn write_text(&self, node: NodeId, data: &str) {
        let Some(node) = self.live_index_of(node) else {
            return;
        };
        self.edit(|edit| edit.set_text(node, data));
    }
}
