//! The background executor an application gets without choosing one.
//!
//! Small, and started late. A program that never calls [`background`](crate::background) or
//! [`blocking`](crate::blocking) never reaches this file, so the cost of having a default is a
//! type and no threads at all. The first call starts the pool; there is no second start.
//!
//! It is deliberately modest — at most four threads — because this is the executor for a program
//! that did not ask for one. A program with real concurrency to run installs `zgui-tokio` or a
//! [`BackgroundSpawner`] of its own and gets that runtime's sizing, work stealing and, for
//! blocking work, its separate blocking pool.

use std::future::Future;
use std::pin::Pin;

use futures::executor::ThreadPool;

use crate::task::background::BackgroundSpawner;

/// The most threads the default pool will start.
///
/// A ceiling rather than a target. Background work here is the odd decode or parse in a program
/// whose real workload is a frame loop, and a pool sized to the machine would take threads from
/// the style engine's rayon pool, which is where this framework's actual parallelism lives.
const MAX_THREADS: usize = 4;

/// A handful of worker threads, shared by every background task in the process.
pub(crate) struct Threads {
    /// The pool. Its threads live until the process ends.
    pool: ThreadPool,
}

impl Threads {
    /// Starts the pool.
    ///
    /// # Panics
    ///
    /// If the operating system refuses to start a thread. There is no useful fallback: running
    /// the work inline instead would run it on the UI thread, which is the one thing every caller
    /// of [`background`](crate::background) has asked not to happen, and it would do it silently.
    pub(crate) fn start() -> Self {
        let size = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .clamp(1, MAX_THREADS);
        let pool = ThreadPool::builder()
            .pool_size(size)
            .name_prefix("zgui-worker-")
            .create()
            .expect("the operating system allowed a background worker thread to start");
        tracing::debug!(threads = size, "started the default background executor");
        Self { pool }
    }
}

impl BackgroundSpawner for Threads {
    fn spawn(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
        self.pool.spawn_ok(future);
    }

    /// Runs `work` on a pool thread, which it holds until it returns.
    ///
    /// The pool has no separate blocking half, so blocking work and asynchronous work compete for
    /// the same few threads: four concurrent blocking calls stall every background future behind
    /// them. That is the ceiling of the default, and the reason a program that leans on blocking
    /// work should install a runtime with a blocking pool of its own.
    fn spawn_blocking(&self, work: Box<dyn FnOnce() + Send>) {
        self.pool.spawn_ok(async move { work() });
    }
}
