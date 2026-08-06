//! Asynchronous values, and the actions that produce them.
//!
//! Every future here runs as a task on the UI thread and is polled by [`flush`](crate::flush). A
//! future that blocks blocks the frame, so work that genuinely takes time goes through
//! [`background`](crate::background) or [`blocking`](crate::blocking), which run it elsewhere and
//! resolve back here.
//!
//! Where that goes is worth being careful about, because both types below take a closure that is
//! re-run when its reactive inputs change, and *the reads that register those inputs happen in the
//! closure body*. Moving the body wholesale off the UI thread moves the reads with it, and a read
//! that happens on a worker subscribes to nothing: the value is computed once and then never
//! again, with nothing to see in a debugger.
//!
//! So read first, and hand the work what it needs:
//!
//! ```no_run
//! # use zgui_reactive::{AsyncDerived, RwSignal, background};
//! # use zgui_reactive::prelude::*;
//! # async fn look_up(id: u32) -> String { String::new() }
//! # fn example(id: RwSignal<u32>) {
//! AsyncDerived::new(move || {
//!     let id = id.get(); // tracked, on the UI thread
//!     background(async move { look_up(id).await }) // and only the work moves
//! });
//! # }
//! ```

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
