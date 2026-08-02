//! Where the handlers live, which is not in the tree.
//!
//! The node tree keeps *registrations* — which event, registered how, under which identity — and
//! the handlers themselves live here, found again by that identity. That is not an arrangement
//! invented for this crate: it is the shape the real document and the real runtime have, forced on
//! them by the fact that a handler is a reference-counted closure over a view-layer type and the
//! document is neither reference-counted nor able to name one. Keeping the same split here is what
//! makes a test written against this backend a test of the thing that will actually run.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use zgui_view::{EventCx, ListenerId, NodeId};
use zgui_vocab::{EventKind, ListenerOptions};

/// What runs when an event reaches a registration.
///
/// Named because it is written down in several places and because the shape is the seam: the tree
/// holds registrations, this holds the closures, and the two are joined by an identity.
pub type Handler = Rc<dyn Fn(&mut EventCx<'_>)>;

/// One registered handler.
#[derive(Clone)]
pub struct Registration {
    /// Which registration this is.
    pub id: ListenerId,
    /// Which event it listens for.
    pub event: EventKind,
    /// How it was registered.
    pub options: ListenerOptions,
    /// What runs.
    pub handler: Handler,
}

/// The handlers registered on a tree, by element, in registration order.
#[derive(Clone, Default)]
pub struct Handlers {
    /// One entry per element that has any, shared with every clone.
    by_node: Rc<RefCell<BTreeMap<NodeId, Vec<Registration>>>>,
}

impl Handlers {
    /// A table with nothing registered.
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a handler.
    pub fn add(&self, node: NodeId, registration: Registration) {
        self.by_node
            .borrow_mut()
            .entry(node)
            .or_default()
            .push(registration);
    }

    /// Forgets one registration, and says whether it was there.
    pub fn remove(&self, node: NodeId, id: ListenerId) -> bool {
        let mut by_node = self.by_node.borrow_mut();
        let Some(registrations) = by_node.get_mut(&node) else {
            return false;
        };
        let Some(at) = registrations.iter().position(|entry| entry.id == id) else {
            return false;
        };
        registrations.remove(at);
        true
    }

    /// One element's registrations for one event, in registration order.
    pub fn of(&self, node: NodeId, event: EventKind) -> Vec<Registration> {
        self.by_node
            .borrow()
            .get(&node)
            .map(|registrations| {
                registrations
                    .iter()
                    .filter(|entry| entry.event == event)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The handler registered under `id` on `node`, if that registration is still there.
    ///
    /// By identity rather than by position, because a handler is entitled to remove a registration
    /// while an event is still travelling — a layer that dismisses itself does exactly that — and
    /// every position after the removed one has moved by the time the next step runs.
    pub fn handler_of(&self, node: NodeId, id: ListenerId) -> Option<Handler> {
        self.by_node
            .borrow()
            .get(&node)?
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| Rc::clone(&entry.handler))
    }

    /// How many registrations are live.
    pub fn len(&self) -> usize {
        self.by_node
            .borrow()
            .values()
            .map(|registrations| registrations.len())
            .sum()
    }

    /// Whether none is.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl core::fmt::Debug for Handlers {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("Handlers")
            .field("registrations", &self.len())
            .finish()
    }
}
