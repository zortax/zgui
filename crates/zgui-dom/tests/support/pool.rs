//! Worker pools, and marking a thread as one.
//!
//! The engine ships a process-global pool and caps itself at six workers however many cores the
//! machine has, but the traversal entry point takes a pool by reference, so a pool of any width can
//! be handed to it.
//!
//! A worker has to be marked as one before it touches element data: several of the engine's own
//! assertions are written against that thread-local flag, and a pool built without the start handler
//! trips them in debug builds.

use rayon::{ThreadPool, ThreadPoolBuilder};
use style::thread_state::{self, ThreadState};

/// The stack size the engine gives its own workers.
const STACK_BYTES: usize = 512 * 1024;

/// A pool of `threads` workers, each marked as a style worker.
///
/// # Panics
///
/// Panics if the pool cannot be built.
pub(crate) fn build(threads: usize) -> ThreadPool {
    ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|index| format!("zgui-style-{index}"))
        .start_handler(|_| thread_state::initialize_layout_worker_thread())
        .exit_handler(|_| ())
        .stack_size(STACK_BYTES)
        .build()
        .expect("a worker pool of the requested width")
}

/// Clears the layout mark when it goes out of scope, panic or no panic.
struct LayoutMark;

impl Drop for LayoutMark {
    fn drop(&mut self) {
        thread_state::exit(ThreadState::LAYOUT);
    }
}

/// Marks the calling thread as the one driving layout, for the duration of `body`.
///
/// Removed on the way out even if `body` unwinds. Leaving it set poisons the thread: the next call
/// finds the flag already there and trips the engine's re-entry assertion, turning one caught panic
/// into an unrelated failure much later.
pub(crate) fn as_layout_thread<T>(body: impl FnOnce() -> T) -> T {
    thread_state::enter(ThreadState::LAYOUT);
    let _mark = LayoutMark;
    body()
}
