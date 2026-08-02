//! Which events a node listens for — and deliberately not what it does about them.
//!
//! This column holds registrations and no handlers, and that is forced twice over. A handler is a
//! reference-counted closure, and a reference count is not safe to share across the threads that
//! walk this store — the store's own `Sync` assertion rejects one outright, which is the assertion's
//! whole purpose. Independently, a handler's argument is a view-layer type that this crate sits
//! three layers below and cannot name.
//!
//! What is here is everything routing needs: which event, registered how, under which identity. The
//! erased handlers live in the runtime's dispatch table, keyed by [`ListenerId`], where a reference
//! count is ordinary and the view's types are in scope.

use smallvec::SmallVec;
use zgui_vocab::{EventKind, ListenerOptions};

/// The identity of one listener registration.
///
/// Issued by the document when a listener is added, and used by the runtime to find the handler
/// that goes with it. Identities are never reused, so a handler removed while an event is mid-flight
/// cannot be confused with one registered afterwards.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct ListenerId(u64);

impl ListenerId {
    /// The identity numbered `value`.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The identity as a plain integer.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One listener registration.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Listener {
    /// Which event it listens for.
    pub kind: EventKind,
    /// How it was registered: which leg of the dispatch it runs in, and the two promises.
    pub options: ListenerOptions,
    /// Which handler it names.
    pub id: ListenerId,
}

/// Every listener registered on one node.
///
/// Sparse: a small fraction of the nodes in a document listen for anything at all.
#[derive(Clone, Default, Debug)]
pub struct ListenerSet {
    /// In registration order, which is the order handlers run in within one leg of a dispatch.
    entries: SmallVec<[Listener; 2]>,
}

impl ListenerSet {
    /// A set with nothing registered.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many registrations the set holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is registered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds a registration.
    pub fn add(&mut self, listener: Listener) {
        self.entries.push(listener);
    }

    /// Removes the registration `id` names, and says whether it was there.
    pub fn remove(&mut self, id: ListenerId) -> bool {
        let Some(position) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        self.entries.remove(position);
        true
    }

    /// Every registration, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &Listener> {
        self.entries.iter()
    }

    /// Whether anything here listens for `kind`.
    pub fn listens_for(&self, kind: EventKind) -> bool {
        self.entries.iter().any(|entry| entry.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use zgui_vocab::{EventKind, ListenerOptions};

    use super::{Listener, ListenerId, ListenerSet};

    fn listener(id: u64, kind: EventKind) -> Listener {
        Listener {
            kind,
            options: ListenerOptions::DEFAULT,
            id: ListenerId::new(id),
        }
    }

    #[test]
    fn registrations_keep_their_order_and_are_removable_by_identity() {
        let mut set = ListenerSet::new();
        assert!(set.is_empty());
        set.add(listener(1, EventKind::Click));
        set.add(listener(2, EventKind::PointerDown));
        assert_eq!(set.len(), 2);
        assert!(set.listens_for(EventKind::Click));
        assert!(!set.listens_for(EventKind::KeyDown));

        assert!(set.remove(ListenerId::new(1)));
        assert!(!set.remove(ListenerId::new(1)));
        assert_eq!(
            set.iter().map(|entry| entry.id.get()).collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn a_registration_carries_no_handler_at_all() {
        // The point of the type, stated as a test: what the column holds is `Copy`, which is what
        // keeps the whole store shareable across the threads that match selectors against it.
        fn assert_copy<T: Copy>() {}
        assert_copy::<Listener>();
    }
}
