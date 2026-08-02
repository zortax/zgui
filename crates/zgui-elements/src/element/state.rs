//! What a built element retains.

use zgui_view::view::AnyViewState;
use zgui_view::{
    A11yBinding, Anchor, AnyView, Binding, BuildCx, Classes, DomHandle, ListenerRegistration,
    NodeId, View,
};

use crate::element::attribute::Attribute;

/// A built element: its node, whatever keeps its attributes up to date, and its children.
///
/// Held by whatever built the element. Dropping it stops the bindings; it does not remove the node,
/// because a view's nodes are taken away by [`Anchor::unmount`] and a state that removed them on
/// drop would take them away twice.
pub struct ElementState {
    /// The element itself.
    node: NodeId,
    /// One entry per reactive attribute. A static attribute keeps nothing.
    bindings: Vec<Binding>,
    /// One entry per listener this description registered, so that describing the element again
    /// replaces them instead of leaving a second copy of each attached.
    listeners: Vec<ListenerRegistration>,
    /// The children, in order.
    children: Vec<AnyViewState>,
}

impl ElementState {
    /// A state over a freshly created element with nothing on it.
    pub(crate) fn new(node: NodeId) -> Self {
        Self {
            node,
            bindings: Vec::new(),
            listeners: Vec::new(),
            children: Vec::new(),
        }
    }

    /// The element this state is about.
    ///
    /// What a caller reaches for when it has to talk to the element directly — mounting something
    /// beside it, asking the backend about it.
    pub fn node(&self) -> NodeId {
        self.node
    }

    /// Writes a whole description onto the element, replacing whatever was keeping the previous
    /// one up to date.
    ///
    /// The class list goes on first and the accessibility description last, because everything in
    /// between may add to either: a class toggle has to land on top of the merged list, and an
    /// accessibility property from a forwarded bundle has to be merged before anything is written.
    ///
    /// The listeners the previous description registered are taken off the element before this
    /// one's are added. A description is a statement of what the element listens for, not a
    /// request to listen for it once more: an element inside a closure is re-described on every
    /// change the closure reads, so leaving the previous registration attached would mean one more
    /// call of the handler per change, for ever.
    pub(crate) fn apply(
        &mut self,
        attributes: Vec<Attribute>,
        classes: Classes,
        a11y: Option<A11yBinding>,
        cx: &mut BuildCx<'_>,
    ) {
        self.bindings.clear();
        for listener in self.listeners.drain(..) {
            listener.remove(&**cx.dom());
        }
        if !classes.is_empty() {
            cx.dom().set_classes(self.node, classes.names());
        }
        for attribute in attributes {
            if let Some(binding) = attribute.apply(cx, self.node, &mut self.listeners) {
                self.bindings.push(binding);
            }
        }
        if let Some(a11y) = a11y {
            self.bindings
                .push(zgui_view::binding::bind_semantics(cx, self.node, a11y));
        }
    }

    /// Builds the children and puts them under the element.
    pub(crate) fn build_children(&mut self, children: Vec<AnyView>, cx: &mut BuildCx<'_>) {
        for child in children {
            let mut built = child.build(cx);
            built.mount(cx.dom(), self.node, None);
            self.children.push(built);
        }
    }

    /// Rebuilds the children position by position.
    ///
    /// The shared prefix is rebuilt in place, anything extra is built and appended, anything left
    /// over is unmounted. Children written out in a view are identified by where they are, so that
    /// is the right rule for them; a collection whose items have identities belongs in a keyed
    /// list, which moves nodes instead of rewriting them.
    pub(crate) fn rebuild_children(&mut self, children: Vec<AnyView>, cx: &mut BuildCx<'_>) {
        let kept = children.len();
        for (position, child) in children.into_iter().enumerate() {
            match self.children.get_mut(position) {
                Some(existing) => child.rebuild(existing, cx),
                None => {
                    let mut built = child.build(cx);
                    built.mount(cx.dom(), self.node, None);
                    self.children.push(built);
                }
            }
        }
        for mut extra in self.children.drain(kept..) {
            extra.unmount(cx.dom());
        }
    }
}

impl Anchor for ElementState {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        dom.insert(parent, self.node, before);
    }

    /// Takes the element out of the tree.
    ///
    /// The listeners stay registered, because the node stays usable and putting it back is the
    /// ordinary case — a list that moves a row unmounts and mounts it again, and a row that came
    /// back deaf would be a row whose buttons had stopped working. What the node holds when it is
    /// gone for good goes with the node.
    fn unmount(&mut self, dom: &DomHandle) {
        for child in &mut self.children {
            child.unmount(dom);
        }
        self.bindings.clear();
        dom.detach(self.node);
    }

    fn first_node(&self) -> Option<NodeId> {
        Some(self.node)
    }
}
