//! What a view is handed while it is being built.
//!
//! The backend is threaded through here and nowhere else. There is no thread-global holding the
//! installed backend and no `&'static` handed out from a reference count, which is what makes two
//! windows in one process work by construction rather than by convention: each window builds its
//! views through its own context, over its own backend, and every handle those views mint carries
//! its own [`DocumentId`].

mod owned;

use std::rc::Rc;

use zgui_reactive::{Owner, provide_local_context, use_local_context};

use crate::dom::DomHandle;
use crate::host::HostHandle;
use crate::id::DocumentId;
use crate::node_ref::ObservationRegistry;

pub use crate::cx::owned::BuildCxOwned;

/// Everything a view needs while it is being built or rebuilt.
///
/// ```
/// use zgui_interned::ElementName;
/// use zgui_reactive::{Mounted, install};
/// use zgui_view::stub::{StubDom, StubHost};
/// use zgui_view::{BuildCxOwned, DocumentId, DomHandle, HostHandle};
///
/// install().unwrap();
/// let node = Mounted::new();
/// let owned = BuildCxOwned::new(
///     DomHandle::new(StubDom::new(DocumentId::FIRST)),
///     HostHandle::new(StubHost::default()),
///     node.owner().clone(),
///     DocumentId::FIRST,
/// );
/// let cx = owned.cx();
///
/// let element = cx.dom().create_element(ElementName::new("row"));
/// assert!(element.belongs_to(cx.document()));
/// ```
pub struct BuildCx<'a> {
    /// The node-tree backend.
    dom: &'a DomHandle,
    /// The engine.
    host: &'a HostHandle,
    /// The scope for signals, memos, contexts and cleanups this view creates.
    owner: &'a Owner,
    /// Which document is being built.
    document: DocumentId,
}

impl<'a> BuildCx<'a> {
    /// Borrows the four parts of a context.
    pub fn new(
        dom: &'a DomHandle,
        host: &'a HostHandle,
        owner: &'a Owner,
        document: DocumentId,
    ) -> Self {
        Self {
            dom,
            host,
            owner,
            document,
        }
    }

    /// The node-tree backend.
    pub fn dom(&self) -> &DomHandle {
        self.dom
    }

    /// A handle to the backend, to capture in a closure that outlives this call.
    pub fn dom_handle(&self) -> DomHandle {
        self.dom.clone()
    }

    /// The engine.
    pub fn host(&self) -> &HostHandle {
        self.host
    }

    /// A handle to the engine, to capture in a closure or to bind into a
    /// [`NodeRef`](crate::NodeRef), that outlives this call.
    pub fn host_handle(&self) -> HostHandle {
        self.host.clone()
    }

    /// The scope everything built through this context belongs to.
    pub fn owner(&self) -> &Owner {
        self.owner
    }

    /// Which document is being built.
    pub fn document(&self) -> DocumentId {
        self.document
    }

    /// The same context, in a form that can be stored and re-borrowed later.
    pub fn to_owned_cx(&self) -> BuildCxOwned {
        BuildCxOwned::new(
            self.dom.clone(),
            self.host.clone(),
            self.owner.clone(),
            self.document,
        )
    }

    /// A child scope of this one, in a storable context.
    ///
    /// What a view creates for a piece of content whose lifetime is shorter than its own: a branch
    /// of a conditional, one row of a list. Disposing of the child frees everything that branch or
    /// row allocated, and nothing else.
    pub fn child(&self) -> BuildCxOwned {
        self.to_owned_cx().with_owner(self.owner.child())
    }

    /// Runs `build` with this context's scope current.
    ///
    /// Reactive values are attached to whichever scope is current when they are created, so
    /// anything that creates one — a component body, a binding, a control-flow branch — runs
    /// inside this.
    pub fn with_owner<T>(&self, build: impl FnOnce() -> T) -> T {
        self.owner.with(build)
    }
}

/// Makes `host` reachable from every scope below the current one, with a fresh registry for the
/// geometry that window's views observe.
///
/// The window's runtime calls this once, in the window's root scope. It is what lets
/// [`focused_node`](crate::focused_node), [`set_timeout`](crate::time::set_timeout) and
/// [`set_interval`](crate::time::set_interval) be free functions: they resolve the host through
/// the reactive ownership tree rather than through a global, so two windows in one process each
/// see their own.
///
/// The observation registry rides along because it is per window for the same reason the host is,
/// and because a view that observes geometry without one would silently observe nothing.
pub fn provide_host(host: HostHandle) {
    provide_local_context(host);
    provide_local_context(Rc::new(ObservationRegistry::new()));
}

/// The host provided by the enclosing window, when there is one.
pub fn current_host() -> Option<HostHandle> {
    use_local_context::<HostHandle>()
}

/// The observation registry the enclosing window provided, when there is one.
pub fn current_observations() -> Option<Rc<ObservationRegistry>> {
    use_local_context::<Rc<ObservationRegistry>>()
}

#[cfg(test)]
mod tests {
    use zgui_reactive::{Mounted, install};

    use super::{BuildCxOwned, current_host, provide_host};
    use crate::dom::DomHandle;
    use crate::host::HostHandle;
    use crate::id::DocumentId;
    use crate::stub::{StubDom, StubHost};

    fn context() -> (Mounted, BuildCxOwned) {
        install().ok();
        let node = Mounted::new();
        let cx = BuildCxOwned::new(
            DomHandle::new(StubDom::new(DocumentId::FIRST)),
            HostHandle::new(StubHost::default()),
            node.owner().clone(),
            DocumentId::FIRST,
        );
        (node, cx)
    }

    #[test]
    fn a_child_context_keeps_the_same_backend_and_a_new_scope() {
        let (node, cx) = context();
        let child = cx.cx().child();
        assert!(child.dom().ptr_eq(cx.dom()));
        assert!(child.host().ptr_eq(cx.host()));
        assert_ne!(child.owner().debug_id(), cx.owner().debug_id());
        node.unmount();
    }

    #[test]
    fn the_host_is_reached_through_the_scope_and_not_a_global() {
        install().ok();
        let outside = Mounted::new();
        assert!(outside.with(current_host).is_none());

        let window = Mounted::new();
        let host = HostHandle::new(StubHost::default());
        window.with(|| provide_host(host.clone()));
        let found = window.with(current_host).expect("the window provides one");
        assert!(found.ptr_eq(&host));

        // A second window's scope sees its own, not the first one's.
        let second_window = Mounted::new();
        let second_host = HostHandle::new(StubHost::default());
        second_window.with(|| provide_host(second_host.clone()));
        let found = second_window
            .with(current_host)
            .expect("the window provides one");
        assert!(found.ptr_eq(&second_host));
        assert!(!found.ptr_eq(&host));

        outside.unmount();
        window.unmount();
        second_window.unmount();
    }
}
