//! The effect the view layer is built on.

/// An effect whose first run happens immediately, synchronously, in its constructor, and which
/// stops when the handle is dropped.
///
/// Both properties are what a view needs. The synchronous first run means building a dynamic
/// part of a view produces a real result to hand back to its parent rather than a hole to be
/// filled on the next poll. The drop-cancels lifetime means a piece of view state that is
/// discarded stops re-running at that moment, rather than surviving until its owner is
/// disposed of.
///
/// Later runs happen at [`flush`](crate::flush), never before: writing a signal marks the
/// effect and wakes its task, and the frame runs it. The closure receives what the previous run
/// returned, which is how a view keeps the state it needs to update in place instead of
/// rebuilding.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RenderEffect, RwSignal, flush, install};
///
/// install().unwrap();
/// let node = Mounted::new();
/// let (source, effect) = node.with(|| {
///     let source = RwSignal::new(1);
///     let effect = RenderEffect::new(move |previous: Option<i32>| {
///         previous.unwrap_or_default() + source.get()
///     });
///     (source, effect)
/// });
///
/// source.set(10);
/// flush();
/// assert_eq!(effect.with_value_mut(|total| *total), Some(11));
/// ```
///
/// The handle must be stored: dropping it immediately cancels the effect, which is why it
/// carries a `must_use`.
pub use reactive_graph::effect::RenderEffect;
