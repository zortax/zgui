//! The owned form of a build context, for closures that outlive a build.

use zgui_reactive::Owner;

use crate::cx::BuildCx;
use crate::dom::DomHandle;
use crate::host::HostHandle;
use crate::id::DocumentId;

/// Everything a view needs while it is being built, in a form that can be stored.
///
/// A [`BuildCx`] borrows; a binding re-runs long after the build that created it has returned and
/// has nothing to borrow from, so it stores one of these and calls [`BuildCxOwned::cx`] each time
/// it runs.
///
/// ```
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
///
/// let cx = owned.cx();
/// assert_eq!(cx.document(), DocumentId::FIRST);
/// ```
#[derive(Clone)]
pub struct BuildCxOwned {
    /// The node-tree backend.
    dom: DomHandle,
    /// The engine.
    host: HostHandle,
    /// The scope everything built through this belongs to.
    owner: Owner,
    /// Which document is being built.
    document: DocumentId,
}

impl BuildCxOwned {
    /// Assembles a context from its four parts.
    pub fn new(dom: DomHandle, host: HostHandle, owner: Owner, document: DocumentId) -> Self {
        Self {
            dom,
            host,
            owner,
            document,
        }
    }

    /// Borrows this as a build context.
    pub fn cx(&self) -> BuildCx<'_> {
        BuildCx::new(&self.dom, &self.host, &self.owner, self.document)
    }

    /// The same context, with everything built through it belonging to `owner` instead.
    pub fn with_owner(&self, owner: Owner) -> Self {
        Self {
            dom: self.dom.clone(),
            host: self.host.clone(),
            owner,
            document: self.document,
        }
    }

    /// The node-tree backend.
    pub fn dom(&self) -> &DomHandle {
        &self.dom
    }

    /// The engine.
    pub fn host(&self) -> &HostHandle {
        &self.host
    }

    /// The scope everything built through this belongs to.
    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    /// Which document is being built.
    pub fn document(&self) -> DocumentId {
        self.document
    }
}
