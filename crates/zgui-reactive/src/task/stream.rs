//! Driving a stream into the reactive graph.
//!
//! A channel, a subscription, a websocket and a progress feed are all the same shape once they are
//! a `Stream`: something that produces items on its own schedule, which the interface has to
//! follow. Both functions here run the stream as an ordinary UI-thread task, so each item is
//! delivered inside a flush where writing a signal is legal, and each one is cancelled when the
//! scope that started it goes away — a subscription that outlived its component is a leak with a
//! network connection attached.
//!
//! Nothing here needs a runtime. `futures` channels work as they are, and so do `tokio::sync`'s:
//! `mpsc`, `watch`, `broadcast` and `oneshot` are all runtime-agnostic and can be awaited on this
//! pool with no tokio installed at all. What needs `zgui-tokio` is `tokio::time`, `tokio::net` and
//! the libraries built on them.

use futures::{Stream, StreamExt};
use reactive_graph::traits::Set;

use crate::reexport::{ReadSignal, signal};
use crate::task::{Task, spawn_local};

/// Runs `stream` on the UI thread, calling `on_item` for each item it produces.
///
/// The task ends when the stream does, and is cancelled when the scope that spawned it is
/// disposed of.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, RwSignal, flush, install, spawn_stream};
///
/// install().expect("no other executor is installed");
/// let node = Mounted::new();
///
/// let total = node.with(|| {
///     let total = RwSignal::new(0);
///     spawn_stream(futures::stream::iter([1, 2, 3]), move |n| {
///         total.update(|sum| *sum += n);
///     });
///     total
/// });
///
/// flush();
/// assert_eq!(total.get(), 6);
/// node.unmount();
/// ```
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn spawn_stream<S>(stream: S, mut on_item: impl FnMut(S::Item) + 'static) -> Task
where
    S: Stream + 'static,
{
    spawn_local(async move {
        let mut stream = core::pin::pin!(stream);
        while let Some(item) = stream.next().await {
            on_item(item);
        }
    })
}

/// A signal holding the latest item `stream` has produced, starting at `initial`.
///
/// The reading half only: nothing but the stream writes it. Reads before the first item see
/// `initial`, so a view has something to render from the first frame.
///
/// The task driving the stream belongs to the current scope and is cancelled with it, at which
/// point the signal stops changing — and is itself disposed of, if it was created in that same
/// scope.
///
/// ```
/// use zgui_reactive::prelude::*;
/// use zgui_reactive::{Mounted, flush, install, signal_from_stream};
///
/// install().expect("no other executor is installed");
/// let node = Mounted::new();
///
/// let latest = node.with(|| signal_from_stream("none", futures::stream::iter(["a", "b"])));
/// assert_eq!(latest.get(), "none");
///
/// flush();
/// assert_eq!(latest.get(), "b");
/// node.unmount();
/// ```
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn signal_from_stream<T>(initial: T, stream: impl Stream<Item = T> + 'static) -> ReadSignal<T>
where
    T: Send + Sync + 'static,
{
    let (read, write) = signal(initial);
    // `try_set` rather than `set`: the signal's own scope may be disposed of between the stream
    // producing an item and the task being cancelled, and a stream ending is not a reason to panic.
    spawn_stream(stream, move |item| {
        write.try_set(item);
    });
    read
}
