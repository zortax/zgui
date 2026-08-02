//! The node-tree seam, implemented over a real document.
//!
//! This is where a view stops being abstract. Building one creates real nodes, sets real
//! attributes, registers real listeners, and marks exactly the invalidation each of those owes —
//! because every change goes through the document's own batch API and that API owns the
//! bookkeeping. Every change, without exception — including the six nodes a window has before any
//! view is built. Nothing here writes a node any other way, and a change that tried to would fail
//! the check that reads this crate's sources looking for one.
//!
//! ```
//! use std::cell::RefCell;
//! use std::rc::Rc;
//! use zgui_dom::Document;
//! use zgui_interned::ClassName;
//! use zgui_view::{Anchor, BuildCxOwned, DomHandle, HostHandle, View};
//! use zgui_view_dom::DocumentDom;
//!
//! zgui_reactive::install().unwrap();
//! let document = Rc::new(RefCell::new(Document::new()));
//! let backend = Rc::new(DocumentDom::new(Rc::clone(&document)));
//! let root = backend.root_node();
//! let dom = DomHandle::from_rc(backend.clone());
//! let window = zgui_reactive::Mounted::new();
//! let cx = BuildCxOwned::new(
//!     dom.clone(),
//!     HostHandle::new(zgui_view::stub::StubHost::default()),
//!     window.owner().clone(),
//!     backend.document_id(),
//! );
//!
//! let mut built = window.with(|| {
//!     zgui_elements::row().class("toolbar").build(&mut cx.cx())
//! });
//! built.mount(&dom, root, None);
//!
//! let node = backend.index_of(built.node());
//! assert_eq!(document.borrow().store().classes_of(node).len(), 1);
//! window.unmount();
//! ```
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`dom`] | [`DocumentDom`], the window's root and overlay layers |
//! | [`handlers`] | the handlers behind the registrations the document holds |
//! | [`observations`] | who is watching which of a node's measurements |
//! | [`id`] | the one conversion between a view's handle and a document's name |

#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod dom;
pub mod handlers;
pub mod id;
pub mod observations;

pub use crate::dom::{DocumentDom, Roots};
