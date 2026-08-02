//! A cheap, cloneable handle to the installed node-tree backend.

use core::fmt::{self, Debug};
use core::ops::Deref;
use std::rc::Rc;

use crate::dom::Dom;

/// A handle to the installed backend.
///
/// Cloning is a reference-count bump. A reactive binding captures one, because a binding re-runs
/// long after the build that created it has returned and there is nothing it could borrow from.
///
/// The handle dereferences to the backend, so every [`Dom`] method is callable on it directly.
///
/// ```
/// use zgui_interned::ElementName;
/// use zgui_view::stub::StubDom;
/// use zgui_view::{DocumentId, DomHandle};
///
/// let dom = DomHandle::new(StubDom::new(DocumentId::FIRST));
/// let captured = dom.clone();
///
/// let node = dom.create_element(ElementName::new("box"));
/// assert_eq!(captured.parent(node), None);
/// ```
#[derive(Clone)]
pub struct DomHandle(Rc<dyn Dom>);

impl DomHandle {
    /// Installs `backend` behind a handle.
    pub fn new(backend: impl Dom + 'static) -> Self {
        Self(Rc::new(backend))
    }

    /// Wraps a backend that is already behind a reference count.
    pub fn from_rc(backend: Rc<dyn Dom>) -> Self {
        Self(backend)
    }

    /// Whether two handles name the same backend.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Deref for DomHandle {
    type Target = dyn Dom;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl Debug for DomHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("DomHandle")
            .field(&Rc::as_ptr(&self.0).cast::<()>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::DomHandle;
    use crate::DocumentId;
    use crate::stub::StubDom;

    #[test]
    fn a_clone_names_the_same_backend() {
        let handle = DomHandle::new(StubDom::new(DocumentId::FIRST));
        let clone = handle.clone();
        assert!(handle.ptr_eq(&clone));
    }

    #[test]
    fn two_backends_are_distinguishable() {
        let first = DomHandle::new(StubDom::new(DocumentId::FIRST));
        let second = DomHandle::new(StubDom::new(DocumentId::FIRST));
        assert!(!first.ptr_eq(&second));
    }
}
