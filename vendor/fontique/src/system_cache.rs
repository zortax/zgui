// Copyright 2026 the zgui Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

// ZGUI-PATCH: this whole file is local to zgui.

//! Counters for the on-disk cache of the system font collection.
//!
//! Only the fontconfig backend has the cache. On other platforms the counters stay at zero.

use core::sync::atomic::Ordering;

use crate::AtomicCounter;

pub(crate) static HITS: AtomicCounter = AtomicCounter::new(0);
pub(crate) static MISSES: AtomicCounter = AtomicCounter::new(0);
pub(crate) static STORES: AtomicCounter = AtomicCounter::new(0);
pub(crate) static CONFIG_LOADS: AtomicCounter = AtomicCounter::new(0);

/// What the system font cache did in this process.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SystemCacheCounts {
    /// Collections built from a valid cache file.
    pub hits: u64,
    /// Collections that found no valid cache file.
    pub misses: u64,
    /// Cache files written.
    pub stores: u64,
    /// Fontconfig configurations loaded, with or without the cache.
    pub config_loads: u64,
}

/// The counts of the system font cache since the process started.
pub fn system_cache_counts() -> SystemCacheCounts {
    SystemCacheCounts {
        hits: u64::from(HITS.load(Ordering::Relaxed)),
        misses: u64::from(MISSES.load(Ordering::Relaxed)),
        stores: u64::from(STORES.load(Ordering::Relaxed)),
        config_loads: u64::from(CONFIG_LOADS.load(Ordering::Relaxed)),
    }
}
