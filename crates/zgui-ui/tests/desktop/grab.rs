//! The two handles a fixture needs, taken from inside the view being driven.
//!
//! A fixture that drives an application through the platform seam sits outside the document, and
//! asking *where* a control ended up is a question for the engine the view was built against. The
//! only place that engine is offered is the build context, so one view in the tree keeps a copy of
//! it and the fixture reads it back.

use std::cell::RefCell;

use zgui::view::{Anchor, BuildCx, DomHandle, HostHandle, NodeId, OverlayLayer, View};

thread_local! {
    /// What the marker view left behind, once it has been built.
    static HANDLES: RefCell<Option<Handles>> = const { RefCell::new(None) };
}

/// The engine seams one document is reachable through.
#[derive(Clone)]
pub struct Handles {
    /// The node tree.
    pub dom: DomHandle,
    /// The engine that laid it out.
    pub host: HostHandle,
    /// The marker this was taken from, which is the way back to the document's root.
    pub marker: NodeId,
}

impl Handles {
    /// Every root a node of this document can be under: the page, and each overlay band.
    ///
    /// A dialog and a menu are not under the page's root — they are portalled onto bands of their
    /// own — so anything looking for "everything in the window" that asks only about the page
    /// cannot see a single overlay, and every claim it makes about one comes back the same way
    /// whether the overlay is there or not.
    pub fn roots(&self) -> Vec<NodeId> {
        let mut roots = vec![self.dom.root(self.marker)];
        for layer in OverlayLayer::ALL {
            let node = self.dom.overlay_root(self.marker, *layer);
            if !roots.contains(&node) {
                roots.push(node);
            }
        }
        roots
    }
}

/// The handles the marker view left, once it has been built and mounted.
pub fn taken() -> Option<Handles> {
    HANDLES.with(|cell| cell.borrow().clone())
}

/// Forgets the last run's handles, so a fixture cannot read the one before it.
pub fn forget() {
    HANDLES.with(|cell| *cell.borrow_mut() = None);
}

/// A view that contributes no box and keeps the context it was built with.
pub struct Grab;

/// The marker [`Grab`] left in the tree.
pub struct Grabbed {
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
