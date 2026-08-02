//! The whole of the node-tree seam, answered by the document.

use std::rc::Rc;

use zgui_dom::NodeKind;
use zgui_interned::{AttrName, ClassName, CustomPropertyName, ElementName, Ident};
use zgui_view::{
    Dom, EventCx, ListenerId, NodeId, ObservationHandle, ObservationSink, Observed, OverlayLayer,
};
use zgui_vocab::{EventKind, ListenerOptions, PropKey, PropValue, Semantics, UiState};

use crate::dom::DocumentDom;

impl Dom for DocumentDom {
    fn create_element(&self, name: ElementName) -> NodeId {
        self.create(NodeKind::Element, name)
    }

    fn create_text(&self, data: &str) -> NodeId {
        let node = self.create(NodeKind::Text, ElementName::new("#text"));
        if !data.is_empty() {
            self.write_text(node, data);
        }
        node
    }

    fn create_marker(&self) -> NodeId {
        self.create(NodeKind::Marker, ElementName::new("#marker"))
    }

    fn set_text(&self, node: NodeId, data: &str) {
        self.write_text(node, data);
    }

    fn insert(&self, parent: NodeId, child: NodeId, before: Option<NodeId>) {
        self.link(parent, child, before);
    }

    fn detach(&self, node: NodeId) {
        self.unlink(node);
    }

    fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.parent_of(node)
    }

    // Every writer from here down resolves through `live_index_of` and quietly does nothing when
    // the node is gone, rather than insisting it is still there. A binding's effect can fire one
    // frame after its element was removed — the record drops when the frame ends, the scope that
    // owns the effect only a moment later — and a write to an element that no longer exists has
    // nothing to update. Unwinding instead would poison the document mid-batch, turning one stale
    // write into a window that can never change again. Creation and insertion stay strict: putting
    // a child under a parent that is gone is not a late echo of anything, it is a bug worth
    // hearing about where it happened.
    fn set_attribute(&self, el: NodeId, name: AttrName, value: Option<&str>) {
        let Some(el) = self.live_index_of(el) else {
            return;
        };
        let value = value.map(Into::into);
        self.edit(|edit| edit.set_attribute(el, name, value));
    }

    fn set_classes(&self, el: NodeId, classes: &[ClassName]) {
        let Some(el) = self.live_index_of(el) else {
            return;
        };
        self.edit(|edit| edit.set_classes(el, classes));
    }

    fn toggle_class(&self, el: NodeId, class: ClassName, on: bool) {
        let Some(el) = self.live_index_of(el) else {
            return;
        };
        self.edit(|edit| {
            if on {
                edit.add_class(el, class);
            } else {
                edit.remove_class(el, class);
            }
        });
    }

    fn set_style_text(&self, el: NodeId, css: Option<&str>) {
        let Some(el) = self.live_index_of(el) else {
            return;
        };
        self.edit(|edit| edit.set_inline_style(el, css));
    }

    fn set_style_property(&self, el: NodeId, property: &str, value: Option<&str>) {
        let Some(index) = self.live_index_of(el) else {
            return;
        };
        let applied = self.edit(|edit| edit.set_style_property(index, property, value));
        if !applied {
            tracing::warn!(
                property,
                value,
                "dropped an inline declaration: no such property, or the value does not parse for it"
            );
        }
    }

    fn set_custom_property(&self, el: NodeId, property: CustomPropertyName, value: Option<&str>) {
        let Some(index) = self.live_index_of(el) else {
            return;
        };
        let applied = self.edit(|edit| edit.set_custom_property(index, property, value));
        if !applied {
            tracing::warn!(
                property = property.as_str(),
                value,
                "dropped a custom property: the value does not parse"
            );
        }
    }

    fn set_ui_state(&self, el: NodeId, state: UiState, on: bool) {
        let Some(el) = self.live_index_of(el) else {
            return;
        };
        self.edit(|edit| edit.set_state(el, state, on));
    }

    fn set_custom_state(&self, el: NodeId, name: Ident, on: bool) {
        let Some(el) = self.live_index_of(el) else {
            return;
        };
        self.edit(|edit| edit.set_custom_state(el, name, on));
    }

    fn set_property(&self, el: NodeId, property: PropKey, value: PropValue) {
        let Some(el) = self.live_index_of(el) else {
            return;
        };
        let value = (!value.is_unset()).then_some(value);
        self.edit(|edit| edit.set_property(el, property, value));
    }

    fn set_semantics(&self, el: NodeId, semantics: Option<&Semantics>) {
        let Some(el) = self.live_index_of(el) else {
            return;
        };
        let semantics = semantics.cloned();
        self.edit(|edit| edit.set_semantics(el, semantics));
    }

    fn add_listener(
        &self,
        el: NodeId,
        event: EventKind,
        options: ListenerOptions,
        handler: Rc<dyn Fn(&mut EventCx<'_>)>,
    ) -> ListenerId {
        let index = self.index_of(el);
        let id = self.edit(|edit| edit.add_listener(index, event, options));
        self.handlers().borrow_mut().insert(id, el, handler);
        ListenerId::new(id.get())
    }

    fn remove_listener(&self, el: NodeId, listener: ListenerId) {
        let id = zgui_dom::side::listeners::ListenerId::new(listener.get());
        // A registration outlives its element in the ordinary course of things. The guard that owns
        // one is dropped when the scope that installed it is disposed of, and disposing of a
        // subtree — an overlay closing, a window shutting down — removes the nodes first. The
        // element's own listener table went with it, so there is nothing left to take the
        // registration out of, and insisting on a live node here turns every teardown into a panic.
        //
        // The handler is released either way. That is the half that does not live in the document,
        // and skipping it for a departed node would leak the closure and everything it captured.
        if let Some(index) = self.live_index_of(el) {
            self.edit(|edit| edit.remove_listener(index, id));
        }
        self.handlers().borrow_mut().remove(id);
    }

    fn overlay_root(&self, of: NodeId, layer: OverlayLayer) -> NodeId {
        debug_assert!(of.belongs_to(self.document_id()));
        self.roots().layer(layer)
    }

    fn root(&self, of: NodeId) -> NodeId {
        debug_assert!(of.belongs_to(self.document_id()));
        self.roots().root()
    }

    fn text_content(&self, node: NodeId) -> String {
        Self::text_content(self, node)
    }

    fn observe(&self, node: NodeId, what: Observed, sink: ObservationSink) -> ObservationHandle {
        let index = self.index_of(node);
        let (registration, mask) = self.observations().borrow_mut().add(node, what, sink);
        self.edit(|edit| edit.set_observed(index, mask));

        // The handle does not keep the document alive. A view whose window has gone is entitled to
        // outlive it for as long as it takes to drop, and a deregistration that resurrected the
        // document in order to run would be the bug this avoids.
        let observations = self.observations_shared();
        let document = self.document_weak();
        ObservationHandle::new(move || {
            let Some(mask) = observations.borrow_mut().remove(node, what, registration) else {
                return;
            };
            let Some(document) = document.upgrade() else {
                return;
            };
            let document = document.borrow();
            let Some(index) =
                crate::id::to_document(node).and_then(|key| document.store().index_of(key))
            else {
                return;
            };
            // The filter is not consulted: recording what is watched takes no snapshot and marks
            // nothing, because what a view observes is not something any selector can see.
            let _ = document.edit(&zgui_dom::EverythingMatters, |edit| {
                edit.set_observed(index, mask);
            });
        })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use zgui_dom::Document;
    use zgui_interned::{AttrName, ClassName, ElementName};
    use zgui_view::Dom;
    use zgui_vocab::UiState;

    use crate::dom::DocumentDom;

    /// A backend over a fresh document, holding a handle whose node has already been dropped.
    fn stale_row() -> (DocumentDom, zgui_view::NodeId) {
        let document = Rc::new(RefCell::new(Document::new()));
        let dom = DocumentDom::new(Rc::clone(&document));
        let row = dom.create_element(ElementName::new("row"));
        dom.insert(dom.root(row), row, None);
        dom.detach(row);
        // The record survives to the end of the frame; past it, the handle names nothing.
        dom.end_frame();
        (dom, row)
    }

    #[test]
    fn a_write_to_a_node_that_is_gone_updates_nothing_and_poisons_nothing() {
        // The shape of a stale binding: its element left with a closing overlay, and the effect
        // that writes it fires once more before its own scope is disposed of. The write has to be
        // nothing at all — a panic here unwinds through the open edit batch, poisons the document,
        // and every later change panics in its place.
        let (dom, row) = stale_row();

        // Qualified calls, because the mutation ledger reads the dotted spelling as a write
        // around the batch: these go through the seam, which batches inside, and the spelling
        // says so.
        Dom::set_attribute(&dom, row, AttrName::new("data-state"), Some("open"));
        dom.toggle_class(row, ClassName::new("lit"), true);
        Dom::set_classes(&dom, row, &[ClassName::new("lit")]);
        dom.set_ui_state(row, UiState::DISABLED, true);
        dom.set_text(row, "still here?");
        // Removing what is already gone is the same late echo, from a teardown's side.
        dom.detach(row);

        // Nothing unwound, so the document still takes changes.
        let next = dom.create_element(ElementName::new("row"));
        dom.insert(dom.root(next), next, None);
        assert_eq!(dom.parent(next), Some(dom.root(next)));
    }
}
