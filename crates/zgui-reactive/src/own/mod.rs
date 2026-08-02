//! Ownership: what a mounted node allocates, and when it is freed.
//!
//! Every reactive value belongs to the [`Owner`] that was current when it was created. An owner
//! is disposed of exactly once, and disposing of it frees its children, runs its cleanups and
//! drops its stored values — in that order, synchronously, before the call returns.
//!
//! Three types cover everything a UI needs from that:
//!
//! * [`Mounted`] — one scope per mounted node, the unit of "this thing went away";
//! * [`Scope`] — a parent for a changing set of siblings, which stays flat as they churn;
//! * [`on_cleanup_local`] — cancellation attached to the current scope, for closures that
//!   capture things that cannot cross threads.
//!
//! An owner that is never disposed of leaks permanently, and a value created with *no* current
//! owner leaks immediately and silently. [`assert_owner`](crate::assert_owner) is the guard.

mod cleanup;
mod mounted;
mod scope;

pub use cleanup::on_cleanup_local;
pub use mounted::Mounted;
pub use scope::Scope;

/// The scope a reactive value belongs to.
///
/// Prefer [`Mounted`], which pairs an owner with the node whose lifetime it follows. Reach for
/// the owner itself only for the few APIs that name one.
///
/// Note that this type's own `on_cleanup` requires a closure that can cross threads, which a
/// closure capturing anything from a view cannot; [`on_cleanup_local`] is the one to use.
pub use reactive_graph::owner::Owner;

/// A value stored in the reactive arena rather than in a signal.
///
/// Cheap to copy and to pass into a closure, and freed with the owner that created it. Unlike a
/// signal it is not tracked, so writing it re-runs nothing — which is exactly what is wanted for
/// a cached handle, a generation counter or a piece of imperative state a view needs to keep
/// between renders.
pub use reactive_graph::owner::StoredValue;
