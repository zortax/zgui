//! Asynchronous values, and the actions that produce them.
//!
//! Every future here runs as a task on the UI thread and is polled by
//! [`flush`](crate::flush) — there is no reactor and no worker pool. A future that blocks
//! blocks the frame; work that genuinely takes time belongs on a thread of its own, writing its
//! result into a signal when it finishes, which the wake edge turns into a frame.

/// A derived value computed by a future, re-run when its reactive inputs change.
///
/// Reads before the first result resolves see `None`, so a view can render a placeholder and
/// replace it when the value arrives.
pub use reactive_graph::computed::AsyncDerived;

/// The reference-counted form of [`AsyncDerived`].
pub use reactive_graph::computed::ArcAsyncDerived;

/// Holds the previous value visible while a new asynchronous one is being computed.
///
/// The difference between a list that greys out while it reloads and one that flashes empty.
pub use reactive_graph::transition::AsyncTransition;

/// An asynchronous operation a view can start and observe: pending, result, and how many times
/// it has run.
///
/// For a submit button, a save, a fetch — anything triggered by a person rather than by a
/// change in state.
pub use reactive_graph::actions::Action;

/// The reference-counted form of [`Action`].
pub use reactive_graph::actions::ArcAction;
