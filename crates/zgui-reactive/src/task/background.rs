//! Work that must not happen on the UI thread, and the seam it happens on instead.
//!
//! The frame is a budget. A task on the UI thread is polled inside [`flush`](crate::flush), so a
//! future that spends ten milliseconds parsing spends them between the input event and the pixels,
//! and the window drops the frame. Anything that takes real time belongs on a thread of its own.
//!
//! [`background`] and [`blocking`] are that thread, wrapped so a caller never sees it: both return
//! a future, so the whole round trip is one `await` inside an ordinary UI task, and the value
//! comes back *on the UI thread* where signals, the document and view state are all legal again.
//!
//! ```no_run
//! # use zgui_reactive::{background, spawn};
//! # use zgui_reactive::prelude::*;
//! # fn example(rows: zgui_reactive::RwSignal<Vec<String>>) {
//! # fn load() -> Vec<String> { Vec::new() }
//! spawn(async move {
//!     let loaded = background(async { load() }).await; // off the UI thread
//!     rows.set(loaded); // back on it
//! });
//! # }
//! ```
//!
//! Which executor runs that work is a seam. The default is a small pool started the first time
//! anything asks for it, so a program that never spawns background work never starts a thread;
//! `zgui-tokio` replaces it with a tokio runtime, which is what makes tokio's own timers, sockets
//! and the libraries built on them usable.

use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use futures::channel::oneshot;
use thiserror::Error;

/// Where work that is not allowed to hold the frame actually runs.
///
/// One implementation is installed per process. The default is a small thread pool; `zgui-tokio`
/// provides one over a tokio runtime, and an application with its own executor can provide a third.
///
/// Both methods are called from the UI thread and must not block it.
pub trait BackgroundSpawner: Send + Sync + 'static {
    /// Runs `future` to completion somewhere that is not the UI thread.
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>);

    /// Runs `work` somewhere that is not the UI thread, where blocking is allowed.
    ///
    /// Separate from [`spawn`](BackgroundSpawner::spawn) because a runtime that multiplexes many
    /// futures onto few threads needs to know that this one will not yield.
    fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send>);
}

/// Why a background executor could not be installed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SpawnerError {
    /// Background work has already been handed to the executor that was installed before.
    ///
    /// Installing now would leave the work already in flight running on one executor while
    /// everything after it ran on another — two thread pools, and results arriving from both.
    /// Install before the first [`background`] or [`blocking`] call, which in practice means
    /// before the first window opens.
    #[error("a background executor is already running work; install one before the first task")]
    AlreadyRunning,
}

/// The installed executor, or `None` until something asks for one.
static SPAWNER: RwLock<Option<Arc<dyn BackgroundSpawner>>> = RwLock::new(None);

/// Whether any work has been handed to the installed executor yet.
static USED: AtomicBool = AtomicBool::new(false);

/// Routes background work to `spawner` for the rest of the process.
///
/// Call once, before the first background task. `zgui-tokio` calls this for you.
///
/// # Errors
///
/// [`SpawnerError::AlreadyRunning`] if background work has already been given to whichever
/// executor was in place before — including the default pool, which installs itself on first use.
pub fn set_background_spawner(spawner: Arc<dyn BackgroundSpawner>) -> Result<(), SpawnerError> {
    let mut installed = SPAWNER
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if USED.load(Ordering::Acquire) {
        return Err(SpawnerError::AlreadyRunning);
    }
    *installed = Some(spawner);
    Ok(())
}

/// The installed executor, starting the default pool if nothing else claimed the slot.
fn spawner() -> Arc<dyn BackgroundSpawner> {
    {
        let installed = SPAWNER
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(spawner) = installed.as_ref() {
            USED.store(true, Ordering::Release);
            return Arc::clone(spawner);
        }
    }

    let mut installed = SPAWNER
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    // Another thread may have installed one between the two locks.
    let spawner = installed.get_or_insert_with(|| Arc::new(super::threads::Threads::start()));
    USED.store(true, Ordering::Release);
    Arc::clone(spawner)
}

/// Runs `future` off the UI thread, and resolves on the UI thread with what it produced.
///
/// The awaiting task stays on the UI thread throughout: only `future` moves. When it finishes, the
/// result crosses back through the executor's wake edge, which asks the platform for the frame
/// that delivers it — so a caller writes one `await` and never sees a thread.
///
/// `future` must be `Send`, and so must its output, because both cross a thread boundary. A future
/// that touches the document, a node handle or a view is neither, and belongs in
/// [`spawn_local`](crate::spawn_local) on this side of the `await`.
///
/// # Panics
///
/// When polled, if `future` panicked or the background executor was shut down before it finished.
/// A panicking background task is not swallowed here for the same reason
/// [`flush`](crate::flush) does not swallow a panicking reactive task.
pub fn background<T: Send + 'static>(
    future: impl Future<Output = T> + Send + 'static,
) -> Background<T> {
    let (send, receive) = oneshot::channel();
    spawner().spawn(Box::pin(async move {
        let value = future.await;
        // The receiver is gone when the task awaiting this was cancelled, which is ordinary.
        let _ = send.send(value);
    }));
    Background { receive }
}

/// Runs `work` off the UI thread, where blocking is allowed, and resolves on the UI thread.
///
/// The variant for synchronous work that has no `async` form: decoding an image, parsing a large
/// file, a database call through a blocking driver. [`background`] is for work that is already a
/// future.
///
/// # Panics
///
/// When polled, if `work` panicked or the background executor was shut down before it finished.
pub fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Background<T> {
    let (send, receive) = oneshot::channel();
    spawner().spawn_blocking(Box::new(move || {
        let _ = send.send(work());
    }));
    Background { receive }
}

/// A value being computed off the UI thread, awaited on it.
///
/// Returned by [`background`] and [`blocking`]. Dropping it before it resolves abandons the
/// result; it does not stop the work, which has no way to be interrupted once it is running on
/// another thread.
#[must_use = "a background future does nothing until it is awaited"]
pub struct Background<T> {
    /// The result, once the worker has sent it.
    receive: oneshot::Receiver<T>,
}

impl<T> Future for Background<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<T> {
        match Pin::new(&mut self.receive).poll(cx) {
            Poll::Ready(Ok(value)) => Poll::Ready(value),
            Poll::Ready(Err(oneshot::Canceled)) => panic!(
                "a background task did not produce a value: it panicked, or the background \
                 executor was shut down while it was still running"
            ),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> core::fmt::Debug for Background<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Background")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installing_after_work_has_started_is_refused() {
        // Starts the default pool, which is what makes the slot "in use".
        let done = blocking(|| 1_u8);
        futures::executor::block_on(async {
            assert_eq!(done.await, 1);
        });

        struct Nowhere;
        impl BackgroundSpawner for Nowhere {
            fn spawn(&self, _future: Pin<Box<dyn Future<Output = ()> + Send>>) {}
            fn spawn_blocking(&self, _work: Box<dyn FnOnce() + Send>) {}
        }

        let refused = set_background_spawner(Arc::new(Nowhere));
        assert!(
            matches!(refused, Err(SpawnerError::AlreadyRunning)),
            "installing over an executor that already has work must not split the work in two"
        );
    }

    #[test]
    fn background_work_produces_its_value() {
        let sum = background(async { (1..=10).sum::<u32>() });
        assert_eq!(futures::executor::block_on(sum), 55);
    }
}
