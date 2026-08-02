//! The two handles a driver needs, taken from inside the view that is being driven.
//!
//! A driver sits outside the document: it is handed events and hands them on. Asking *where* a
//! control is, or what it says, is a question for the engine the view was built against, and the
//! only place that engine is offered is the build context. So one view in the tree keeps a copy of
//! it here, and the driver reads it back.
//!
//! The handles are reference counts into structures owned by the window's own thread, which is the
//! thread the driver runs on too, so nothing here crosses a thread and nothing is synchronised.

use std::cell::RefCell;

use zgui::view::{Anchor, BuildCx, DomHandle, HostHandle, NodeId, View};

thread_local! {
    /// What the marker view left behind, once it has been built.
    static HANDLES: RefCell<Option<Handles>> = const { RefCell::new(None) };
}

/// The engine seams one document is reachable through.
#[derive(Clone)]
pub(crate) struct Handles {
    /// The node tree.
    pub(crate) dom: DomHandle,
    /// The engine that laid it out.
    pub(crate) host: HostHandle,
    /// The marker this was taken from, which is the way back to the document's root.
    pub(crate) marker: NodeId,
}

impl Handles {
    /// The document's root element, resolved through the marker.
    pub(crate) fn root(&self) -> NodeId {
        self.dom.root(self.marker)
    }

    /// Every root a node of this document can be under: the page, and each overlay band.
    ///
    /// A dialog, a menu and a toast are not under the page's root — they are portalled onto bands
    /// of their own, which is what lets a menu paint over a dialog without either knowing about
    /// the other. Anything looking for "everything in the window" that asks only about the page
    /// therefore cannot see a single overlay, and every claim it makes about one — that it opened,
    /// that it closed — comes back the same way whether or not the overlay is there at all.
    pub(crate) fn roots(&self) -> Vec<NodeId> {
        let mut roots = vec![self.root()];
        for layer in zgui::view::OverlayLayer::ALL {
            let node = self.dom.overlay_root(self.marker, *layer);
            if !roots.contains(&node) {
                roots.push(node);
            }
        }
        roots
    }
}

/// The handles the marker view left, once it has been built and mounted.
pub(crate) fn taken() -> Option<Handles> {
    HANDLES.with(|cell| cell.borrow().clone())
}

/// A view that contributes no box and keeps the context it was built with.
///
/// It builds a marker rather than an element so that adding it to a document changes neither the
/// layout nor the picture: what is driven is the interface as it ships, not the interface with an
/// extra box in it.
pub(crate) struct Grab;

/// The marker [`Grab`] left in the tree.
pub(crate) struct Grabbed {
    /// The marker node.
    node: NodeId,
}

impl View for Grab {
    type State = Grabbed;

    fn build(self, cx: &mut BuildCx<'_>) -> Grabbed {
        let node = cx.dom().create_marker();
        HANDLES.with(|cell| {
            *cell.borrow_mut() = Some(Handles {
                dom: cx.dom_handle(),
                host: cx.host_handle(),
                marker: node,
            });
        });
        Grabbed { node }
    }

    fn rebuild(self, _state: &mut Grabbed, _cx: &mut BuildCx<'_>) {}
}

impl Anchor for Grabbed {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        dom.insert(parent, self.node, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        dom.detach(self.node);
    }

    fn first_node(&self) -> Option<NodeId> {
        Some(self.node)
    }
}
