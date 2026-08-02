//! Derived values that recompute only when their inputs actually change.

/// A cached derived value that notifies its subscribers only when the result differs.
///
/// The difference from a plain closure is the comparison: a memo over `count.get() > 10` re-runs
/// on every write to `count`, but wakes nothing while the answer stays `false`. That is what
/// keeps a cheap predicate from restyling a subtree on every keystroke.
///
/// Requires the value to be shareable across threads. For a derived value that is not, derive a
/// local signal instead — see [`Signal`](crate::Signal).
pub use reactive_graph::computed::Memo;

/// The reference-counted form of [`Memo`], freed when the last handle is dropped.
pub use reactive_graph::computed::ArcMemo;

/// Splits one field of a larger signal into a read half and a write half.
///
/// The idiomatic way to hand a component exactly the slice of state it needs: readers wake only
/// when that field changes, and the writer updates it in place without replacing the whole
/// value.
pub use reactive_graph::computed::create_slice;
