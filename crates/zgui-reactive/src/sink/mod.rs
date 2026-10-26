//! Sinks: the cheapest reactive node, for a closure that only writes outside the graph.

mod node;
mod slots;

pub(crate) use slots::{clear, drain, release_deferred};

use core::marker::PhantomData;

/// A reactive node that runs one closure again whenever a source it read changes.
///
/// A sink is the end of a dependency chain. It reads signals and memos, writes somewhere outside
/// the graph, and nothing reads it. A binding from a value to one property of one element is the
/// typical use. The first run happens in the constructor, as a [`RenderEffect`]'s does. Later
/// runs happen at [`flush`], after the task pool stalls, in the order the sinks were marked. A
/// sink that reads a memo asks it for its value before it runs, so a memo that recomputes to an
/// equal value does not run the sink.
///
/// A sink keeps one shared node and one slot on the UI thread. It has no owner, no task and no
/// channel, so it costs a fraction of a [`RenderEffect`]. In exchange:
///
/// * The closure returns nothing and gets nothing from its previous run. Keep that state in the
///   closure.
/// * Later runs run under the owner that was current when the sink was made, so a context or a
///   scoped registration resolves as it did in the first run. If that owner is disposed of before
///   the sink, later runs run with no owner.
/// * Nothing is cleaned up between two runs. A cleanup, context, signal or memo that a run
///   creates stays with the owner, once for each run, until the owner is disposed of. Use a
///   [`RenderEffect`] for a closure that creates signals, memos, effects or owners.
/// * In each round of a [`flush`], the task pool runs before the sinks. An effect sees what a
///   sink wrote in that flush only from the next flush. Each step from a sink to a task in one
///   chain uses one of the flush's rounds.
/// * [`Owner::pause`](crate::Owner::pause) does not stop a sink.
///
/// Dropping the handle stops the sink and frees its slot. A run that panics stops the sink for
/// good, and the panic reaches the caller of [`flush`]; the next flush runs the other sinks.
///
/// ```
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, Sink, flush, install};
///
/// install().unwrap();
/// let node = Mounted::new();
/// let seen = Rc::new(Cell::new(0));
/// let (source, sink) = node.with(|| {
///     let source = RwSignal::new(1);
///     let seen = Rc::clone(&seen);
///     (source, Sink::new(move || seen.set(source.get())))
/// });
/// assert_eq!(seen.get(), 1);
///
/// source.set(2);
/// assert_eq!(seen.get(), 1);
/// flush();
/// assert_eq!(seen.get(), 2);
///
/// drop(sink);
/// node.unmount();
/// ```
///
/// [`RenderEffect`]: crate::RenderEffect
/// [`flush`]: crate::flush
#[must_use = "a sink stops when it is dropped"]
pub struct Sink {
    /// The slot.
    key: slots::Key,
    /// A sink lives in its thread's slots, so the handle stays on that thread.
    _thread: PhantomData<*const ()>,
}

impl Sink {
    /// Runs `run` now, with the sink as the observer, and again whenever a source it read
    /// changes.
    ///
    /// # Panics
    ///
    /// In debug builds, if called off the thread that installed the runtime. In any build, if the
    /// first run panics; the sink is then freed.
    pub fn new(run: impl FnMut() + 'static) -> Self {
        crate::executor::assert_ui_thread("Sink::new");
        Self {
            key: slots::create(run),
            _thread: PhantomData,
        }
    }

    /// The number of live sinks on this thread.
    ///
    /// Diagnostic. A count that grows while the view stays the same means sinks are being kept
    /// after the view that made them is gone.
    #[must_use]
    pub fn live() -> usize {
        slots::live()
    }
}

impl Drop for Sink {
    fn drop(&mut self) {
        slots::release(self.key);
    }
}

impl core::fmt::Debug for Sink {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_tuple("Sink").field(&self.key).finish()
    }
}

#[cfg(test)]
mod tests;
