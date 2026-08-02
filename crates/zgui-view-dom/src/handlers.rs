//! Where a listener's handler lives.
//!
//! A registration and a handler are stored apart, and the split is forced rather than chosen. The
//! document keeps registrations — which event, registered how, under which identity — because that
//! is what routing an event needs and because those are plain data that many threads may read at
//! once. A handler is a reference-counted closure over the view layer's own context type, which
//! neither travels between threads nor is nameable from inside the document at all.
//!
//! So the handler stays here, on the backend that registered it, found again by the identity the
//! document handed back. Resolving an event is the document's half; calling what the identity
//! resolves to is this table's.

use std::rc::Rc;

use rustc_hash::FxHashMap;
use zgui_dom::Document;
use zgui_dom::side::listeners::ListenerId;
use zgui_view::{EventCx, NodeId};

/// One erased handler: a listener's body, with its argument type already forgotten.
pub type Handler = Rc<dyn Fn(&mut EventCx<'_>)>;

/// The handlers this backend is holding, by the identity that names each one.
#[derive(Default)]
pub struct Handlers {
    /// One entry per live registration, with the node it is registered on.
    entries: FxHashMap<ListenerId, (NodeId, Handler)>,
}

impl Handlers {
    /// A table with nothing in it.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many handlers are held.
    ///
    /// The number a test asserts is back to where it started after a subtree is unmounted: a
    /// handler left behind keeps a whole view's captured state alive and nothing else notices.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no handler is held.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Records `handler` under `id`.
    pub fn insert(&mut self, id: ListenerId, node: NodeId, handler: Handler) {
        self.entries.insert(id, (node, handler));
    }

    /// Forgets whatever was recorded under `id`.
    pub fn remove(&mut self, id: ListenerId) {
        self.entries.remove(&id);
    }

    /// The handler `id` names, if it still names one.
    ///
    /// Cloned rather than borrowed, because calling it re-enters the backend — a handler that
    /// changes the document is the ordinary case, not the exotic one — and a borrow held across
    /// that call would be a borrow held across an arbitrary amount of other work.
    pub fn get(&self, id: ListenerId) -> Option<Handler> {
        self.entries.get(&id).map(|(_, handler)| Rc::clone(handler))
    }

    /// Forgets every handler whose node `document` no longer has.
    ///
    /// A view takes its nodes out of the document and does not remove its listeners one by one —
    /// there is nothing left to remove them from. So the handlers outlive the nodes they were
    /// registered on by exactly as long as the removal is deferred, and this is what ends that.
    /// Without it every unmounted view leaves its captured state alive for the life of the window.
    pub fn retain_live(&mut self, document: &Document) {
        self.entries
            .retain(|_, (node, _)| crate::id::is_live(document, *node));
    }
}
