//! A record field whose type has a borrow counter must not compile.
//!
//! `RefCell::borrow` is a non-atomic read-modify-write, so two workers reading the same shared
//! ancestor would race on the counter even though both accesses are logically reads.

use std::cell::{Cell, RefCell};

use zgui_dom::node_inner;

node_inner! {
    /// A record with one forbidden field.
    pub struct Bad {
        good: Cell<u32>,
        pending_hints: RefCell<Vec<u32>>,
    }
}

fn main() {}
