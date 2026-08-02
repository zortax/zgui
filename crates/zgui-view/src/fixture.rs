//! One assembled backend, host, window scope and root element, for this crate's own tests.

use std::rc::Rc;

use zgui_interned::ElementName;
use zgui_reactive::{Mounted, install};

use crate::cx::{BuildCx, BuildCxOwned};
use crate::dom::DomHandle;
use crate::host::HostHandle;
use crate::id::{DocumentId, NodeId};
use crate::stub::{StubDom, StubHost};

/// Everything a view test needs, assembled once.
pub(crate) struct Fixture {
    /// The tree, for reading back what a view did.
    pub(crate) backend: Rc<StubDom>,
    /// The engine, for declaring geometry and advancing the clock.
    pub(crate) engine: Rc<StubHost>,
    /// The backend behind its handle.
    pub(crate) dom: DomHandle,
    /// The window's scope.
    pub(crate) window: Mounted,
    /// The build context.
    pub(crate) cx: BuildCxOwned,
    /// An element to mount into.
    pub(crate) root: NodeId,
}

impl Fixture {
    /// Assembles one.
    pub(crate) fn new() -> Self {
        install().ok();
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let engine = Rc::new(StubHost::default());
        let dom = DomHandle::from_rc(backend.clone());
        let host = HostHandle::from_rc(engine.clone());
        let window = Mounted::new();
        window.with(|| crate::cx::provide_host(host.clone()));
        let cx = BuildCxOwned::new(dom.clone(), host, window.owner().clone(), DocumentId::FIRST);
        let root = dom.create_element(ElementName::new("root"));
        Self {
            backend,
            engine,
            dom,
            window,
            cx,
            root,
        }
    }

    /// A borrowed build context.
    pub(crate) fn cx(&self) -> BuildCx<'_> {
        self.cx.cx()
    }

    /// The text of everything mounted under the root.
    pub(crate) fn text(&self) -> String {
        self.backend.text_content(self.root)
    }
}
