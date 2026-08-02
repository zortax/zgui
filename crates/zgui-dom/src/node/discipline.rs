//! The rule every field of the node record is declared through.
//!
//! A field of the record is read by several worker threads at once, while the style traversal
//! runs, through nothing but a shared reference. Three field shapes survive that treatment and a
//! fourth does not:
//!
//! 1. **Plain data**, written only while the document is exclusively borrowed, so a traversal only
//!    ever reads it.
//! 2. **[`Cell<T>`] where `T: Copy`** — `get` is a plain load and `set` a plain store, and two
//!    concurrent loads are not a race. Stores happen only between traversals.
//! 3. **An atomic**, for the fields the style engine itself writes from a worker.
//! 4. **[`RefCell`](core::cell::RefCell), which is forbidden.** `borrow` is a non-atomic
//!    read-modify-write of a borrow counter, so two workers reading the *same* field of a shared
//!    ancestor race on that counter even though both accesses are logically reads. Selector
//!    matching reads ancestors and siblings constantly, so this is not a corner case; it is the
//!    normal path.
//!
//! [`CellDisciplined`] is the compiler-checked statement of the rule and [`node_inner!`](crate::node_inner) is how the
//! record is declared, so a field whose type carries no implementation is a compile error at the
//! declaration site rather than a review finding.
//!
//! ```
//! use zgui_dom::CellDisciplined;
//!
//! const fn accepts<T: CellDisciplined>() {}
//! const _: () = accepts::<core::cell::Cell<u32>>();
//! const _: () = accepts::<core::sync::atomic::AtomicU32>();
//! ```
//!
//! ```compile_fail
//! use zgui_dom::CellDisciplined;
//!
//! const fn accepts<T: CellDisciplined>() {}
//! const _: () = accepts::<core::cell::RefCell<Vec<u32>>>();
//! ```
//!
//! [`Cell<T>`]: core::cell::Cell

use core::cell::Cell;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64};

use style::data::ElementDataWrapper;
use zgui_bits::DirtyCell;

use crate::arena::store::DocumentStore;

/// A field type permitted inside the node record.
///
/// # Safety
///
/// Implementors are safe to read concurrently through a shared reference while the style traversal
/// runs: no non-atomic read-modify-write is reachable from a shared method, and any mutation
/// through a shared reference is either a plain store or a single atomic operation.
pub unsafe trait CellDisciplined {}

// SAFETY: shape 2. `Cell::get` is a load and `Cell::set` a store, with no flag in between. The
// `T: Sync` bound is what stops a `Cell<&RefCell<_>>` from smuggling a borrow counter in behind a
// type that is itself disciplined.
unsafe impl<T: Copy + Sync> CellDisciplined for Cell<T> {}

// SAFETY: shape 3. Every operation is one atomic instruction.
unsafe impl CellDisciplined for AtomicU32 {}
// SAFETY: shape 3.
unsafe impl CellDisciplined for AtomicU64 {}
// SAFETY: shape 3.
unsafe impl CellDisciplined for AtomicI32 {}
// SAFETY: shape 3. One atomic word holding a node's own obligations beside the union of its
// subtree's, updated by a single read-modify-write.
unsafe impl CellDisciplined for DirtyCell {}

// SAFETY: the style engine owns this type's access protocol. It is an `UnsafeCell<ElementData>`
// plus, in debug builds only, a borrow token that is itself atomic, and the engine guarantees that
// a single worker owns an element's data for the duration of that element's restyle.
unsafe impl CellDisciplined for ElementDataWrapper {}

// SAFETY: shape 1. The store outlives every record it holds and the pointer is only ever
// dereferenced immutably, so a worker following it reads memory nobody writes.
unsafe impl CellDisciplined for *const DocumentStore {}

/// Declares plain immutable data — shape 1 — as disciplined.
///
/// Each use names a type written only while the document is exclusively borrowed, so a traversal
/// reads it and nothing else touches it. The types are all `Copy` and hold no interior mutability
/// of any kind, which is why one line covers them.
///
/// # Safety
///
/// This expands to an implementation of an unsafe trait, so each use carries that trait's
/// obligation without the keyword that would otherwise announce it: the named type must hold no
/// interior mutability at all, so that every access to it through a shared reference is a plain
/// read. A type with a borrow counter, a reference count or a non-atomic counter inside it named
/// here is a data race the compiler will not catch, because naming it here is what tells the
/// compiler there is nothing to catch.
#[macro_export]
macro_rules! plain_data {
    ($($ty:ty),* $(,)?) => {$(
        // SAFETY: shape 1 — plain `Copy` data with no interior mutability, written only under an
        // exclusive borrow of the document.
        unsafe impl $crate::node::discipline::CellDisciplined for $ty {}
    )*};
}

/// Declares the node record, gating every field type on [`CellDisciplined`].
///
/// The `const _` this emits per field is what turns "no borrow counter in the node record" from a
/// review finding into a compile error at the line that introduces it.
#[macro_export]
macro_rules! node_inner {
    ($(#[$outer:meta])* $vis:vis struct $name:ident {
        $( $(#[$m:meta])* $f:ident : $ty:ty ),* $(,)?
    }) => {
        $(#[$outer])*
        #[repr(C)]
        $vis struct $name { $( $(#[$m])* pub(crate) $f: $ty, )* }

        $( const _: () = {
            const fn disciplined<T: $crate::node::discipline::CellDisciplined>() {}
            disciplined::<$ty>();
        }; )*
    };
}
