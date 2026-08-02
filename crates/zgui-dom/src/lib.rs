//! The document: one arena of nodes, the records inside it, and the rule that keeps them safe to
//! read from many threads at once.
//!
//! # Why the record looks the way it does
//!
//! A style engine matches selectors in parallel. It hands a worker a handle to one element and
//! that worker walks *outwards* — to the parent for a descendant combinator, to the previous
//! sibling for `+`, to the children for `:empty` — reading names, classes, identifiers and
//! interaction state off records that other workers are simultaneously standing on. Everything a
//! handle can reach is therefore read concurrently, and the few things the engine writes are
//! written concurrently.
//!
//! That single fact shapes the whole crate:
//!
//! * A handle is [`Node`], and it is exactly one machine word, because the engine's style-sharing
//!   cache stores one in a word-sized slot and checks the size while it runs. The back-pointer
//!   that makes a bare reference sufficient lives *inside* the record.
//! * Every field of [`NodeInner`] obeys the cell discipline: plain data, a [`Cell`] of something
//!   [`Copy`], or an atomic. [`RefCell`] is forbidden, because its borrow counter is a non-atomic
//!   read-modify-write and two workers reading the same shared ancestor would race on it even
//!   though both accesses are logically reads. The rule is enforced at the declaration site by
//!   [`CellDisciplined`], not by review.
//! * Everything that is not [`Copy`] lives in a [`Columns`] side table, reached through the
//!   back-pointer, and the whole store carries a `Sync` assertion so that an [`Rc`] parked in a
//!   column is a compile error rather than a data race.
//! * Node addresses never move, so a reference handed to a worker survives every insertion that
//!   happens while it is held.
//!
//! [`Cell`]: core::cell::Cell
//! [`RefCell`]: core::cell::RefCell
//! [`Rc`]: std::rc::Rc
//!
//! # What lives where
//!
//! | Module | Contents |
//! |---|---|
//! | [`id`] | [`NodeKey`], [`NodeIndex`], [`OptIndex`] and the opaque identities other engines key by |
//! | [`node`] | the record, the handle, the link chains, the flag words and the discipline |
//! | [`arena`] | the store, the columns, the class pool and the once-per-frame recycle |
//! | [`side`] | the value type of every column |
//! | [`text`] | text content and the element a text node inherits from |
//! | [`dirty`] | the invalidation lattice at the path its consumers already use |
//! | [`stylo`] | the style engine's DOM traits, implemented over [`Node`] |
//! | [`host`] | the hooks a consumer implements to put a document language on top |
//! | [`mutate`] | changing a document, and everything a change owes the style engine |
//!
//! ```
//! use zgui_dom::{Document, NodeKind};
//! use zgui_interned::{ClassName, ElementName};
//!
//! let mut document = Document::new();
//! let root = document.append(document.document_index(), NodeKind::Element, ElementName::new("root"));
//! let item = document.append(root, NodeKind::Element, ElementName::new("item"));
//! document.append(root, NodeKind::Text, ElementName::new("#text"));
//! let last = document.append(root, NodeKind::Element, ElementName::new("item"));
//! document.set_classes(item, &[ClassName::new("selected")]);
//!
//! // The element-only chain skips the text node, which is what keeps sibling combinators O(1).
//! let store = document.store();
//! assert_eq!(store.core(item).next_element(), Some(last));
//! assert_eq!(store.classes_of(item).len(), 1);
//! ```

#![deny(missing_docs)]
// The record holds a raw back-pointer to the store that owns it, which is what keeps a handle one
// word wide, and the store is owned through a pointer so that pointer stays valid while the
// document is moved. Those two facts, and the `Send`/`Sync` promises they force, are what this
// crate needs unsafe for. Every use states what it rests on.
#![allow(unsafe_code)]

pub mod arena;
pub mod dirty;
pub mod host;
pub mod id;
pub mod mutate;
pub mod node;
pub mod side;
pub mod stylo;
pub mod text;

pub use crate::arena::columns::Columns;
pub use crate::arena::document::Document;
pub use crate::arena::store::DocumentStore;
pub use crate::dirty::{Dirty, DirtyCell, DirtyChildren};
pub use crate::host::{
    HostSeams, Intrinsic, LinkResolver, PresentationalHints, ReplacedContent, ReplacedId,
    SheetLoader, SheetRequest,
};
pub use crate::id::{DocumentId, NodeIndex, NodeKey, OptIndex};
pub use crate::mutate::{Edit, EverythingMatters, Poisoned, SnapshotStore, StyleFilter};
pub use crate::node::discipline::CellDisciplined;
pub use crate::node::inner::NodeInner;
pub use crate::node::kind::NodeKind;
pub use crate::node::{Node, NodeFlags};

/// Statically requires `T` to be safe to share across threads.
///
/// This is the shape of the assertion the store makes about itself, exported so that a caller
/// adding a side table of its own can make the same one — and so that the rejection cases can be
/// written as compile-fail tests against the real function rather than against a copy of it.
///
/// ```
/// zgui_dom::assert_sync::<zgui_dom::DocumentStore>();
/// ```
///
/// ```compile_fail
/// struct Handlers(Vec<std::rc::Rc<dyn Fn()>>);
/// zgui_dom::assert_sync::<Handlers>();
/// ```
pub const fn assert_sync<T: Sync>() {}
