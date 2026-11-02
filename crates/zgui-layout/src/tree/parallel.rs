//! The worker pool the layout engine distributes independent work across.
//!
//! Separate from the style cascade's pool: that one runs stylo's per-worker initialisation and
//! is capped at the width stylo's worker storage supports, and neither property belongs to
//! layout work. The two pools never run at the same time — a frame restyles, then lays out — so
//! holding both costs idle stacks and nothing else.
//!
//! The workers start on the first batch or pre-shape that the engine distributes. A document too
//! small to pass the distribution gates never starts them.

use std::sync::{Arc, OnceLock};

/// How many workers the pool will take, whatever the machine offers.
///
/// Layout batches are memory-bound and coarse; past this width the extra workers contend on the
/// caches the batches read more than they add throughput.
const MAX_LAYOUT_THREADS: usize = 8;

/// The layout engine's worker pool.
///
/// One per application. Windows lay out one at a time on the frame thread, so sharing one pool
/// between them contends on nothing.
#[derive(Debug)]
pub struct LayoutPool {
    /// How many workers the pool starts.
    width: usize,
    /// The workers, once the first distributed job asks for them.
    pool: OnceLock<rayon::ThreadPool>,
}

impl LayoutPool {
    /// A pool of `threads` workers, clamped to what the engine benefits from.
    ///
    /// No worker starts here. The first call to [`LayoutPool::scope`] or [`LayoutPool::map`]
    /// starts all of them.
    pub fn new(threads: usize) -> Arc<Self> {
        Arc::new(Self {
            width: threads.clamp(1, MAX_LAYOUT_THREADS),
            pool: OnceLock::new(),
        })
    }

    /// How many workers the pool holds, or starts on its first job.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Whether the workers are running.
    #[cfg(test)]
    fn is_started(&self) -> bool {
        self.pool.get().is_some()
    }

    /// The workers, started on the first call.
    ///
    /// # Panics
    ///
    /// If the operating system refuses to spawn threads at all.
    fn workers(&self) -> &rayon::ThreadPool {
        self.pool.get_or_init(|| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(self.width)
                .thread_name(|index| format!("zgui-layout-{index}"))
                .build()
                .expect("the platform can spawn layout workers")
        })
    }

    /// Runs `work` with a scope that may spawn onto the pool.
    ///
    /// # Panics
    ///
    /// If the operating system refuses to spawn the workers on the first job.
    pub fn scope<'scope, R: Send>(
        &self,
        work: impl FnOnce(&rayon::Scope<'scope>) -> R + Send,
    ) -> R {
        self.workers().scope(work)
    }

    /// Maps `items` across the workers, preserving order.
    ///
    /// # Panics
    ///
    /// If the operating system refuses to spawn the workers on the first job.
    pub fn map<T: Send, R: Send>(
        &self,
        items: Vec<T>,
        work: impl Fn(T) -> R + Send + Sync,
    ) -> Vec<R> {
        use rayon::prelude::*;
        self.workers()
            .install(|| items.into_par_iter().map(work).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::LayoutPool;

    #[test]
    fn the_workers_start_on_the_first_job() {
        let pool = LayoutPool::new(3);
        assert_eq!(pool.width(), 3);
        assert!(!pool.is_started());
        assert_eq!(pool.map(vec![1, 2, 3], |n| n * 2), vec![2, 4, 6]);
        assert!(pool.is_started());
        assert_eq!(pool.scope(|_| rayon::current_num_threads()), 3);
    }
}
