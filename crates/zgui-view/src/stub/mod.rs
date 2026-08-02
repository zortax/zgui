//! A backend that keeps a tree in memory and answers everything else from what a test told it.
//!
//! Two implementations, one of each seam, and between them they are enough to build any view in
//! this crate, run its effects, mount it, unmount it and read back what happened. They exist for
//! three purposes: every example in this crate's documentation runs against them, this crate's
//! own tests use them, and anyone writing a real backend has a small complete one to read first.
//!
//! They are not a test harness. Nothing here records a transcript, asserts an expectation or
//! knows what a component is — a harness built for asserting on backend traffic is a different
//! thing, and it belongs beside the components it tests.
//!
//! This module is behind the `stub-backend` feature, which is off by default, so an application
//! links none of it.
//!
//! ```
//! use zgui_view::stub::{StubDom, StubHost};
//! use zgui_view::{Anchor, BuildCxOwned, DocumentId, DomHandle, HostHandle, IntoView, View};
//! use zgui_reactive::{Mounted, install};
//!
//! install().unwrap();
//! let dom = DomHandle::new(StubDom::new(DocumentId::FIRST));
//! let host = HostHandle::new(StubHost::default());
//! let root = Mounted::new();
//!
//! let cx = BuildCxOwned::new(dom.clone(), host, root.owner().clone(), DocumentId::FIRST);
//! let mut state = "hello".into_view().build(&mut cx.cx());
//! assert!(state.first_node().is_some());
//! ```

mod dom;
mod host;
mod node;

pub use crate::stub::dom::StubDom;
pub use crate::stub::host::StubHost;
pub use crate::stub::node::{StubKind, StubListener, StubNode};
