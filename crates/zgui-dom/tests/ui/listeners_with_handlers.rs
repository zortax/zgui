//! A listener column that carries handlers must not compile.
//!
//! A handler is a reference-counted closure. Putting one in a column puts it inside the store, and
//! the store is shared with the threads that match selectors, so the reference count would be
//! updated from several of them at once.

use std::rc::Rc;

use zgui_arena::PagedVec;
use zgui_dom::NodeKey;

/// The rejected shape: registrations *and* the handlers that go with them.
#[derive(Default)]
struct ListenerSet {
    handlers: Vec<Rc<dyn Fn()>>,
}

/// A store with that column in it.
struct DocumentStore {
    listeners: PagedVec<NodeKey, ListenerSet>,
}

const _: () = zgui_dom::assert_sync::<DocumentStore>();

fn main() {}
