//! A cheap, cloneable handle to the installed host.

use core::fmt::{self, Debug};
use core::ops::Deref;
use std::rc::Rc;

use crate::host::ViewHost;

/// A handle to the engine that lays a view's tree out.
///
/// Cloning is a reference-count bump. A [`NodeRef`](crate::NodeRef) stores one *inside* its
/// signal's value rather than beside it, which is what keeps a `NodeRef` [`Copy`].
///
/// The handle dereferences to the host, so every [`ViewHost`] method is callable on it directly.
///
/// ```
/// use zgui_view::stub::StubHost;
/// use zgui_view::{DocumentId, HostHandle, NodeId};
///
/// let host = HostHandle::new(StubHost::default());
/// let node = NodeId::new(DocumentId::FIRST, 1).expect("in range");
///
/// // A host with no layout answers the geometry questions with nothing, not with a guess.
/// assert_eq!(host.border_box(node), None);
/// ```
#[derive(Clone)]
pub struct HostHandle(Rc<dyn ViewHost>);

impl HostHandle {
    /// Installs `host` behind a handle.
    pub fn new(host: impl ViewHost + 'static) -> Self {
        Self(Rc::new(host))
    }

    /// Wraps a host that is already behind a reference count.
    pub fn from_rc(host: Rc<dyn ViewHost>) -> Self {
        Self(host)
    }

    /// Whether two handles name the same host.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Deref for HostHandle {
    type Target = dyn ViewHost;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl Debug for HostHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("HostHandle")
            .field(&Rc::as_ptr(&self.0).cast::<()>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::HostHandle;
    use crate::stub::StubHost;

    #[test]
    fn a_clone_names_the_same_host() {
        let handle = HostHandle::new(StubHost::default());
        assert!(handle.ptr_eq(&handle.clone()));
    }
}
