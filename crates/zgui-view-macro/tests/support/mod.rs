//! One assembled backend, host, window scope and root element, for the macros' own tests.

use std::rc::Rc;

use zgui_reactive::{Mounted, install};
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::{
    Anchor, BuildCxOwned, DocumentId, DomHandle, ElementName, HostHandle, IntoView, NodeId, View,
};

/// Everything a built view needs, assembled once.
#[allow(dead_code, reason = "each test uses the part of the harness it needs")]
pub(crate) struct Harness {
    /// The tree, for reading back what a view did.
    pub(crate) backend: Rc<StubDom>,
    /// The backend behind its handle.
    pub(crate) dom: DomHandle,
    /// The window's scope.
    pub(crate) window: Mounted,
    /// The build context.
    pub(crate) cx: BuildCxOwned,
    /// An element to mount into.
    pub(crate) root: NodeId,
}

#[allow(dead_code, reason = "each test uses the part of the harness it needs")]
impl Harness {
    /// Assembles one.
    pub(crate) fn new() -> Self {
        install().ok();
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let host = HostHandle::new(StubHost::default());
        let window = Mounted::new();
        window.with(|| zgui_view::provide_host(host.clone()));
        let cx = BuildCxOwned::new(dom.clone(), host, window.owner().clone(), DocumentId::FIRST);
        let root = dom.create_element(ElementName::new("root"));
        Self {
            backend,
            dom,
            window,
            cx,
            root,
        }
    }

    /// Builds and mounts a view, and hands back the state that unmounts it.
    pub(crate) fn mount<V: IntoView>(&self, view: V) -> impl Anchor {
        let mut state = self
            .window
            .with(|| view.into_view().build(&mut self.cx.cx()));
        state.mount(&self.dom, self.root, None);
        state
    }

    /// The text of everything mounted under the root.
    pub(crate) fn text(&self) -> String {
        self.backend.text_content(self.root)
    }
}
