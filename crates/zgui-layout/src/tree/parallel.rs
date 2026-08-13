//! The worker pool the layout engine distributes independent work across.
//!
//! Separate from the style cascade's pool: that one runs stylo's per-worker initialisation and
//! is capped at the width stylo's worker storage supports, and neither property belongs to
//! layout work. The two pools never run at the same time — a frame restyles, then lays out — so
//! holding both costs idle stacks and nothing else.

use std::sync::Arc;

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
    /// The workers.
    pool: rayon::ThreadPool,
}

impl LayoutPool {
    /// A pool of `threads` workers, clamped to what the engine benefits from.
    ///
    /// # Panics
    ///
    /// If the operating system refuses to spawn threads at all.
    pub fn new(threads: usize) -> Arc<Self> {
        let width = threads.clamp(1, MAX_LAYOUT_THREADS);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(width)
            .thread_name(|index| format!("zgui-layout-{index}"))
            .build()
            .expect("the platform can spawn layout workers");
        Arc::new(Self { pool })
    }

    /// How many workers the pool holds.
    pub fn width(&self) -> usize {
        self.pool.current_num_threads()
    }

    /// Runs `work` with a scope that may spawn onto the pool.
    pub fn scope<'scope, R: Send>(
        &self,
        work: impl FnOnce(&rayon::Scope<'scope>) -> R + Send,
    ) -> R {
        self.pool.scope(work)
    }

    /// Maps `items` across the workers, preserving order.
    pub fn map<T: Send, R: Send>(
        &self,
        items: Vec<T>,
        work: impl Fn(T) -> R + Send + Sync,
    ) -> Vec<R> {
        use rayon::prelude::*;
        self.pool
            .install(|| items.into_par_iter().map(work).collect())
    }
}
