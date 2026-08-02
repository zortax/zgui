//! Which events a node listens for.
//!
//! A registration is which event, registered how, under which identity — and no handler. A handler
//! is a reference-counted closure over a type this crate sits several layers below, and the store
//! it would have to live in is shared with worker threads that cannot hold either. So what is kept
//! here is exactly what routing an event needs, and the handler lives with whoever registered it,
//! found again by the identity these methods hand back.
//!
//! Nothing about a registration is visible to selector matching or to any computed value, so none
//! of this enters the style engine. It does mark accessibility work: whether an element responds to
//! activation is part of what it means, and an element that gained a click listener has become
//! something an assistive technology should offer.

use zgui_bits::Dirty;
use zgui_vocab::{EventKind, ListenerOptions};

use crate::id::node_key::NodeIndex;
use crate::mutate::ancestors;
use crate::mutate::edit::Edit;
use crate::side::listeners::{Listener, ListenerId};

impl Edit<'_> {
    /// Registers that `node` listens for `event`, and returns the registration's identity.
    ///
    /// Identities are unique for the life of the document and are never reused, so one that names
    /// a registration already removed resolves to nothing rather than to a later one.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn add_listener(
        &mut self,
        node: NodeIndex,
        event: EventKind,
        options: ListenerOptions,
    ) -> ListenerId {
        let id = self.document().edit_state().next_listener();
        let store = self.store();
        let key = store.key_of(node);
        store.columns_mut().listeners.get_mut(key).add(Listener {
            kind: event,
            options,
            id,
        });
        ancestors::mark(self.store(), node, Dirty::A11Y);
        id
    }

    /// Removes the registration `id` names from `node`, and says whether it was there.
    ///
    /// # Panics
    ///
    /// Panics if `node` names no live node of the document.
    pub fn remove_listener(&mut self, node: NodeIndex, id: ListenerId) -> bool {
        let store = self.store();
        let key = store.key_of(node);
        if !store.columns_mut().listeners.get_mut(key).remove(id) {
            return false;
        }
        ancestors::mark(self.store(), node, Dirty::A11Y);
        true
    }
}

#[cfg(test)]
mod tests {
    use zgui_interned::ElementName;
    use zgui_vocab::{EventKind, ListenerOptions};

    use crate::arena::document::Document;
    use crate::mutate::filter::EverythingMatters;
    use crate::node::kind::NodeKind;
    use crate::side::listeners::ListenerId;

    #[test]
    fn a_registration_is_findable_and_removable_and_its_identity_is_never_reused() {
        let mut document = Document::new();
        let control = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("control"),
        );
        let key = document.store().key_of(control);

        let (first, second) = document
            .edit(&EverythingMatters, |edit| {
                let first = edit.add_listener(control, EventKind::Click, ListenerOptions::DEFAULT);
                let second =
                    edit.add_listener(control, EventKind::KeyDown, ListenerOptions::DEFAULT);
                (first, second)
            })
            .expect("not poisoned");
        assert_ne!(first, second);
        assert!(
            document
                .store()
                .columns()
                .listeners
                .get(key)
                .expect("the node listens for something")
                .listens_for(EventKind::Click)
        );

        let third = document
            .edit(&EverythingMatters, |edit| {
                assert!(edit.remove_listener(control, first));
                assert!(!edit.remove_listener(control, first));
                edit.add_listener(control, EventKind::Click, ListenerOptions::DEFAULT)
            })
            .expect("not poisoned");
        assert_ne!(third, first, "an identity is never handed out twice");
        assert_ne!(third, second);
        assert_ne!(third, ListenerId::new(0));
    }
}
