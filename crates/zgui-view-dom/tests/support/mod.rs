//! A window: a document, a backend over it, and a scope to build views in.

#![allow(unreachable_pub, dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use zgui_dom::Document;
use zgui_reactive::Mounted;
use zgui_view::stub::StubHost;
use zgui_view::{BuildCxOwned, DomHandle, HostHandle, NodeId};
use zgui_view_dom::DocumentDom;

/// Everything a test needs in order to build a view into a real document.
pub struct Window {
    /// The document itself, shared with the backend.
    pub document: Rc<RefCell<Document>>,
    /// The backend the view is built against.
    pub backend: Rc<DocumentDom>,
    /// The handle a view holds.
    pub dom: DomHandle,
    /// The scope everything built here belongs to.
    pub window: Mounted,
    /// The context views are built through.
    pub cx: BuildCxOwned,
    /// The window's root element.
    pub root: NodeId,
}

impl Window {
    /// Opens one.
    pub fn open() -> Self {
        zgui_reactive::install().ok();
        let document = Rc::new(RefCell::new(Document::new()));
        let backend = Rc::new(DocumentDom::new(Rc::clone(&document)));
        let dom = DomHandle::from_rc(backend.clone());
        let window = Mounted::new();
        let cx = BuildCxOwned::new(
            dom.clone(),
            HostHandle::new(StubHost::default()),
            window.owner().clone(),
            backend.document_id(),
        );
        let root = backend.root_node();
        Self {
            document,
            backend,
            dom,
            window,
            cx,
            root,
        }
    }

    /// How many nodes the document is holding.
    pub fn live_nodes(&self) -> usize {
        self.document.borrow().len()
    }
}

/// Calls `handler` the way a dispatch would, with a press on `node` as the target.
///
/// Not a dispatcher: it resolves nothing and walks nothing. It exists so a test can ask what one
/// press costs a handler, which is the question a listener that was registered twice answers
/// differently.
pub fn dispatch_click(handler: &zgui_view_dom::handlers::Handler, node: NodeId) {
    use zgui_geom::{CssPx, Point};
    use zgui_view::{DiscardCommands, EventControl, EventCx, EventType, events};
    use zgui_vocab::{Modifiers, Payload, Phase, PointerEvent, Timestamp};

    let payload = Payload::Pointer(PointerEvent::mouse(Point::new(CssPx(0.0), CssPx(0.0))));
    let control = EventControl::new();
    let mut sink = DiscardCommands;
    let mut cx = EventCx::new(
        events::CLICK.kind(),
        node,
        node,
        Phase::Target,
        Modifiers::NONE,
        Timestamp::ORIGIN,
        &payload,
        &control,
        &mut sink,
    );
    handler(&mut cx);
}
