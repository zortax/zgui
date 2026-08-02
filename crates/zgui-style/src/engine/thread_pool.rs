//! The workers a restyle may run on, and the ceiling that is a panic rather than a slowdown.
//!
//! The style engine keeps one per-worker slot array whose length is a compile-time constant of its
//! own, and indexes it by the worker's index in the pool it was handed. A pool wider than that
//! array does not run slowly: the seventh worker indexes past the end of it and the traversal
//! panics. So the width is clamped where the pool is built, once, and every throughput figure this
//! framework quotes is a figure at six workers.

use rayon::{ThreadPool, ThreadPoolBuilder};
use style::thread_state::{self, ThreadState};
use zgui_css::MAX_STYLE_THREADS;

/// The stack each worker gets.
///
/// Selector matching and the cascade recurse with the document, and the default worker stack is
/// smaller than a thread's.
const STACK_BYTES: usize = 512 * 1024;

/// A pool of workers a restyle may be handed.
pub struct StylePool {
    /// The pool itself.
    pool: ThreadPool,
    /// How many workers it has.
    width: usize,
}

impl StylePool {
    /// A pool of `threads` workers, clamped to the widest the engine supports.
    ///
    /// # Panics
    ///
    /// Panics if the pool cannot be built.
    pub fn new(threads: usize) -> Self {
        Self::exactly(threads.min(MAX_STYLE_THREADS))
    }

    /// A pool of exactly `threads` workers, clamped by nothing.
    ///
    /// Above the supported width the engine's per-worker storage is indexed out of bounds and the
    /// traversal panics, so this exists to *demonstrate* the ceiling rather than to be used above
    /// it. Everything that styles a document builds its pool with [`StylePool::new`].
    ///
    /// # Panics
    ///
    /// Panics if the pool cannot be built.
    pub fn exactly(threads: usize) -> Self {
        let pool = ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|index| format!("zgui-style-{index}"))
            // The engine asserts that the thread touching per-element data is a style worker, so a
            // pool built without this trips its own assertion before it does any work.
            .start_handler(|_| thread_state::initialize_layout_worker_thread())
            .stack_size(STACK_BYTES)
            .build()
            .expect("a worker pool of the requested width");
        Self {
            width: pool.current_num_threads(),
            pool,
        }
    }

    /// How many workers this pool has.
    pub fn width(&self) -> usize {
        self.width
    }

    /// The pool itself.
    pub(crate) fn pool(&self) -> &ThreadPool {
        &self.pool
    }
}

/// Clears the mark when the scope it guards ends, panic or no panic.
struct LayoutMark;

impl Drop for LayoutMark {
    fn drop(&mut self) {
        thread_state::exit(ThreadState::LAYOUT);
    }
}

/// Marks the calling thread as the one driving a restyle, for the duration of `body`.
///
/// Removed on the way out even when `body` unwinds. A mark left behind poisons the thread: the
/// next restyle finds it already set and trips the engine's re-entry assertion, turning one caught
/// panic into an unrelated failure much later.
pub(crate) fn as_layout_thread<T>(body: impl FnOnce() -> T) -> T {
    thread_state::enter(ThreadState::LAYOUT);
    let _mark = LayoutMark;
    body()
}

#[cfg(test)]
mod tests {
    use zgui_css::MAX_STYLE_THREADS;

    use super::StylePool;

    #[test]
    fn a_pool_is_never_built_wider_than_the_engine_supports() {
        assert_eq!(MAX_STYLE_THREADS, 6);
        assert_eq!(StylePool::new(64).width(), MAX_STYLE_THREADS);
        assert_eq!(StylePool::new(2).width(), 2);
    }
}
