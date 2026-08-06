//! tokio channels, as reactive state.
//!
//! None of this needs a runtime. `tokio::sync` is runtime-agnostic — `mpsc`, `watch`, `broadcast`
//! and `oneshot` can all be awaited on zgui's UI-thread pool with nothing installed at all — so
//! these are conveniences over [`zgui_reactive::spawn_local`], not a bridge across
//! anything. They are here because this is the crate an application reaches for when it has tokio
//! channels to display, and because each one gets the cancellation right: the task driving a
//! receiver belongs to the scope that started it, so a subscription does not outlive the view that
//! is showing it.

use tokio::sync::{broadcast, mpsc, watch};
use zgui_reactive::prelude::*;
use zgui_reactive::{ReadSignal, Task, signal, spawn_local};

/// Calls `on_item` on the UI thread for everything sent to `receiver`.
///
/// The task ends when every sender is dropped, and is cancelled when the scope that started it
/// goes away.
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn spawn_receiver<T: 'static>(
    mut receiver: mpsc::Receiver<T>,
    mut on_item: impl FnMut(T) + 'static,
) -> Task {
    spawn_local(async move {
        while let Some(item) = receiver.recv().await {
            on_item(item);
        }
    })
}

/// Calls `on_change` on the UI thread with the current value, and again on every change.
///
/// Called once immediately with what the channel already holds, so a view has a value to render
/// from the first frame rather than after the first change.
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn spawn_watch<T: Clone + Send + Sync + 'static>(
    mut receiver: watch::Receiver<T>,
    mut on_change: impl FnMut(T) + 'static,
) -> Task {
    spawn_local(async move {
        loop {
            // Cloned out and the borrow released before the await: a watch guard held across one
            // blocks every sender for as long as the UI is parked, which is most of the time.
            let value = receiver.borrow_and_update().clone();
            on_change(value);
            if receiver.changed().await.is_err() {
                break; // every sender is gone; the value can never change again
            }
        }
    })
}

/// A signal holding whatever `receiver` last saw.
///
/// The reading half only: nothing but the channel writes it. It starts at the value the channel
/// already holds, so there is no placeholder to render and no `Option` to unwrap.
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn watch_signal<T: Clone + Send + Sync + 'static>(
    receiver: watch::Receiver<T>,
) -> ReadSignal<T> {
    let (read, write) = signal(receiver.borrow().clone());
    // `try_set` rather than `set`: the signal is disposed of with the scope that created it, and
    // a change arriving in the same flush as that disposal is ordinary rather than a fault.
    spawn_watch(receiver, move |value| {
        write.try_set(value);
    });
    read
}

/// Calls `on_item` on the UI thread for everything broadcast to `receiver`.
///
/// A receiver that falls behind the channel's capacity loses the messages in between. That is
/// reported through `tracing` at `warn` and the task carries on from the oldest message still
/// held, because the alternative — ending the subscription — turns a slow frame into a view that
/// silently stops updating.
///
/// # Panics
///
/// In debug builds, if called off the UI thread.
#[track_caller]
pub fn spawn_broadcast<T: Clone + 'static>(
    mut receiver: broadcast::Receiver<T>,
    mut on_item: impl FnMut(T) + 'static,
) -> Task {
    spawn_local(async move {
        loop {
            match receiver.recv().await {
                Ok(item) => on_item(item),
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(
                        missed,
                        "a broadcast subscription fell behind and lost messages"
                    );
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}
