//! An observation column that carries the delivery channel must not compile.
//!
//! The channel is a reference-counted closure, for the same reason and with the same consequence as
//! a listener's handler: it would live inside the store, which is shared across worker threads.

use std::rc::Rc;

use zgui_arena::PagedVec;
use zgui_dom::side::ObservedMask;
use zgui_dom::NodeKey;

/// The rejected shape: the mask, and the sink the values are delivered through.
#[derive(Default)]
struct ObservationSlots {
    mask: ObservedMask,
    sink: Option<Rc<dyn Fn(f32)>>,
}

/// A store with that column in it.
struct DocumentStore {
    observed: PagedVec<NodeKey, ObservationSlots>,
}

const _: () = zgui_dom::assert_sync::<DocumentStore>();

fn main() {}
