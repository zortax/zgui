//! Where a built view's nodes sit, and how they are taken away again.

use core::any::Any;

use crate::dom::DomHandle;
use crate::id::NodeId;

/// The positioning contract every built view satisfies.
///
/// A view's state knows which nodes it contributed and can put them somewhere, take them away, and
/// say which node comes first — which is all a parent needs in order to position a sibling before
/// it without knowing anything else about it.
///
/// [`Anchor::unmount`] runs the view's cleanups **synchronously**, before it returns. Letting the
/// effects behind a view's bindings drop their own scopes instead defers every cleanup by one poll
/// of the executor, which means one frame in which an unmounted view's timers still fire and its
/// observers still report geometry for nodes that are no longer in the tree.
pub trait Anchor: 'static {
    /// Attaches this view's nodes under `parent`, immediately before `before`.
    ///
    /// Calling this on a view that is already mounted moves it, which is what a keyed list does
    /// when an item changes position.
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>);

    /// Detaches this view's nodes and runs its cleanups, before returning.
    fn unmount(&mut self, dom: &DomHandle);

    /// The first node this view contributes, which a sibling uses as its insertion point.
    ///
    /// `None` only for a view that contributes no node at all.
    fn first_node(&self) -> Option<NodeId>;
}

/// An [`Anchor`] that can be downcast back to what it was.
///
/// This is what lets a type-erased view rebuild in place when it is handed a view of the same
/// type, instead of throwing its nodes away and building them again.
pub trait AnyAnchor: Anchor {
    /// This state, for downcasting.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T: Anchor> AnyAnchor for T {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Anchor for Box<dyn AnyAnchor> {
    fn mount(&mut self, dom: &DomHandle, parent: NodeId, before: Option<NodeId>) {
        (**self).mount(dom, parent, before);
    }

    fn unmount(&mut self, dom: &DomHandle) {
        (**self).unmount(dom);
    }

    fn first_node(&self) -> Option<NodeId> {
        (**self).first_node()
    }
}

/// A view that contributes nothing.
///
/// What `()` and an empty `Option` build to.
#[derive(Clone, Copy, Debug, Default)]
pub struct Empty;

impl Anchor for Empty {
    fn mount(&mut self, _dom: &DomHandle, _parent: NodeId, _before: Option<NodeId>) {}

    fn unmount(&mut self, _dom: &DomHandle) {}

    fn first_node(&self) -> Option<NodeId> {
        None
    }
}
